# Deployment

Ferro ships as one binary plus the admin SPA's static assets. Run it under any Linux/macOS supervisor; front it with TLS termination.

## Production build

```sh
cargo build --release -p ferro-cli
cargo leptos build --project ferro-admin --release --precompress
```

Outputs:

- `target/release/ferro` — the server binary (~30 MB stripped).
- `target/site/pkg/ferro_admin.{js,wasm,css}` plus `.br` siblings — the admin SPA.
- `target/site/favicon.svg` — favicon.

`--precompress` writes brotli-compressed siblings; `tower-http`'s `ServeDir::precompressed_br()` will serve them when the client sends `Accept-Encoding: br`.

## Filesystem layout (recommended)

```
/opt/ferro/
  bin/ferro                   # the binary
  site/
    pkg/...                   # cargo-leptos output
    favicon.svg
  config/
    ferro.toml
  data/                       # storage (fs-json) or rocksdb (surreal-embedded)
  media/                      # local media backend root
  plugins/                    # WASM components (when wired)
  log/
```

## systemd unit

```ini
# /etc/systemd/system/ferro.service
[Unit]
Description=Ferro CMS
After=network.target

[Service]
Type=simple
User=ferro
Group=ferro
WorkingDirectory=/opt/ferro
EnvironmentFile=/opt/ferro/config/env
ExecStart=/opt/ferro/bin/ferro --config /opt/ferro/config/ferro.toml serve --site-dir /opt/ferro/site
Restart=on-failure
RestartSec=2
LimitNOFILE=65536
ProtectSystem=strict
ReadWritePaths=/opt/ferro/data /opt/ferro/media /opt/ferro/log
NoNewPrivileges=yes
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

```sh
# /opt/ferro/config/env
FERRO_JWT_SECRET=<32+ bytes hex>
RUST_LOG=info,ferro=info
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now ferro
sudo journalctl -u ferro -f
```

## Reverse proxy (nginx)

```nginx
upstream ferro {
    server 127.0.0.1:8080;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name cms.example.com;

    ssl_certificate     /etc/letsencrypt/live/cms.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/cms.example.com/privkey.pem;

    client_max_body_size 30M;       # > 25 MiB media upload cap

    location / {
        proxy_pass http://ferro;
        proxy_http_version 1.1;

        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        proxy_read_timeout 300s;     # for SSE / WS subscriptions
    }
}
```

Caddy / Traefik configs follow the same shape — pass `X-Real-IP` so rate limiting works per real client.

## Dockerfile

A minimal multi-stage build (also see the workspace's `Dockerfile`):

```dockerfile
# syntax=docker/dockerfile:1.7
FROM rustlang/rust:nightly-bookworm AS build
WORKDIR /src

# Cache deps
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY examples ./examples
COPY .cargo ./.cargo

RUN cargo install cargo-leptos --locked
RUN cargo build --release -p ferro-cli
RUN cargo leptos build --project ferro-admin --release --precompress

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
RUN useradd --system --user-group --home /var/lib/ferro ferro
WORKDIR /opt/ferro
COPY --from=build /src/target/release/ferro /usr/local/bin/ferro
COPY --from=build /src/target/site /opt/ferro/site
USER ferro
EXPOSE 8080
ENTRYPOINT ["ferro"]
CMD ["--config", "/opt/ferro/config/ferro.toml", "serve", "--bind", "0.0.0.0:8080", "--site-dir", "/opt/ferro/site"]
```

`docker run` example:

```sh
docker run -d --name ferro \
  -p 8080:8080 \
  -e FERRO_JWT_SECRET=$(openssl rand -hex 32) \
  -v $(pwd)/config:/opt/ferro/config:ro \
  -v $(pwd)/data:/opt/ferro/data \
  -v $(pwd)/media:/opt/ferro/media \
  ferro:latest
```

## docker-compose

```yaml
version: "3.9"
services:
  ferro:
    image: ferro:latest
    restart: unless-stopped
    ports:
      - "127.0.0.1:8080:8080"
    environment:
      FERRO_JWT_SECRET: "${FERRO_JWT_SECRET}"
      RUST_LOG: "info,ferro=info"
    volumes:
      - ./config:/opt/ferro/config:ro
      - ./data:/opt/ferro/data
      - ./media:/opt/ferro/media
    depends_on:
      - postgres                  # if [storage] kind = "postgres"

  postgres:
    image: postgres:16
    restart: unless-stopped
    environment:
      POSTGRES_USER: ferro
      POSTGRES_PASSWORD: "${PG_PASSWORD}"
      POSTGRES_DB: ferro
    volumes:
      - pg-data:/var/lib/postgresql/data

volumes:
  pg-data:
```

## Health & readiness

- **Liveness**: `GET /healthz` — returns `ok` if the process is up. Use as the orchestrator's liveness probe.
- **Readiness**: `GET /readyz` — pings the storage backend (e.g. Postgres `SELECT 1`). Use as the readiness probe; failed reads keep traffic away during DB blips.

## Logging

Default tracing emits human-readable lines. Pipe through `jq` for JSON-style consumption (the `tracing-subscriber` JSON layer is wired but on a feature flag we'll surface in v1.0). For aggregation, scrape stdout via your usual log shipper (Vector, Fluent Bit, Loki Promtail).

## Backups

Per backend:

- **fs-json / fs-markdown**: `tar c data/` (or `git push`).
- **surreal-embedded**: shut down `ferro serve`, snapshot `data/ferro.db/`, restart. Online backup needs a remote SurrealDB.
- **postgres**: `pg_dump --format=custom > ferro.dump`. PITR via WAL archiving for ≤RPO 5 min.
- **media**: snapshot `media-store/`, or rely on the object-storage backend's lifecycle policies (S3 versioning + Glacier).

## Hardening checklist

- [ ] TLS everywhere; HSTS in nginx.
- [ ] `FERRO_JWT_SECRET` ≥ 32 bytes of entropy, env-injected, never committed.
- [ ] Reverse proxy passes `X-Real-IP` so rate limiting works.
- [ ] `[auth] allow_public_signup = false` unless intentional.
- [ ] `client_max_body_size` matches Ferro's 25 MiB upload cap (or wider if you raise it).
- [ ] Process user is non-root (`ferro:ferro`); writable dirs locked down.
- [ ] Backups run + are restored periodically (untested backups don't exist).
- [ ] Postgres `max_connections` matches your worker count.
- [ ] CDN in front of `/api/v1/media/*/raw` if traffic warrants it.
- [ ] Webhooks have HMAC secrets, receivers verify.
- [ ] Admin users have TOTP enrolled (Settings → Two-factor authentication).

## Zero-downtime deploys

The current binary doesn't gracefully drain in-flight requests on SIGTERM (TODO). For now:

- Use a load balancer with two backends; rolling restart.
- Keep mutations short (< 5s); long-running tasks belong off the request path.
- For schema migrations, deploy the migration in one release, then the dependent feature in the next — additive changes are safe under multi-writer Postgres.

## Cost shape

Single-binary architecture means the floor is small: a 1 vCPU / 1 GiB instance handles small-team CMS workloads with room to spare. RocksDB / Postgres dominate disk usage; media bytes dwarf both at scale. Metrics on actual cost shape will land with the Prometheus integration.
