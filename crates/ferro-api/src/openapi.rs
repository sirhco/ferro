//! OpenAPI 3.0 surface for the REST layer.
//!
//! Spec is built by hand with `utoipa::openapi::*` so handlers stay free of
//! attributes and ferro-core types don't need `ToSchema` impls leaking out of
//! their crate. Served at `/api/openapi.json`.

use std::sync::Arc;

use axum::Router;
use utoipa::openapi::path::{OperationBuilder, Parameter, ParameterBuilder, ParameterIn};
use utoipa::openapi::request_body::{RequestBody, RequestBodyBuilder};
use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Type};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityRequirement, SecurityScheme};
use utoipa::openapi::tag::TagBuilder;
use utoipa::openapi::{
    ComponentsBuilder, ContentBuilder, HttpMethod, InfoBuilder, OpenApi, OpenApiBuilder,
    PathItem, PathsBuilder, Ref, RefOr, Required, Response, ResponseBuilder, ResponsesBuilder,
    Schema,
};

use crate::state::AppState;

#[must_use]
pub fn api_doc() -> OpenApi {
    let info = InfoBuilder::new()
        .title("Ferro API")
        .version("0.1.0")
        .description(Some(
            "Ferro headless CMS — REST surface. GraphQL lives at /graphql.",
        ))
        .build();

    let bearer = SecurityScheme::Http(
        HttpBuilder::new()
            .scheme(HttpAuthScheme::Bearer)
            .bearer_format("JWT")
            .build(),
    );

    let components = ComponentsBuilder::new()
        .schema("LoginBody", login_body_schema())
        .schema("LoginResponse", login_response_schema())
        .schema("MfaChallenge", mfa_challenge_schema())
        .schema("Content", content_schema())
        .schema("ContentPatch", content_patch_schema())
        .schema("ContentType", content_type_schema())
        .schema("FieldDef", field_def_schema())
        .schema("Page", page_schema())
        .schema("Site", site_schema())
        .schema("User", user_schema())
        .schema("Role", role_schema())
        .schema("Media", media_schema())
        .schema("ContentVersion", content_version_schema())
        .schema("CsrfTokenResponse", csrf_token_response_schema())
        .schema(
            "TypeUpdateResponse",
            type_update_response_schema(),
        )
        .schema("PluginInfo", plugin_info_schema())
        .schema("PluginGrantBody", plugin_grant_body_schema())
        .schema("PluginEnabledBody", plugin_enabled_body_schema())
        .security_scheme("bearer", bearer)
        .build();

    let paths = PathsBuilder::new()
        .path(
            "/healthz",
            PathItem::new(
                HttpMethod::Get,
                op("meta", "Liveness probe.", None, text_200(), None::<Vec<Parameter>>),
            ),
        )
        .path(
            "/readyz",
            PathItem::new(
                HttpMethod::Get,
                op(
                    "meta",
                    "Readiness probe (checks storage).",
                    None,
                    text_200(),
                    None::<Vec<Parameter>>,
                ),
            ),
        )
        .path(
            "/api/v1/auth/csrf",
            PathItem::new(
                HttpMethod::Get,
                op(
                    "auth",
                    "Mint a CSRF double-submit token. Sets the `ferro_csrf` \
                     cookie and returns the same value as JSON for SPAs to \
                     mirror in the `X-CSRF-Token` header on mutating calls.",
                    None,
                    json_200_ref("CsrfTokenResponse"),
                    None::<Vec<Parameter>>,
                ),
            ),
        )
        .path(
            "/api/v1/auth/login",
            PathItem::new(
                HttpMethod::Post,
                op(
                    "auth",
                    "Exchange email+password for a short-lived bearer JWT.",
                    Some(json_body_ref("LoginBody")),
                    json_200_ref("LoginResponse"),
                    None::<Vec<Parameter>>,
                ),
            ),
        )
        .path(
            "/api/v1/auth/logout",
            PathItem::new(
                HttpMethod::Post,
                secure(op_builder("auth", "Logout.", None, no_content(), None::<Vec<Parameter>>)).build(),
            ),
        )
        .path(
            "/api/v1/auth/me",
            PathItem::new(
                HttpMethod::Get,
                secure(op_builder(
                    "auth",
                    "Current user.",
                    None,
                    json_200_ref("User"),
                    None::<Vec<Parameter>>,
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/sites",
            PathItem::new(
                HttpMethod::Get,
                op(
                    "content",
                    "List sites.",
                    None,
                    json_200_array_ref("Site"),
                    None::<Vec<Parameter>>,
                ),
            ),
        )
        .path(
            "/api/v1/content/{type_slug}",
            PathItem::new(
                HttpMethod::Get,
                op(
                    "content",
                    "List content entries for a type.",
                    None,
                    json_200_ref("Page"),
                    Some(vec![path_param("type_slug")]),
                ),
            )
            .also_method(
                HttpMethod::Post,
                secure(op_builder(
                    "content",
                    "Create a content entry.",
                    Some(json_body_ref("Content")),
                    json_200_ref("Content"),
                    Some(vec![path_param("type_slug")]),
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/content/{type_slug}/{slug}",
            PathItem::new(
                HttpMethod::Get,
                op(
                    "content",
                    "Get a content entry.",
                    None,
                    json_200_ref("Content"),
                    Some(vec![path_param("type_slug"), path_param("slug")]),
                ),
            )
            .also_method(
                HttpMethod::Patch,
                secure(op_builder(
                    "content",
                    "Patch a content entry.",
                    Some(json_body_ref("ContentPatch")),
                    json_200_ref("Content"),
                    Some(vec![path_param("type_slug"), path_param("slug")]),
                ))
                .build(),
            )
            .also_method(
                HttpMethod::Delete,
                secure(op_builder(
                    "content",
                    "Delete a content entry.",
                    None,
                    no_content(),
                    Some(vec![path_param("type_slug"), path_param("slug")]),
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/content/{type_slug}/{slug}/publish",
            PathItem::new(
                HttpMethod::Post,
                secure(op_builder(
                    "content",
                    "Publish a content entry.",
                    None,
                    json_200_ref("Content"),
                    Some(vec![path_param("type_slug"), path_param("slug")]),
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/types",
            PathItem::new(
                HttpMethod::Get,
                op(
                    "types",
                    "List content types for the active site.",
                    None,
                    json_200_array_ref("ContentType"),
                    None::<Vec<Parameter>>,
                ),
            )
            .also_method(
                HttpMethod::Post,
                secure(op_builder(
                    "types",
                    "Create a content type.",
                    Some(json_body_ref("ContentType")),
                    json_200_ref("ContentType"),
                    None::<Vec<Parameter>>,
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/types/{slug}",
            PathItem::new(
                HttpMethod::Get,
                op(
                    "types",
                    "Get a content type.",
                    None,
                    json_200_ref("ContentType"),
                    Some(vec![path_param("slug")]),
                ),
            )
            .also_method(
                HttpMethod::Patch,
                secure(op_builder(
                    "types",
                    "Update a content type and migrate existing content data.",
                    Some(json_body_ref("ContentType")),
                    json_200_ref("TypeUpdateResponse"),
                    Some(vec![path_param("slug")]),
                ))
                .build(),
            )
            .also_method(
                HttpMethod::Delete,
                secure(op_builder(
                    "types",
                    "Delete a content type.",
                    None,
                    no_content(),
                    Some(vec![path_param("slug")]),
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/plugins",
            PathItem::new(
                HttpMethod::Get,
                secure(op_builder(
                    "plugins",
                    "List installed WASM plugins (requires `manage_plugins`).",
                    None,
                    json_200_array_ref("PluginInfo"),
                    None::<Vec<Parameter>>,
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/plugins/{name}",
            PathItem::new(
                HttpMethod::Get,
                secure(op_builder(
                    "plugins",
                    "Inspect a single plugin.",
                    None,
                    json_200_ref("PluginInfo"),
                    Some(vec![path_param("name")]),
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/plugins/{name}/grant",
            PathItem::new(
                HttpMethod::Post,
                secure(op_builder(
                    "plugins",
                    "Update granted capabilities (in-memory only — persist via ferro.toml `[[plugins.grants]]` to survive restart). Triggers a reload of the affected plugin.",
                    Some(json_body_ref("PluginGrantBody")),
                    json_200_ref("PluginInfo"),
                    Some(vec![path_param("name")]),
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/plugins/{name}/reload",
            PathItem::new(
                HttpMethod::Post,
                secure(op_builder(
                    "plugins",
                    "Re-scan the plugin directory and reload all plugins. (Path `name` is currently unused; full reload is the only supported mode.)",
                    None,
                    no_content(),
                    Some(vec![path_param("name")]),
                ))
                .build(),
            ),
        )
        .path(
            "/api/v1/plugins/{name}/enabled",
            PathItem::new(
                HttpMethod::Post,
                secure(op_builder(
                    "plugins",
                    "Enable or disable a plugin's hook dispatch without unloading it.",
                    Some(json_body_ref("PluginEnabledBody")),
                    json_200_ref("PluginInfo"),
                    Some(vec![path_param("name")]),
                ))
                .build(),
            ),
        )
        .build();

    let tags = vec![
        TagBuilder::new()
            .name("auth")
            .description(Some("Login, logout, session introspection."))
            .build(),
        TagBuilder::new()
            .name("content")
            .description(Some("CRUD over content entries."))
            .build(),
        TagBuilder::new()
            .name("types")
            .description(Some("Content-type schema management + migration."))
            .build(),
        TagBuilder::new()
            .name("meta")
            .description(Some("Health and readiness probes."))
            .build(),
        TagBuilder::new()
            .name("plugins")
            .description(Some(
                "WASM plugin host: list, inspect, grant capabilities, enable/disable, reload.",
            ))
            .build(),
    ];

    OpenApiBuilder::new()
        .info(info)
        .paths(paths)
        .components(Some(components))
        .tags(Some(tags))
        .build()
}

// --- helpers ---

fn opaque_object(description: &str) -> utoipa::openapi::schema::Object {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .description(Some(description.to_string()))
        .build()
}

fn path_param(name: &str) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Path)
        .required(Required::True)
        .schema(Some(
            ObjectBuilder::new().schema_type(Type::String).build(),
        ))
        .build()
}

fn op_builder(
    tag: &str,
    summary: &str,
    body: Option<RequestBody>,
    ok: Response,
    params: Option<Vec<Parameter>>,
) -> OperationBuilder {
    let responses = ResponsesBuilder::new().response("200", ok).build();
    let mut b = OperationBuilder::new()
        .tag(tag)
        .summary(Some(summary.to_string()))
        .responses(responses);
    if let Some(body) = body {
        b = b.request_body(Some(body));
    }
    if let Some(params) = params {
        b = b.parameters(Some(params));
    }
    b
}

fn op(
    tag: &str,
    summary: &str,
    body: Option<RequestBody>,
    ok: Response,
    params: Option<Vec<Parameter>>,
) -> utoipa::openapi::path::Operation {
    op_builder(tag, summary, body, ok, params).build()
}

fn secure(op: OperationBuilder) -> OperationBuilder {
    op.security(SecurityRequirement::new::<&str, [&str; 0], &str>("bearer", []))
}

fn json_body_ref(name: &str) -> RequestBody {
    RequestBodyBuilder::new()
        .content(
            "application/json",
            ContentBuilder::new()
                .schema(Some(RefOr::Ref(Ref::from_schema_name(name))))
                .build(),
        )
        .required(Some(Required::True))
        .build()
}

fn json_200_ref(name: &str) -> Response {
    ResponseBuilder::new()
        .description("ok")
        .content(
            "application/json",
            ContentBuilder::new()
                .schema(Some(RefOr::Ref(Ref::from_schema_name(name))))
                .build(),
        )
        .build()
}

fn json_200_array_ref(name: &str) -> Response {
    let array: RefOr<Schema> = RefOr::T(Schema::Array(
        ArrayBuilder::new()
            .items(RefOr::Ref(Ref::from_schema_name(name)))
            .build(),
    ));
    ResponseBuilder::new()
        .description("ok")
        .content(
            "application/json",
            ContentBuilder::new().schema(Some(array)).build(),
        )
        .build()
}

fn text_200() -> Response {
    ResponseBuilder::new()
        .description("ok")
        .content(
            "text/plain",
            ContentBuilder::new()
                .schema(Some(RefOr::T(Schema::Object(
                    ObjectBuilder::new().schema_type(Type::String).build(),
                ))))
                .build(),
        )
        .build()
}

fn no_content() -> Response {
    ResponseBuilder::new().description("no content").build()
}

// --- Component schemas (hand-built; richer than a derived ToSchema since the
// REST handlers serialize core types verbatim and adding `utoipa::ToSchema`
// derives across `ferro-core` would couple the domain crate to the doc tool) ---

fn str_field() -> ObjectBuilder {
    ObjectBuilder::new().schema_type(Type::String)
}

fn ulid_field() -> ObjectBuilder {
    ObjectBuilder::new()
        .schema_type(Type::String)
        .description(Some("Crockford ULID (26 chars).".to_string()))
}

fn rfc3339_field() -> ObjectBuilder {
    ObjectBuilder::new()
        .schema_type(Type::String)
        .format(Some(utoipa::openapi::SchemaFormat::KnownFormat(
            utoipa::openapi::KnownFormat::DateTime,
        )))
}

fn bool_field() -> ObjectBuilder {
    ObjectBuilder::new().schema_type(Type::Boolean)
}

fn int_field() -> ObjectBuilder {
    ObjectBuilder::new().schema_type(Type::Integer)
}

fn opaque_field(desc: &str) -> ObjectBuilder {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .description(Some(desc.to_string()))
}

fn array_of(name: &str) -> Schema {
    Schema::Array(
        ArrayBuilder::new()
            .items(RefOr::Ref(Ref::from_schema_name(name)))
            .build(),
    )
}

fn login_body_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("email", str_field())
            .property("password", str_field())
            .required("email")
            .required("password")
            .build(),
    )
}

fn login_response_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("token", str_field().description(Some("Short-lived JWT.".to_string())))
            .property(
                "refresh_token",
                str_field().description(Some("Long-lived opaque refresh token.".to_string())),
            )
            .property("user", RefOr::Ref(Ref::from_schema_name("User")))
            .required("token")
            .required("refresh_token")
            .required("user")
            .build(),
    )
}

fn mfa_challenge_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("mfa_required", bool_field())
            .property("mfa_token", str_field())
            .required("mfa_required")
            .required("mfa_token")
            .build(),
    )
}

fn site_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("id", ulid_field())
            .property("slug", str_field())
            .property("name", str_field())
            .property(
                "description",
                str_field().description(Some("Optional.".to_string())),
            )
            .property(
                "primary_url",
                str_field().description(Some("Optional URL.".to_string())),
            )
            .property("locales", Schema::Array(
                ArrayBuilder::new()
                    .items(RefOr::T(Schema::Object(str_field().build())))
                    .build(),
            ))
            .property("default_locale", str_field())
            .property("settings", opaque_field("Free-form site settings"))
            .property("created_at", rfc3339_field())
            .property("updated_at", rfc3339_field())
            .required("id")
            .required("slug")
            .required("name")
            .required("default_locale")
            .required("created_at")
            .required("updated_at")
            .build(),
    )
}

fn field_def_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("id", ulid_field())
            .property("slug", str_field())
            .property("name", str_field())
            .property("help", str_field())
            .property(
                "kind",
                opaque_field(
                    "Tagged union: text/rich_text/number/boolean/date/date_time/enum/reference/media/json/slug",
                ),
            )
            .property("required", bool_field())
            .property("localized", bool_field())
            .property("unique", bool_field())
            .property("hidden", bool_field())
            .required("id")
            .required("slug")
            .required("name")
            .required("kind")
            .build(),
    )
}

fn content_type_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("id", ulid_field())
            .property("site_id", ulid_field())
            .property("slug", str_field())
            .property("name", str_field())
            .property("description", str_field())
            .property("fields", Schema::Array(
                ArrayBuilder::new()
                    .items(RefOr::Ref(Ref::from_schema_name("FieldDef")))
                    .build(),
            ))
            .property("singleton", bool_field())
            .property("title_field", str_field())
            .property("slug_field", str_field())
            .property("created_at", rfc3339_field())
            .property("updated_at", rfc3339_field())
            .required("id")
            .required("site_id")
            .required("slug")
            .required("name")
            .required("created_at")
            .required("updated_at")
            .build(),
    )
}

fn content_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("id", ulid_field())
            .property("site_id", ulid_field())
            .property("type_id", ulid_field())
            .property("slug", str_field())
            .property("locale", str_field())
            .property(
                "status",
                str_field().description(Some("draft | published | archived".to_string())),
            )
            .property("data", opaque_field("Field-keyed object of FieldValue."))
            .property("author_id", ulid_field())
            .property("created_at", rfc3339_field())
            .property("updated_at", rfc3339_field())
            .property("published_at", rfc3339_field())
            .required("id")
            .required("site_id")
            .required("type_id")
            .required("slug")
            .required("locale")
            .required("status")
            .required("data")
            .required("created_at")
            .required("updated_at")
            .build(),
    )
}

fn content_patch_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .description(Some(
                "Partial update — every field is optional and applies only when present.".to_string(),
            ))
            .property("slug", str_field())
            .property("status", str_field())
            .property("data", opaque_field("Field-keyed object of FieldValue."))
            .build(),
    )
}

fn content_version_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("id", ulid_field())
            .property("content_id", ulid_field())
            .property("site_id", ulid_field())
            .property("type_id", ulid_field())
            .property("slug", str_field())
            .property("locale", str_field())
            .property("status", str_field())
            .property("data", opaque_field("Field-keyed snapshot."))
            .property("author_id", ulid_field())
            .property("captured_at", rfc3339_field())
            .property("parent_version", ulid_field())
            .required("id")
            .required("content_id")
            .required("captured_at")
            .build(),
    )
}

fn csrf_token_response_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property(
                "token",
                str_field().description(Some(
                    "Hex token; mirror in the X-CSRF-Token header on mutating calls.".to_string(),
                )),
            )
            .required("token")
            .build(),
    )
}

fn page_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .description(Some("Paginated list envelope.".to_string()))
            .property(
                "items",
                opaque_field("Backend-typed list elements (varies per endpoint)."),
            )
            .property("total", int_field())
            .property("page", int_field())
            .property("per_page", int_field())
            .required("items")
            .required("total")
            .required("page")
            .required("per_page")
            .build(),
    )
}

fn user_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("id", ulid_field())
            .property("email", str_field())
            .property("handle", str_field())
            .property("display_name", str_field())
            .property("roles", Schema::Array(
                ArrayBuilder::new()
                    .items(RefOr::T(Schema::Object(ulid_field().build())))
                    .build(),
            ))
            .property("active", bool_field())
            .property("created_at", rfc3339_field())
            .property("last_login", rfc3339_field())
            .property("password_changed_at", rfc3339_field())
            .required("id")
            .required("email")
            .required("handle")
            .required("active")
            .required("created_at")
            .build(),
    )
}

fn role_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("id", ulid_field())
            .property("name", str_field())
            .property("description", str_field())
            .property(
                "permissions",
                opaque_field("Tagged union of Read/Write/Publish/ManageUsers/ManageSchema/Admin."),
            )
            .required("id")
            .required("name")
            .required("permissions")
            .build(),
    )
}

