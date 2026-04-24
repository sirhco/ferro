//! OpenAPI 3.0 surface for the REST layer.
//!
//! Spec is built by hand with `utoipa::openapi::*` so handlers stay free of
//! attributes and ferro-core types don't need `ToSchema` impls leaking out of
//! their crate. Served at `/api/openapi.json`.

use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
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
        .schema("LoginBody", Schema::from(opaque_object("Login email + password.")))
        .schema(
            "LoginResponse",
            Schema::from(opaque_object("Bearer JWT + user record.")),
        )
        .schema("Content", Schema::from(opaque_object("Content entry.")))
        .schema("ContentPatch", Schema::from(opaque_object("Partial update.")))
        .schema(
            "ContentType",
            Schema::from(opaque_object("Content type / schema definition.")),
        )
        .schema("Page", Schema::from(opaque_object("Paginated list envelope.")))
        .schema("Site", Schema::from(opaque_object("Site record.")))
        .schema("User", Schema::from(opaque_object("User record.")))
        .schema(
            "TypeUpdateResponse",
            Schema::from(opaque_object(
                "Content type after update + schema migration report.",
            )),
        )
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

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/openapi.json", get(openapi_json))
}

async fn openapi_json(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(api_doc())
}
