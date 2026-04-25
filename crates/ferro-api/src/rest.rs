use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum::body::Body;
use axum::extract::Multipart;
use axum::http::HeaderMap;
use axum::response::Response;
use std::net::IpAddr;
use ferro_auth::{authorize, hash_password, verify_password, AuthContext};
use ferro_core::{
    Content, ContentPatch, ContentQuery, ContentType, ContentTypeId, ContentVersion,
    ContentVersionId, Media, MediaId, MediaKind, NewContent, Page, Permission, Role, RoleId,
    Scope, Site, User, UserId,
};
use ferro_plugin::HookEvent;
use ferro_storage::schema as schema_migrator;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/auth/signup", post(signup))
        .route("/api/v1/auth/change-password", post(change_password))
        .route("/api/v1/auth/totp/setup", post(totp_setup))
        .route("/api/v1/auth/totp/enable", post(totp_enable))
        .route("/api/v1/auth/totp/disable", post(totp_disable))
        .route("/api/v1/auth/totp/login", post(totp_login))
        .route("/api/v1/sites", get(list_sites))
        .route(
            "/api/v1/content/{type_slug}",
            get(list_content).post(create_content),
        )
        .route(
            "/api/v1/content/{type_slug}/{slug}",
            get(get_content).patch(update_content).delete(delete_content),
        )
        .route(
            "/api/v1/content/{type_slug}/{slug}/publish",
            post(publish_content),
        )
        .route(
            "/api/v1/content/{type_slug}/{slug}/versions",
            get(list_versions),
        )
        .route(
            "/api/v1/content/{type_slug}/{slug}/versions/{version_id}/restore",
            post(restore_version),
        )
        .route("/api/v1/types", get(list_types).post(create_type))
        .route(
            "/api/v1/types/{slug}",
            get(get_type).patch(update_type).delete(delete_type),
        )
        .route("/api/v1/users", get(list_users).post(create_user))
        .route(
            "/api/v1/users/{id}",
            get(get_user).patch(update_user).delete(delete_user),
        )
        .route("/api/v1/roles", get(list_roles).post(create_role))
        .route(
            "/api/v1/roles/{id}",
            get(get_role).patch(update_role).delete(delete_role),
        )
        .route("/api/v1/media", get(list_media).post(upload_media))
        .route(
            "/api/v1/media/{id}",
            get(get_media).delete(delete_media),
        )
        .route("/api/v1/media/{id}/raw", get(get_media_raw))
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(state): State<Arc<AppState>>) -> ApiResult<&'static str> {
    state.repo.health().await?;
    Ok("ok")
}