fn media_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("id", ulid_field())
            .property("site_id", ulid_field())
            .property("key", str_field())
            .property("filename", str_field())
            .property("mime", str_field())
            .property("size", int_field())
            .property("width", int_field())
            .property("height", int_field())
            .property("alt", str_field())
            .property(
                "kind",
                str_field().description(Some("image|video|audio|document|other".to_string())),
            )
            .property("uploaded_by", ulid_field())
            .property("created_at", rfc3339_field())
            .required("id")
            .required("site_id")
            .required("key")
            .required("filename")
            .required("mime")
            .required("size")
            .required("kind")
            .required("created_at")
            .build(),
    )
}

fn type_update_response_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("type", RefOr::Ref(Ref::from_schema_name("ContentType")))
            .property(
                "rows_migrated",
                int_field().description(Some("Number of rows the schema migrator rewrote.".to_string())),
            )
            .property("changes", Schema::from(opaque_field("FieldChange[]")))
            .required("type")
            .required("rows_migrated")
            .required("changes")
            .build(),
    )
}

#[allow(dead_code)]
fn _unused_array(name: &str) -> Schema {
    array_of(name)
}

fn plugin_info_schema() -> Schema {
    let str_array: RefOr<Schema> = RefOr::T(Schema::Array(
        ArrayBuilder::new()
            .items(RefOr::T(Schema::Object(str_field().build())))
            .build(),
    ));
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("name", str_field())
            .property("version", str_field())
            .property("description", str_field())
            .property("declared", str_array.clone())
            .property("granted", str_array.clone())
            .property("hooks", str_array)
            .property(
                "enabled",
                ObjectBuilder::new().schema_type(Type::Boolean),
            )
            .required("name")
            .required("version")
            .required("declared")
            .required("granted")
            .required("hooks")
            .required("enabled")
            .build(),
    )
}

