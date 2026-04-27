//! `ferro admin` — direct repo access for bootstrap and ops tooling.
//!
//! Bypasses the HTTP layer so an operator can create the first admin user
//! before the server is reachable. Reads `ferro.toml` for storage config.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args as ClapArgs, Subcommand};
use console::style;
use ferro_auth::hash_password;
use ferro_core::{Permission, Role, RoleId, Scope, User, UserId};
use time::OffsetDateTime;

use crate::config::FerroConfig;

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Create a user. Pass `--role` repeatedly to attach roles by id or name.
    CreateUser(CreateUserArgs),
    /// List all users (password hashes redacted).
    ListUsers,
    /// Create a role with a named preset of permissions.
    CreateRole(CreateRoleArgs),
    /// List all roles.
    ListRoles,
    /// Attach a role to an existing user.
    GrantRole(GrantRoleArgs),
    /// Seed the `admin` role (full permissions). Idempotent.
    SeedAdminRole,
}

#[derive(Debug, ClapArgs)]
pub struct CreateUserArgs {
    #[arg(long)]
    pub email: String,
    #[arg(long)]
    pub handle: String,
    #[arg(long)]
    pub password: String,
    #[arg(long)]
    pub display_name: Option<String>,
    /// Role identifiers (typed-id like `role_01HK...`) OR role names.
    #[arg(long)]
    pub role: Vec<String>,
    /// Skip activation flag — user must be enabled later via PATCH.
    #[arg(long)]
    pub inactive: bool,
    /// Convenience: auto-seed an `admin` role (full access) if missing and
    /// attach it to this user. Equivalent to running `seed-admin-role` then
    /// `create-user --role admin`.
    #[arg(long)]
    pub with_admin: bool,
}

#[derive(Debug, ClapArgs)]
pub struct CreateRoleArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub description: Option<String>,
    /// Permission preset.
    #[arg(long, value_enum, default_value = "custom")]
    pub preset: Preset,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Preset {
    /// Full access (`Permission::Admin`).
    Admin,
    /// Manage users + roles only.
    UserManager,
    /// Read-only on every type.
    Reader,
    /// No permissions; populate later via REST PATCH.
    Custom,
}

#[derive(Debug, ClapArgs)]
pub struct GrantRoleArgs {
    /// User id (`user_01HK...`) or email.
    #[arg(long)]
    pub user: String,
    /// Role id (`role_01HK...`) or name.
    #[arg(long)]
    pub role: String,
}

pub async fn run(cmd: Cmd, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let repo = ferro_storage::connect(&cfg.storage).await?;
    repo.migrate().await?;

    match cmd {
        Cmd::CreateUser(a) => create_user(&*repo, a).await,
        Cmd::ListUsers => list_users(&*repo).await,
        Cmd::CreateRole(a) => {
            let role = build_role(a);
            let saved = repo.users().upsert_role(role).await?;
            println!(
                "{} created role {} ({})",
                style("✓").green(),
                style(&saved.name).cyan(),
                saved.id
            );
            Ok(())
        }
        Cmd::ListRoles => list_roles(&*repo).await,
        Cmd::GrantRole(a) => grant_role(&*repo, a).await,
        Cmd::SeedAdminRole => seed_admin_role(&*repo).await,
    }
}

