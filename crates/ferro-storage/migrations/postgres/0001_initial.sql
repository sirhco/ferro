-- Ferro Postgres schema v0.1
CREATE TABLE IF NOT EXISTS sites (
    id              TEXT PRIMARY KEY,
    slug            TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    description     TEXT,
    primary_url     TEXT,
    locales         TEXT[] NOT NULL DEFAULT ARRAY['en']::TEXT[],
    default_locale  TEXT NOT NULL DEFAULT 'en',
    settings        JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at      TIMESTAMPTZ NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS content_types (
    id              TEXT PRIMARY KEY,
    site_id         TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    slug            TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    fields          JSONB NOT NULL,
    singleton       BOOLEAN NOT NULL DEFAULT FALSE,
    title_field     TEXT,
    slug_field      TEXT,
    created_at      TIMESTAMPTZ NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL,
    UNIQUE (site_id, slug)
);

CREATE TABLE IF NOT EXISTS content (
    id              TEXT PRIMARY KEY,
    site_id         TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    type_id         TEXT NOT NULL REFERENCES content_types(id) ON DELETE CASCADE,
    slug            TEXT NOT NULL,
    locale          TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('draft','published','archived')),
    data            JSONB NOT NULL,
    author_id       TEXT,
    created_at      TIMESTAMPTZ NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL,
    published_at    TIMESTAMPTZ,
    UNIQUE (site_id, type_id, slug, locale)
);
CREATE INDEX IF NOT EXISTS content_status_idx ON content(site_id, type_id, status);
CREATE INDEX IF NOT EXISTS content_data_gin    ON content USING GIN (data);

CREATE TABLE IF NOT EXISTS roles (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    description     TEXT,
    permissions     JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,
    email           TEXT NOT NULL UNIQUE,
    handle          TEXT NOT NULL UNIQUE,
    display_name    TEXT,
    password_hash   TEXT,
    roles           TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    active          BOOLEAN NOT NULL DEFAULT TRUE,
    created_at          TIMESTAMPTZ NOT NULL,
    last_login          TIMESTAMPTZ,
    password_changed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS media (
    id              TEXT PRIMARY KEY,
    site_id         TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    key             TEXT NOT NULL,
    filename        TEXT NOT NULL,
    mime            TEXT NOT NULL,
    size            BIGINT NOT NULL,
    width           INTEGER,
    height          INTEGER,
    alt             TEXT,
    kind            TEXT NOT NULL,
    uploaded_by     TEXT,
    created_at      TIMESTAMPTZ NOT NULL,
    UNIQUE (site_id, key)
);
