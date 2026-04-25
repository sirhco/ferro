-- Ferro SurrealDB schema (v0.3)
--
-- Schemaless: Ferro owns the entity shape via serde, so we let SurrealDB
-- store payloads verbatim and key off `id_str` (our prefixed ULID) rather
-- than Surreal's auto-generated `id` Thing. This avoids fighting `record<>`
-- typing while we settle on the long-term mapping.

BEGIN;

DEFINE TABLE site SCHEMALESS;
DEFINE INDEX site_id_str ON site FIELDS id_str UNIQUE;

DEFINE TABLE content_type SCHEMALESS;
DEFINE INDEX content_type_id_str ON content_type FIELDS id_str UNIQUE;

DEFINE TABLE content SCHEMALESS;
DEFINE INDEX content_id_str ON content FIELDS id_str UNIQUE;

DEFINE TABLE user SCHEMALESS;
DEFINE INDEX user_id_str ON user FIELDS id_str UNIQUE;
DEFINE INDEX user_email_unique ON user FIELDS email UNIQUE;

DEFINE TABLE role SCHEMALESS;
DEFINE INDEX role_id_str ON role FIELDS id_str UNIQUE;

DEFINE TABLE media SCHEMALESS;
DEFINE INDEX media_id_str ON media FIELDS id_str UNIQUE;

COMMIT;