async fn create_user(repo: &dyn ferro_storage::Repository, args: CreateUserArgs) -> Result<()> {
    if repo.users().by_email(&args.email).await?.is_some() {
        bail!("email `{}` already in use", args.email);
    }
    let mut role_ids = Vec::with_capacity(args.role.len() + 1);
    for r in &args.role {
        role_ids.push(resolve_role(repo, r).await?);
    }
    if args.with_admin {
        let admin_id = ensure_admin_role(repo).await?;
        if !role_ids.contains(&admin_id) {
            role_ids.push(admin_id);
        }
    }
    let user = User {
        id: UserId::new(),
        email: args.email.clone(),
        handle: args.handle,
        display_name: args.display_name,
        password_hash: Some(hash_password(&args.password)?),
        roles: role_ids,
        active: !args.inactive,
        created_at: OffsetDateTime::now_utc(),
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    let saved = repo.users().upsert(user).await?;
    println!(
        "{} created user {} ({}) — {} role(s)",
        style("✓").green(),
        style(&args.email).cyan(),
        saved.id,
        saved.roles.len()
    );
    Ok(())
}

async fn list_users(repo: &dyn ferro_storage::Repository) -> Result<()> {
    let users = repo.users().list().await?;
    if users.is_empty() {
        println!("(no users)");
        return Ok(());
    }
    for u in users {
        println!(
            "{}  {}  {}  {} role(s)  {}",
            u.id,
            style(&u.email).cyan(),
            u.handle,
            u.roles.len(),
            if u.active { "active" } else { "inactive" }
        );
    }
    Ok(())
}

async fn list_roles(repo: &dyn ferro_storage::Repository) -> Result<()> {
    let roles = repo.users().list_roles().await?;
    if roles.is_empty() {
        println!("(no roles)");
        return Ok(());
    }
    for r in roles {
        println!("{}  {}  perms={:?}", r.id, style(&r.name).cyan(), r.permissions);
    }
    Ok(())
}

async fn grant_role(repo: &dyn ferro_storage::Repository, args: GrantRoleArgs) -> Result<()> {
    let mut user = resolve_user_record(repo, &args.user).await?;
    let role_id = resolve_role(repo, &args.role).await?;
    if !user.roles.contains(&role_id) {
        user.roles.push(role_id);
        repo.users().upsert(user.clone()).await?;
    }
    println!("{} {} now has {} role(s)", style("✓").green(), user.email, user.roles.len());
    Ok(())
}

/// Idempotent: skips if a role named `admin` already exists.
async fn seed_admin_role(repo: &dyn ferro_storage::Repository) -> Result<()> {
    let id = ensure_admin_role(repo).await?;
    println!("{} admin role available ({})", style("✓").green(), id);
    Ok(())
}

/// Return the id of the `admin` role, creating it if absent.
async fn ensure_admin_role(repo: &dyn ferro_storage::Repository) -> Result<RoleId> {
    if let Some(existing) = repo.users().list_roles().await?.into_iter().find(|r| r.name == "admin")
    {
        return Ok(existing.id);
    }
    let role = Role {
        id: RoleId::new(),
        name: "admin".into(),
        description: Some("Full system access.".into()),
        permissions: vec![Permission::Admin],
    };
    Ok(repo.users().upsert_role(role).await?.id)
}

fn build_role(args: CreateRoleArgs) -> Role {
    let permissions = match args.preset {
        Preset::Admin => vec![Permission::Admin],
        Preset::UserManager => vec![Permission::ManageUsers],
        Preset::Reader => vec![Permission::Read(Scope::Global)],
        Preset::Custom => Vec::new(),
    };
    Role { id: RoleId::new(), name: args.name, description: args.description, permissions }
}

/// Accept a typed-id (`role_<ulid>`) OR a role name. Names look up by linear
/// scan — fine for the bootstrap path where `roles` is small.
async fn resolve_role(repo: &dyn ferro_storage::Repository, key: &str) -> Result<RoleId> {
    if let Ok(id) = key.parse::<RoleId>() {
        if repo.users().get_role(id).await?.is_some() {
            return Ok(id);
        }
        bail!("role id `{key}` not found");
    }
    repo.users()
        .list_roles()
        .await?
        .into_iter()
        .find(|r| r.name == key)
        .map(|r| r.id)
        .with_context(|| format!("no role named `{key}`"))
}

async fn resolve_user_record(repo: &dyn ferro_storage::Repository, key: &str) -> Result<User> {
    if let Ok(id) = key.parse::<UserId>() {
        return repo.users().get(id).await?.with_context(|| format!("user id `{key}` not found"));
    }
    repo.users().by_email(key).await?.with_context(|| format!("no user with email `{key}`"))
}
