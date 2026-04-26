use leptos::prelude::*;
use leptos_router::components::*;
use leptos_router::path;

use crate::data::{ApiClient, ContentEntry};
use crate::render::{currency_format, humanize_date, render_blocks, render_markdown_basic};

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=NotFound>
                <Route path=path!("/") view=Home/>
                <Route path=path!("/blog") view=BlogIndex/>
                <Route path=path!("/blog/:slug") view=PostDetail/>
                <Route path=path!("/products") view=ProductIndex/>
                <Route path=path!("/products/:slug") view=ProductDetail/>
                <Route path=path!("/events") view=EventIndex/>
                <Route path=path!("/events/:slug") view=EventDetail/>
                <Route path=path!("/:slug") view=PageDetail/>
            </Routes>
        </Router>
    }
}

#[component]
fn Layout(children: Children) -> impl IntoView {
    view! {
        <div class="site">
            <header class="site-header">
                <a class="site-brand" href="/">"Ferro Demo"</a>
                <nav class="site-nav">
                    <a href="/blog">"Blog"</a>
                    <a href="/products">"Products"</a>
                    <a href="/events">"Events"</a>
                    <a href="/about">"About"</a>
                </nav>
            </header>
            <main class="site-main">{children()}</main>
            <footer class="site-footer">
                <p>"Built with Ferro · zero-JS · server-rendered"</p>
            </footer>
        </div>
    }
}

#[component]
fn Home() -> impl IntoView {
    let posts = Resource::new(|| (), |_| async move { fetch_list("post").await });
    let products = Resource::new(|| (), |_| async move { fetch_list("product").await });

    view! {
        <Layout>
            <section class="hero">
                <h1>"A CMS that gets out of your way."</h1>
                <p>"This entire site renders server-side from a Rust binary. No JS framework, no database round-trip per element."</p>
            </section>
            <Suspense fallback=|| view! { <p class="muted">"Loading…"</p> }>
                <section class="grid-section">
                    <h2>"Latest posts"</h2>
                    <div class="card-grid">
                        {move || posts.get().unwrap_or_default().into_iter().take(3).map(|p| view! {
                            <PostCard entry=p />
                        }).collect_view()}
                    </div>
                    <p><a href="/blog" class="ferro-cta">"All posts →"</a></p>
                </section>
                <section class="grid-section">
                    <h2>"Plans"</h2>
                    <div class="card-grid">
                        {move || products.get().unwrap_or_default().into_iter().map(|p| view! {
                            <ProductCard entry=p />
                        }).collect_view()}
                    </div>
                </section>
            </Suspense>
        </Layout>
    }
}

#[component]
fn BlogIndex() -> impl IntoView {
    let posts = Resource::new(|| (), |_| async move { fetch_list("post").await });
    view! {
        <Layout>
            <h1>"Blog"</h1>
            <Suspense fallback=|| view! { <p class="muted">"Loading…"</p> }>
                <ul class="post-list">
                    {move || posts.get().unwrap_or_default().into_iter().map(|p| view! {
                        <li class="post-list-item">
                            <a href=format!("/blog/{}", p.slug)>
                                <h3>{p.title()}</h3>
                                <p class="muted">{p.published_date()}</p>
                                {p.excerpt().map(|e| view! { <p>{e}</p> })}
                            </a>
                        </li>
                    }).collect_view()}
                </ul>
            </Suspense>
        </Layout>
    }
}

#[component]
fn PostDetail() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();
    let entry = Resource::new(slug, |s| async move { fetch_one("post", &s).await });

    view! {
        <Layout>
            <Suspense fallback=|| view! { <p class="muted">"Loading…"</p> }>
                {move || match entry.get().flatten() {
                    None => view! { <NotFoundInner /> }.into_any(),
                    Some(e) => {
                        let title = e.title();
                        let body_html = e.data.get("body")
                            .and_then(|v| v.as_str())
                            .map(render_markdown_basic)
                            .unwrap_or_default();
                        view! {
                            <article class="post">
                                <header>
                                    <h1>{title}</h1>
                                    <p class="muted">{e.published_date()}</p>
                                </header>
                                <div class="prose" inner_html=body_html />
                            </article>
                        }.into_any()
                    }
                }}
            </Suspense>
        </Layout>
    }
}

#[component]
fn ProductIndex() -> impl IntoView {
    let items = Resource::new(|| (), |_| async move { fetch_list("product").await });
    view! {
        <Layout>
            <h1>"Products"</h1>
            <Suspense fallback=|| view! { <p class="muted">"Loading…"</p> }>
                <div class="card-grid">
                    {move || items.get().unwrap_or_default().into_iter().map(|p| view! {
                        <ProductCard entry=p />
                    }).collect_view()}
                </div>
            </Suspense>
        </Layout>
    }
}

