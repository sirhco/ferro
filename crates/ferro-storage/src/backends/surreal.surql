-- Ferro SurrealDB schema (v0.1)
-- Schemaful where shape is stable; schemaless where content data lives.

BEGIN;

DEFINE TABLE site SCHEMAFULL;
DEFINE FIELD slug ON site TYPE string;
DEFINE FIELD name ON site TYPE string;
DEFINE FIELD description ON site TYPE option<string>;
DEFINE FIELD primary_url ON site TYPE option<string>;
DEFINE FIELD locales ON site TYPE array<string>;
DEFINE FIELD default_locale ON site TYPE string;
DEFINE FIELD settings ON site FLEXIBLE TYPE object;
DEFINE FIELD created_at ON site TYPE datetime;
DEFINE FIELD updated_at ON site TYPE datetime;
DEFINE INDEX site_slug ON site FIELDS slug UNIQUE;

DEFINE TABLE content_type SCHEMAFULL;
DEFINE FIELD site_id ON content_type TYPE record<site>;
DEFINE FIELD slug ON content_type TYPE string;
DEFINE FIELD name ON content_type TYPE string;
DEFINE FIELD description ON content_type TYPE option<string>;
DEFINE FIELD fields ON content_type FLEXIBLE TYPE array;
DEFINE FIELD singleton ON content_type TYPE bool;
DEFINE FIELD title_field ON content_type TYPE option<string>;
DEFINE FIELD slug_field ON content_type TYPE option<string>;
DEFINE FIELD created_at ON content_type TYPE datetime;
DEFINE FIELD updated_at ON content_type TYPE datetime;
DEFINE INDEX content_type_site_slug ON content_type FIELDS site_id, slug UNIQUE;

DEFINE TABLE content SCHEMAFULL;
DEFINE FIELD site_id ON content TYPE record<site>;
DEFINE FIELD type_id ON content TYPE record<content_type>;
DEFINE FIELD slug ON content TYPE string;
DEFINE FIELD locale ON content TYPE string;
DEFINE FIELD status ON content TYPE string
    ASSERT $value INSIDE ["draft", "published", "archived"];
DEFINE FIELD data ON content FLEXIBLE TYPE object;
DEFINE FIELD author_id ON content TYPE option<record<user>>;
DEFINE FIELD created_at ON content TYPE datetime;
DEFINE FIELD updated_at ON content TYPE datetime;
DEFINE FIELD published_at ON content TYPE option<datetime>;
DEFINE INDEX content_site_type_slug_locale ON content
    FIELDS site_id, type_id, slug, locale UNIQUE;
DEFINE INDEX content_search ON content FIELDS slug, locale, status;

DEFINE TABLE user SCHEMAFULL;
DEFINE FIELD email ON user TYPE string;
DEFINE FIELD handle ON user TYPE string;
DEFINE FIELD display_name ON user TYPE option<string>;
DEFINE FIELD password_hash ON user TYPE option<string>;
DEFINE FIELD roles ON user TYPE array<record<role>>;
DEFINE FIELD active ON user TYPE bool;
DEFINE FIELD created_at ON user TYPE datetime;
DEFINE FIELD last_login ON user TYPE option<datetime>;
DEFINE INDEX user_email ON user FIELDS email UNIQUE;
DEFINE INDEX user_handle ON user FIELDS handle UNIQUE;

DEFINE TABLE role SCHEMAFULL;
DEFINE FIELD name ON role TYPE string;
DEFINE FIELD description ON role TYPE option<string>;
DEFINE FIELD permissions ON role FLEXIBLE TYPE array;
DEFINE INDEX role_name ON role FIELDS name UNIQUE;

DEFINE TABLE media SCHEMAFULL;
DEFINE FIELD site_id ON media TYPE record<site>;
DEFINE FIELD key ON media TYPE string;
DEFINE FIELD filename ON media TYPE string;
DEFINE FIELD mime ON media TYPE string;
DEFINE FIELD size ON media TYPE int;
DEFINE FIELD width ON media TYPE option<int>;
DEFINE FIELD height ON media TYPE option<int>;
DEFINE FIELD alt ON media TYPE option<string>;
DEFINE FIELD kind ON media TYPE string;
DEFINE FIELD uploaded_by ON media TYPE option<record<user>>;
DEFINE FIELD created_at ON media TYPE datetime;
DEFINE INDEX media_site_key ON media FIELDS site_id, key UNIQUE;

COMMIT;
