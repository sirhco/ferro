# starter-blog

Minimal Ferro example: a blog with `Post` and `Author` content types declared via `#[derive(ContentType)]`.

## Run

```sh
cd examples/starter-blog
cargo run -p ferro-cli -- --config ferro.toml serve
```

Admin UI at `http://localhost:8080/admin`, GraphiQL at `/graphiql`, REST at `/api/v1/*`.