#[component]
fn ProductDetail() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();
    let entry = Resource::new(slug, |s| async move { fetch_one("product", &s).await });

    view! {
        <Layout>
            <Suspense fallback=|| view! { <p class="muted">"Loading…"</p> }>
                {move || match entry.get().flatten() {
                    None => view! { <NotFoundInner /> }.into_any(),
                    Some(e) => {
                        let title = e.title();
                        let price = e.data.get("price_cents").and_then(|v| v.as_i64()).unwrap_or(0);
                        let currency = e.data.get("currency").and_then(|v| v.as_str()).unwrap_or("USD").to_string();
                        let in_stock = e.data.get("in_stock").and_then(|v| v.as_bool()).unwrap_or(false);
                        let blocks_html = render_blocks(e.data.get("blocks"), "/media");
                        view! {
                            <article class="product">
                                <header>
                                    <h1>{title}</h1>
                                    <p class="product-price">{currency_format(price, &currency)}</p>
                                    <p class={if in_stock { "stock-yes" } else { "stock-no" }}>
                                        {if in_stock { "In stock" } else { "Sold out" }}
                                    </p>
                                </header>
                                <div class="prose" inner_html=blocks_html />
                            </article>
                        }.into_any()
                    }
                }}
            </Suspense>
        </Layout>
    }
}

#[component]
fn EventIndex() -> impl IntoView {
    let items = Resource::new(|| (), |_| async move { fetch_list("event").await });
    view! {
        <Layout>
            <h1>"Events"</h1>
            <Suspense fallback=|| view! { <p class="muted">"Loading…"</p> }>
                <ul class="post-list">
                    {move || items.get().unwrap_or_default().into_iter().map(|p| {
                        let starts = p.data.get("starts_at").and_then(|v| v.as_str()).map(humanize_date).unwrap_or_default();
                        let venue = p.data.get("venue").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        view! {
                            <li class="post-list-item">
                                <a href=format!("/events/{}", p.slug)>
                                    <h3>{p.title()}</h3>
                                    <p class="muted">{starts}" · "{venue}</p>
                                </a>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </Suspense>
        </Layout>
    }
}

#[component]
fn EventDetail() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();
    let entry = Resource::new(slug, |s| async move { fetch_one("event", &s).await });

    view! {
        <Layout>
            <Suspense fallback=|| view! { <p class="muted">"Loading…"</p> }>
                {move || match entry.get().flatten() {
                    None => view! { <NotFoundInner /> }.into_any(),
                    Some(e) => {
                        let title = e.title();
                        let starts = e.data.get("starts_at").and_then(|v| v.as_str()).map(humanize_date).unwrap_or_default();
                        let ends = e.data.get("ends_at").and_then(|v| v.as_str()).map(humanize_date).unwrap_or_default();
                        let venue = e.data.get("venue").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let blocks_html = render_blocks(e.data.get("blocks"), "/media");
                        view! {
                            <article class="event">
                                <header>
                                    <h1>{title}</h1>
                                    <p class="muted">{starts}" → "{ends}" · "{venue}</p>
                                </header>
                                <div class="prose" inner_html=blocks_html />
                            </article>
                        }.into_any()
                    }
                }}
            </Suspense>
        </Layout>
    }
}

#[component]
fn PageDetail() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();
    let entry = Resource::new(slug, |s| async move { fetch_one("page", &s).await });

    view! {
        <Layout>
            <Suspense fallback=|| view! { <p class="muted">"Loading…"</p> }>
                {move || match entry.get().flatten() {
                    None => view! { <NotFoundInner /> }.into_any(),
                    Some(e) => {
                        let title = e.title();
                        let blocks_html = render_blocks(e.data.get("blocks"), "/media");
                        view! {
                            <article class="page">
                                <header><h1>{title}</h1></header>
                                <div class="prose" inner_html=blocks_html />
                            </article>
                        }.into_any()
                    }
                }}
            </Suspense>
        </Layout>
    }
}

#[component]
fn PostCard(entry: ContentEntry) -> impl IntoView {
    let slug = entry.slug.clone();
    let title = entry.title();
    let excerpt = entry.excerpt();
    view! {
        <a class="card" href=format!("/blog/{slug}")>
            <h3>{title}</h3>
            {excerpt.map(|e| view! { <p>{e}</p> })}
        </a>
    }
}

#[component]
fn ProductCard(entry: ContentEntry) -> impl IntoView {
    let slug = entry.slug.clone();
    let name = entry.title();
    let price = entry.data.get("price_cents").and_then(|v| v.as_i64()).unwrap_or(0);
    let currency = entry.data.get("currency").and_then(|v| v.as_str()).unwrap_or("USD").to_string();
    view! {
        <a class="card card-product" href=format!("/products/{slug}")>
            <h3>{name}</h3>
            <p class="product-price">{currency_format(price, &currency)}</p>
        </a>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <Layout>
            <NotFoundInner />
        </Layout>
    }
}

#[component]
fn NotFoundInner() -> impl IntoView {
    view! {
        <div class="not-found">
            <h1>"Not found"</h1>
            <p>"The page you were looking for doesn't exist."</p>
            <p><a href="/">"Back home"</a></p>
        </div>
    }
}

async fn fetch_list(type_slug: &str) -> Vec<ContentEntry> {
    let client = expect_context::<ApiClient>();
    client.list_published(type_slug).await.unwrap_or_default()
}

async fn fetch_one(type_slug: &str, slug: &str) -> Option<ContentEntry> {
    let client = expect_context::<ApiClient>();
    client.get(type_slug, slug).await.ok().flatten()
}