async fn list_sites(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<Site>>> {
    Ok(Json(state.repo.sites().list().await?))
}

#[derive(Debug, Deserialize)]
struct ListParams {
    locale: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
    /// Free-text query. Backends do best-effort substring matching on the
    /// content's JSON payload; refine to tsvector / SurrealDB `search::` in
    /// future hardening passes.
    q: Option<String>,
}

async fn list_content(
    State(state): State<Arc<AppState>>,
    Path(type_slug): Path<String>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<Page<Content>>> {
    let q = ContentQuery {
        type_slug: Some(type_slug),
        locale: params.locale.and_then(|l| l.parse().ok()),
        status: params
            .status
            .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
        page: params.page,
        per_page: params.per_page,
        search: params.q,
        ..Default::default()
    };
    Ok(Json(state.repo.content().list(q).await?))
}

async fn get_content(
    State(state): State<Arc<AppState>>,
    Path((type_slug, slug)): Path<(String, String)>,
) -> ApiResult<Json<Content>> {
    let (_site, _ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    Ok(Json(content))
}

async fn create_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(type_slug): Path<String>,
    Json(body): Json<NewContent>,
) -> ApiResult<Json<Content>> {
    let (site, ty) = resolve_type(&state, &type_slug).await?;
    if body.type_id != ty.id {
        return Err(ApiError::BadRequest(
            "type_id does not match URL type slug".into(),
        ));
    }
    require_write(&auth.ctx, ty.id)?;
    body.validate(&ty)?;
    let created = state.repo.content().create(site.id, body).await?;
    state
        .hooks
        .dispatch(HookEvent::ContentCreated {
            content: created.clone(),
            type_slug: Some(ty.slug.clone()),
        })
        .await;
    Ok(Json(created))
}

async fn update_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((type_slug, slug)): Path<(String, String)>,
    Json(patch): Json<ContentPatch>,
) -> ApiResult<Json<Content>> {
    let (_site, ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    require_write(&auth.ctx, ty.id)?;
    patch.validate(&ty)?;
    let before = content.clone();
    // Snapshot pre-update state so callers can restore later.
    let _ = state
        .repo
        .versions()
        .create(ContentVersion::from_content(&before, Some(auth.user.id), None))
        .await;
    let after = state.repo.content().update(content.id, patch).await?;
    state
        .hooks
        .dispatch(HookEvent::ContentUpdated {
            before: Box::new(before),
            after: Box::new(after.clone()),
            type_slug: Some(ty.slug.clone()),
        })
        .await;
    Ok(Json(after))
}

async fn delete_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((type_slug, slug)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let (site, ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    require_write(&auth.ctx, ty.id)?;
    state.repo.content().delete(content.id).await?;
    state
        .hooks
        .dispatch(HookEvent::ContentDeleted {
            site_id: site.id,
            type_id: ty.id,
            content_id: content.id,
            slug: content.slug,
            type_slug: Some(ty.slug.clone()),
        })
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn publish_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((type_slug, slug)): Path<(String, String)>,
) -> ApiResult<Json<Content>> {
    let (_site, ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    authorize(&auth.ctx, Permission::Publish(Scope::Type { id: ty.id }))
        .map_err(|_| ApiError::Forbidden("publish denied".into()))?;
    // Snapshot pre-publish state.
    let _ = state
        .repo
        .versions()
        .create(ContentVersion::from_content(&content, Some(auth.user.id), None))
        .await;
    let published = state.repo.content().publish(content.id).await?;
    state
        .hooks
        .dispatch(HookEvent::ContentPublished {
            content: published.clone(),
            type_slug: Some(ty.slug.clone()),
        })
        .await;
    Ok(Json(published))
}

#[derive(Debug, Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AuthResponse {
    Tokens(LoginResponse),
    /// Caller has TOTP enabled; redeem `mfa_token` + 6-digit code at
    /// `/api/v1/auth/totp/login` to get real tokens.
    Mfa(MfaChallenge),
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    /// Short-lived bearer JWT (default 12h).
    token: String,
    /// Long-lived opaque refresh token (default 30d). Persist client-side and
    /// exchange via `POST /api/v1/auth/refresh` to rotate the access JWT
    /// without re-prompting for credentials. Rotation invalidates this token.
    refresh_token: String,
    user: User,
}

#[derive(Debug, Serialize)]
struct MfaChallenge {
    mfa_required: bool,
    /// Opaque token (5-minute lifetime) the client redeems with the TOTP
    /// code. Stored in the session store under a `mfa:` prefix to keep it
    /// distinct from refresh tokens.
    mfa_token: String,
}

const JWT_TTL_SECS: i64 = 60 * 60 * 12; // 12 hours
const REFRESH_TTL: time::Duration = time::Duration::days(30);
const MFA_TTL_SECS: i64 = 300; // 5 minutes
const MFA_PREFIX: &str = "mfa:";

async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> ApiResult<Json<AuthResponse>> {
    enforce_auth_rate(&state, &headers)?;
    let (user, _session) = state.auth.login(&body.email, &body.password, None, None).await?;
    if user.totp_secret.is_some() {
        let mfa_token = mint_mfa_challenge(&state, &user).await?;
        return Ok(Json(AuthResponse::Mfa(MfaChallenge {
            mfa_required: true,
            mfa_token,
        })));
    }
    let resp = mint_login(&state, user).await?;
    Ok(Json(AuthResponse::Tokens(resp)))
}

async fn mint_mfa_challenge(state: &AppState, user: &User) -> ApiResult<String> {
    let now = time::OffsetDateTime::now_utc();
    let token = format!("{MFA_PREFIX}{}", ferro_auth::session::new_token());
    let session = ferro_auth::Session {
        token: token.clone(),
        user_id: user.id,
        created_at: now,
        expires_at: now + time::Duration::seconds(MFA_TTL_SECS),
        ip: None,
        user_agent: None,
    };
    state.auth.sessions.put(session).await.map_err(ApiError::Auth)?;
    Ok(token)
}

#[derive(Debug, Deserialize)]
struct RefreshBody {
    refresh_token: String,
}

/// Exchange a refresh token for a fresh access JWT + refresh pair. The old
/// refresh token is invalidated atomically so a leaked refresh can only be
/// used once before the legitimate user notices their next refresh failing.
async fn refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<RefreshBody>,
) -> ApiResult<Json<LoginResponse>> {
    enforce_auth_rate(&state, &headers)?;
    let (session, user) = state
        .auth
        .resolve_session(&body.refresh_token)
        .await
        .map_err(|_| ApiError::Unauthorized)?;
    if !user.active {
        return Err(ApiError::Forbidden("account disabled".into()));
    }
    // One-time use — rotate.
    state
        .auth
        .logout(&session.token)
        .await
        .map_err(ApiError::Auth)?;
    let resp = mint_login(&state, user).await?;
    Ok(Json(resp))
}

/// Mint an access JWT + new refresh token for `user`. Refresh tokens live in
/// the [`SessionStore`]; the access JWT is stateless.
async fn mint_login(state: &AppState, user: User) -> ApiResult<LoginResponse> {
    let role_names: Vec<String> = user.roles.iter().map(|r| r.to_string()).collect();
    let token = state
        .jwt
        .mint(user.id, role_names, JWT_TTL_SECS)
        .map_err(ApiError::Auth)?;

    let now = time::OffsetDateTime::now_utc();
    let refresh = ferro_auth::Session {
        token: ferro_auth::session::new_token(),
        user_id: user.id,
        created_at: now,
        expires_at: now + REFRESH_TTL,
        ip: None,
        user_agent: None,
    };
    state
        .auth
        .sessions
        .put(refresh.clone())
        .await
        .map_err(ApiError::Auth)?;
    Ok(LoginResponse {
        token,
        refresh_token: refresh.token,
        user: user.redacted(),
    })
}

// --- TOTP / 2FA ------------------------------------------------------------

#[derive(Debug, Serialize)]
struct TotpSetupResponse {
    /// Base32 secret. Display once during enrollment — losing it locks the
    /// user out of TOTP without admin reset.
    secret: String,
    /// `otpauth://totp/...` URI; render as a QR code for authenticator apps.
    otpauth_uri: String,
}

#[derive(Debug, Deserialize)]
struct TotpEnableBody {
    secret: String,
    code: String,
}

#[derive(Debug, Deserialize)]
struct TotpDisableBody {
    code: String,
}

#[derive(Debug, Deserialize)]
struct TotpLoginBody {
    mfa_token: String,
    code: String,
}

/// Mint a fresh secret for the calling user. Doesn't persist anything — the
/// caller must call `/auth/totp/enable` with the secret + a verifying code to
/// commit. This two-step keeps the user from locking themselves out by
/// generating a secret they can't actually scan.
async fn totp_setup(auth: AuthUser) -> ApiResult<Json<TotpSetupResponse>> {
    if auth.user.totp_secret.is_some() {
        return Err(ApiError::BadRequest(
            "TOTP already enabled; disable first to rotate".into(),
        ));
    }
    let secret = ferro_auth::totp::generate_secret();
    let uri = ferro_auth::totp::otpauth_uri(&secret, &auth.user.email, "Ferro");
    Ok(Json(TotpSetupResponse { secret, otpauth_uri: uri }))
}

async fn totp_enable(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<TotpEnableBody>,
) -> ApiResult<StatusCode> {
    if !ferro_auth::totp::verify(&body.secret, &body.code, time::OffsetDateTime::now_utc()) {
        return Err(ApiError::BadRequest("invalid TOTP code".into()));
    }
    let mut user = state
        .repo
        .users()
        .get(auth.user.id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    user.totp_secret = Some(body.secret);
    state.repo.users().upsert(user).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn totp_disable(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<TotpDisableBody>,
) -> ApiResult<StatusCode> {
    let secret = auth
        .user
        .totp_secret
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("TOTP not enabled".into()))?;
    if !ferro_auth::totp::verify(secret, &body.code, time::OffsetDateTime::now_utc()) {
        return Err(ApiError::BadRequest("invalid TOTP code".into()));
    }
    let mut user = state
        .repo
        .users()
        .get(auth.user.id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    user.totp_secret = None;
    state.repo.users().upsert(user).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Step 2 of TOTP login. Caller already has email+password (verified via
/// `/login` which returned an `mfa_token`); this endpoint exchanges the
/// challenge token + 6-digit code for a real token pair.
async fn totp_login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<TotpLoginBody>,
) -> ApiResult<Json<LoginResponse>> {
    enforce_auth_rate(&state, &headers)?;
    if !body.mfa_token.starts_with(MFA_PREFIX) {
        return Err(ApiError::Unauthorized);
    }
    let (session, user) = state
        .auth
        .resolve_session(&body.mfa_token)
        .await
        .map_err(|_| ApiError::Unauthorized)?;
    // One-shot: invalidate the challenge regardless of code outcome to deny
    // brute-force on the same token.
    let _ = state.auth.logout(&session.token).await;
    let secret = user
        .totp_secret
        .as_deref()
        .ok_or(ApiError::Unauthorized)?;
    if !ferro_auth::totp::verify(secret, &body.code, time::OffsetDateTime::now_utc()) {
        return Err(ApiError::Unauthorized);
    }
    let resp = mint_login(&state, user).await?;
    Ok(Json(resp))
}

#[derive(Debug, Deserialize, Default)]
struct LogoutBody {
    /// Optional refresh token to revoke. When present the server-side session
    /// is dropped so the token can no longer be exchanged. Omitting it is
    /// allowed — clients just discard the access JWT.
    #[serde(default)]
    refresh_token: Option<String>,
}

async fn logout(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    body: Option<Json<LogoutBody>>,
) -> ApiResult<StatusCode> {
    if let Some(Json(body)) = body {
        if let Some(token) = body.refresh_token {
            // Best-effort revoke. A missing/expired token isn't an error —
            // the caller's intent is "this session is over", which it is.
            let _ = state.auth.logout(&token).await;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
struct MeResponse {
    #[serde(flatten)]
    user: User,
    /// Convenience boolean derived from `totp_secret.is_some()` so clients can
    /// gate UI without ever seeing the secret itself (it's redacted from the
    /// flattened user above).
    totp_enabled: bool,
}

async fn me(auth: AuthUser) -> Json<MeResponse> {
    let totp_enabled = auth.user.totp_secret.is_some();
    Json(MeResponse {
        user: auth.user.redacted(),
        totp_enabled,
    })
}

#[derive(Debug, Deserialize)]
struct SignupBody {
    email: String,
    handle: String,
    password: String,
    #[serde(default)]
    display_name: Option<String>,
}

/// Public signup. Gated by `auth.allow_public_signup` in `ferro.toml`. New
/// users land active with **no roles** — operators must promote them via
/// `PATCH /api/v1/users/{id}` if they need any permissions.
async fn signup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SignupBody>,
) -> ApiResult<Json<AuthResponse>> {
    enforce_auth_rate(&state, &headers)?;
    if !state.options.allow_public_signup {
        return Err(ApiError::Forbidden(
            "public signup disabled; set auth.allow_public_signup = true".into(),
        ));
    }
    if body.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    if state.repo.users().by_email(&body.email).await?.is_some() {
        return Err(ApiError::BadRequest("email already in use".into()));
    }
    let user = User {
        id: UserId::new(),
        email: body.email,
        handle: body.handle,
        display_name: body.display_name,
        password_hash: Some(hash_password(&body.password).map_err(ApiError::Auth)?),
        roles: Vec::new(),
        active: true,
        created_at: time::OffsetDateTime::now_utc(),
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    let saved = state.repo.users().upsert(user).await?;
    let resp = mint_login(&state, saved).await?;
    Ok(Json(AuthResponse::Tokens(resp)))
}

#[derive(Debug, Deserialize)]
struct ChangePasswordBody {
    current_password: String,
    new_password: String,
}

/// Authenticated user rotates their own password. Verifies the current
/// password against the stored argon2id hash, then re-hashes and persists the
/// new one. Does not invalidate other tokens — that requires JWT denylist
/// support, scheduled for a future hardening pass.
async fn change_password(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ChangePasswordBody>,
) -> ApiResult<StatusCode> {
    if body.new_password.len() < 8 {
        return Err(ApiError::BadRequest(
            "new password must be at least 8 characters".into(),
        ));
    }
    let mut user = state
        .repo
        .users()
        .get(auth.user.id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let hash = user
        .password_hash
        .as_deref()
        .ok_or(ApiError::Unauthorized)?;
    if !verify_password(&body.current_password, hash).map_err(ApiError::Auth)? {
        return Err(ApiError::Auth(ferro_auth::AuthError::InvalidCredentials));
    }
    user.password_hash = Some(hash_password(&body.new_password).map_err(ApiError::Auth)?);
    user.password_changed_at = Some(time::OffsetDateTime::now_utc());
    state.repo.users().upsert(user).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resolve_type(state: &AppState, type_slug: &str) -> ApiResult<(Site, ContentType)> {
    let sites = state.repo.sites().list().await?;
    let site = sites.into_iter().next().ok_or(ApiError::NotFound)?;
    let ty = state
        .repo
        .types()
        .by_slug(site.id, type_slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((site, ty))
}

async fn resolve_entry(
    state: &AppState,
    type_slug: &str,
    slug: &str,
) -> ApiResult<(Site, ContentType, Content)> {
    let (site, ty) = resolve_type(state, type_slug).await?;
    let content = state
        .repo
        .content()
        .by_slug(site.id, ty.id, slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((site, ty, content))
}

fn require_write(ctx: &AuthContext, ty: ContentTypeId) -> ApiResult<()> {
    authorize(ctx, Permission::Write(Scope::Type { id: ty }))
        .map_err(|_| ApiError::Forbidden("write denied".into()))
}

fn require_manage_schema(ctx: &AuthContext) -> ApiResult<()> {
    authorize(ctx, Permission::ManageSchema)
        .map_err(|_| ApiError::Forbidden("schema management denied".into()))
}

// --- Content version routes ---

async fn list_versions(
    State(state): State<Arc<AppState>>,
    Path((type_slug, slug)): Path<(String, String)>,
) -> ApiResult<Json<Vec<ContentVersion>>> {
    let (_site, _ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    Ok(Json(state.repo.versions().list(content.id).await?))
}

async fn restore_version(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((type_slug, slug, version_id)): Path<(String, String, String)>,
) -> ApiResult<Json<Content>> {
    let (_site, ty, current) = resolve_entry(&state, &type_slug, &slug).await?;
    require_write(&auth.ctx, ty.id)?;
    let version_id: ContentVersionId = parse_typed_id(&version_id)?;
    let version = state
        .repo
        .versions()
        .get(version_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if version.content_id != current.id {
        return Err(ApiError::BadRequest(
            "version belongs to a different content row".into(),
        ));
    }
    // Snapshot the live row before overwriting it so the restore itself is
    // reversible.
    let _ = state
        .repo
        .versions()
        .create(ContentVersion::from_content(&current, Some(auth.user.id), None))
        .await;
    // Apply via update so storage backends consistently bump `updated_at`.
    let patch = ContentPatch {
        slug: Some(version.slug),
        status: Some(version.status),
        data: Some(version.data),
    };
    let after = state.repo.content().update(current.id, patch).await?;
    Ok(Json(after))
}

// --- Content-type routes ---

async fn list_types(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<ContentType>>> {
    let sites = state.repo.sites().list().await?;
    let Some(site) = sites.into_iter().next() else {
        return Ok(Json(Vec::new()));
    };
    Ok(Json(state.repo.types().list(site.id).await?))
}

async fn get_type(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> ApiResult<Json<ContentType>> {
    let (_site, ty) = resolve_type(&state, &slug).await?;
    Ok(Json(ty))
}

async fn create_type(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut ty): Json<ContentType>,
) -> ApiResult<Json<ContentType>> {
    require_manage_schema(&auth.ctx)?;
    let sites = state.repo.sites().list().await?;
    let site = sites.into_iter().next().ok_or(ApiError::NotFound)?;
    // Force site scoping to the caller's site (prevents client-supplied site_id
    // from pointing at an unrelated tenant).
    ty.site_id = site.id;
    Ok(Json(state.repo.types().upsert(ty).await?))
}

#[derive(Debug, Serialize)]
struct TypeUpdateResponse {
    #[serde(rename = "type")]
    ty: ContentType,
    /// Number of content rows rewritten by the schema migrator.
    rows_migrated: u64,
    /// Field changes that were applied.
    changes: Vec<ferro_core::FieldChange>,
}

async fn update_type(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(new_ty): Json<ContentType>,
) -> ApiResult<Json<TypeUpdateResponse>> {
    require_manage_schema(&auth.ctx)?;
    let (site, old) = resolve_type(&state, &slug).await?;
    if new_ty.id != old.id {
        return Err(ApiError::BadRequest("content-type id cannot change".into()));
    }
    let changes = ContentType::diff(&old, &new_ty);
    let saved = state.repo.types().upsert(new_ty).await?;
    let rows_migrated =
        schema_migrator::apply_changes(&*state.repo, site.id, saved.id, &changes).await?;
    state
        .hooks
        .dispatch(HookEvent::TypeMigrated {
            site_id: site.id,
            type_id: saved.id,
            type_slug: Some(saved.slug.clone()),
            rows_migrated,
            changes: changes.clone(),
        })
        .await;
    Ok(Json(TypeUpdateResponse { ty: saved, rows_migrated, changes }))
}

async fn delete_type(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> ApiResult<StatusCode> {
    require_manage_schema(&auth.ctx)?;
    let (_site, ty) = resolve_type(&state, &slug).await?;
    state.repo.types().delete(ty.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Token-bucket gate for unauthenticated endpoints (login + signup).
///
/// Identifies the caller via `X-Real-IP` / first `X-Forwarded-For` entry, the
/// conventions emitted by reverse proxies. Falls back to a synthetic
/// `0.0.0.0` bucket when no header is present so direct `oneshot` test
/// callers share a single bucket — production deployments should always sit
/// behind a proxy that fills these headers.
fn enforce_auth_rate(state: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    let ip = client_ip(headers).unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    if let Some(retry) = state.auth_rate_limit.check(ip) {
        return Err(ApiError::RateLimited(retry));
    }
    Ok(())
}

fn client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    if let Some(v) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        if let Ok(ip) = v.trim().parse() {
            return Some(ip);
        }
    }
    if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = v.split(',').next() {
            if let Ok(ip) = first.trim().parse() {
                return Some(ip);
            }
        }
    }
    None
}

fn require_manage_users(ctx: &AuthContext) -> ApiResult<()> {
    authorize(ctx, Permission::ManageUsers)
        .map_err(|_| ApiError::Forbidden("user management denied".into()))
}

fn parse_typed_id<T: std::str::FromStr>(s: &str) -> ApiResult<T>
where
    T::Err: std::fmt::Display,
{
    s.parse::<T>()
        .map_err(|e| ApiError::BadRequest(format!("invalid id `{s}`: {e}")))
}

// --- User-management routes ---

#[derive(Debug, Deserialize)]
struct NewUserBody {
    email: String,
    handle: String,
    #[serde(default)]
    display_name: Option<String>,
    /// Plaintext password — hashed with argon2id before storage. Empty string
    /// or absent value creates a passwordless user (e.g. SSO-only).
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    roles: Vec<RoleId>,
    #[serde(default = "default_active")]
    active: bool,
}

fn default_active() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct UserPatchBody {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    handle: Option<String>,
    #[serde(default)]
    display_name: Option<Option<String>>,
    /// Setting to `Some("...")` rotates the password. `None` leaves the hash
    /// untouched.
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    roles: Option<Vec<RoleId>>,
    #[serde(default)]
    active: Option<bool>,
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<User>>> {
    require_manage_users(&auth.ctx)?;
    let users = state
        .repo
        .users()
        .list()
        .await?
        .into_iter()
        .map(User::redacted)
        .collect();
    Ok(Json(users))
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<NewUserBody>,
) -> ApiResult<Json<User>> {
    require_manage_users(&auth.ctx)?;
    if state.repo.users().by_email(&body.email).await?.is_some() {
        return Err(ApiError::BadRequest("email already in use".into()));
    }
    let password_hash = match body.password.as_deref() {
        Some(p) if !p.is_empty() => Some(hash_password(p).map_err(ApiError::Auth)?),
        _ => None,
    };
    let user = User {
        id: UserId::new(),
        email: body.email,
        handle: body.handle,
        display_name: body.display_name,
        password_hash,
        roles: body.roles,
        active: body.active,
        created_at: time::OffsetDateTime::now_utc(),
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    Ok(Json(state.repo.users().upsert(user).await?.redacted()))
}

async fn get_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<User>> {
    require_manage_users(&auth.ctx)?;
    let id: UserId = parse_typed_id(&id)?;
    let user = state.repo.users().get(id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(user.redacted()))
}

async fn update_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(patch): Json<UserPatchBody>,
) -> ApiResult<Json<User>> {
    require_manage_users(&auth.ctx)?;
    let id: UserId = parse_typed_id(&id)?;
    let mut user = state.repo.users().get(id).await?.ok_or(ApiError::NotFound)?;
    if let Some(email) = patch.email {
        user.email = email;
    }
    if let Some(handle) = patch.handle {
        user.handle = handle;
    }
    if let Some(display) = patch.display_name {
        user.display_name = display;
    }
    if let Some(password) = patch.password {
        if !password.is_empty() {
            user.password_hash = Some(hash_password(&password).map_err(ApiError::Auth)?);
        }
    }
    if let Some(roles) = patch.roles {
        user.roles = roles;
    }
    if let Some(active) = patch.active {
        user.active = active;
    }
    Ok(Json(state.repo.users().upsert(user).await?.redacted()))
}

async fn delete_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_manage_users(&auth.ctx)?;
    let id: UserId = parse_typed_id(&id)?;
    if id == auth.user.id {
        return Err(ApiError::BadRequest("cannot delete self".into()));
    }
    state.repo.users().delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Role-management routes ---

#[derive(Debug, Deserialize)]
struct NewRoleBody {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    permissions: Vec<Permission>,
}

#[derive(Debug, Deserialize)]
struct RolePatchBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<Option<String>>,
    #[serde(default)]
    permissions: Option<Vec<Permission>>,
}

async fn list_roles(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<Role>>> {
    require_manage_users(&auth.ctx)?;
    Ok(Json(state.repo.users().list_roles().await?))
}

async fn create_role(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<NewRoleBody>,
) -> ApiResult<Json<Role>> {
    require_manage_users(&auth.ctx)?;
    let role = Role {
        id: RoleId::new(),
        name: body.name,
        description: body.description,
        permissions: body.permissions,
    };
    Ok(Json(state.repo.users().upsert_role(role).await?))
}

async fn get_role(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Role>> {
    require_manage_users(&auth.ctx)?;
    let id: RoleId = parse_typed_id(&id)?;
    let role = state.repo.users().get_role(id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(role))
}

async fn update_role(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(patch): Json<RolePatchBody>,
) -> ApiResult<Json<Role>> {
    require_manage_users(&auth.ctx)?;
    let id: RoleId = parse_typed_id(&id)?;
    let mut role = state.repo.users().get_role(id).await?.ok_or(ApiError::NotFound)?;
    if let Some(name) = patch.name {
        role.name = name;
    }
    if let Some(desc) = patch.description {
        role.description = desc;
    }
    if let Some(perms) = patch.permissions {
        role.permissions = perms;
    }
    Ok(Json(state.repo.users().upsert_role(role).await?))
}

async fn delete_role(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_manage_users(&auth.ctx)?;
    let id: RoleId = parse_typed_id(&id)?;
    state.repo.users().delete_role(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Media routes ----------------------------------------------------------

/// List media for the active site.
async fn list_media(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<Media>>> {
    let _ = auth; // any authenticated caller can browse; refine via RBAC later.
    let sites = state.repo.sites().list().await?;
    let Some(site) = sites.into_iter().next() else {
        return Ok(Json(Vec::new()));
    };
    Ok(Json(state.repo.media().list(site.id).await?))
}

async fn get_media(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Media>> {
    let _ = auth;
    let id: MediaId = parse_typed_id(&id)?;
    let m = state.repo.media().get(id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(m))
}

/// Stream the raw bytes of a media file. Useful for previews; production
/// deployments should configure the media store's `base_url` so clients hit
/// the CDN/object store directly via [`Media`]'s precomputed URL.
async fn get_media_raw(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Response<Body>> {
    let _ = auth;
    let id: MediaId = parse_typed_id(&id)?;
    let meta = state.repo.media().get(id).await?.ok_or(ApiError::NotFound)?;
    let stream = state
        .media
        .get(&meta.key)
        .await
        .map_err(|e| match e {
            ferro_media::MediaError::NotFound => ApiError::NotFound,
            other => ApiError::Media(other),
        })?;
    let body = Body::from_stream(stream);
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, meta.mime.clone())
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", meta.filename),
        )
        .body(body)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(resp)
}

const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024; // 25 MiB

/// Multipart upload. Field `file` carries the bytes; optional `alt` populates
/// `Media.alt` text for images. Stores in the configured media backend, then
/// records metadata in the repo.
async fn upload_media(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> ApiResult<Json<Media>> {
    let sites = state.repo.sites().list().await?;
    let site = sites.into_iter().next().ok_or(ApiError::NotFound)?;

    let mut filename: Option<String> = None;
    let mut mime: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;
    let mut alt: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            filename = field.file_name().map(|s| s.to_string());
            mime = field.content_type().map(|m| m.to_string());
            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::BadRequest(format!("file read: {e}")))?;
            if data.len() > MAX_UPLOAD_BYTES {
                return Err(ApiError::BadRequest(format!(
                    "file exceeds {MAX_UPLOAD_BYTES}-byte limit"
                )));
            }
            bytes = Some(data.to_vec());
        } else if name == "alt" {
            let text: String = field
                .text()
                .await
                .map_err(|e| ApiError::BadRequest(format!("alt text: {e}")))?;
            alt = Some(text);
        }
    }

    let bytes = bytes.ok_or_else(|| ApiError::BadRequest("missing `file` field".into()))?;
    let filename = filename.unwrap_or_else(|| "upload".into());
    let mime = mime
        .or_else(|| Some(mime_guess::from_path(&filename).first_or_octet_stream().to_string()))
        .unwrap();
    let size = bytes.len() as u64;

    let id = MediaId::new();
    // Storage key: `<media-id>/<filename>` keeps human-readable names while
    // ensuring uniqueness without collision-on-rename.
    let safe_name = sanitize_filename(&filename);
    let key = format!("{id}/{safe_name}");

    let body_stream =
        futures::stream::once(async move { Ok::<_, std::io::Error>(bytes::Bytes::from(bytes)) });
    let body: ferro_media::ByteStream = Box::pin(body_stream);
    let media_ref = state.media.put(&key, body, &mime, size).await?;

    let now = time::OffsetDateTime::now_utc();
    let media = Media {
        id,
        site_id: site.id,
        key: media_ref.key.clone(),
        filename,
        mime: mime.clone(),
        size: media_ref.size,
        width: None,
        height: None,
        alt,
        kind: MediaKind::from_mime(&mime),
        uploaded_by: Some(auth.user.id),
        created_at: now,
    };
    let saved = state.repo.media().create(media).await?;
    Ok(Json(saved))
}

async fn delete_media(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_manage_users(&auth.ctx)?;
    let id: MediaId = parse_typed_id(&id)?;
    if let Some(meta) = state.repo.media().get(id).await? {
        // Best-effort blob removal — metadata delete is the source of truth.
        let _ = state.media.delete(&meta.key).await;
    }
    state.repo.media().delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Strip directory traversal from uploaded filenames. Replaces anything
/// non-alphanumeric / non-`.-_` with `_`; collapses consecutive underscores.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.' || c == '_') {
        "upload".into()
    } else {
        cleaned
    }
}