fn plugin_grant_body_schema() -> Schema {
    let str_array: RefOr<Schema> = RefOr::T(Schema::Array(
        ArrayBuilder::new()
            .items(RefOr::T(Schema::Object(str_field().build())))
            .build(),
    ));
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("capabilities", str_array)
            .required("capabilities")
            .build(),
    )
}

fn plugin_enabled_body_schema() -> Schema {
    Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property(
                "enabled",
                ObjectBuilder::new().schema_type(Type::Boolean),
            )
            .required("enabled")
            .build(),
    )
}

trait PathItemExt: Sized {
    fn also_method(self, method: HttpMethod, op: utoipa::openapi::path::Operation) -> Self;
}

impl PathItemExt for PathItem {
    fn also_method(mut self, method: HttpMethod, op: utoipa::openapi::path::Operation) -> Self {
        let extra = PathItem::new(method, op);
        self.merge_operations(extra);
        self
    }
}

// --- router ---

/// Empty placeholder router. Spec + UI are served by [`swagger_ui_router`]
/// (which mounts both `/api/docs/*` and `/api/openapi.json`).
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
}

/// State-less Swagger UI router serving the rendered docs at `/api/docs` and
/// the JSON spec at `/api/openapi.json`. Mounted from `lib.rs` after the main
/// router has been bound to its state.
pub fn swagger_ui_router() -> Router {
    let swagger =
        utoipa_swagger_ui::SwaggerUi::new("/api/docs").url("/api/openapi.json", api_doc());
    <Router<()> as From<utoipa_swagger_ui::SwaggerUi>>::from(swagger)
}
