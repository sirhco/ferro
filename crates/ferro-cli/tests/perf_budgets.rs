//! Perf-budget enforcement against the real built admin bundle.
//!
//! Skips silently when `target/site/pkg/` has no `*.wasm.br` files (CI runs
//! that haven't executed `ferro build` first). Run locally with:
//!
//! ```sh
//! cargo run -p ferro-cli -- build
//! cargo test -p ferro-cli --test perf_budgets
//! ```
//!
//! Or set `PERF_BUDGETS=strict` to fail loud when bundles are missing.

use std::path::PathBuf;

use ferro_cli::budgets::{collect, Args};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn admin_bundle_under_budget() {
    let site_dir = workspace_root().join("target").join("site");
    let pkg = site_dir.join("pkg");
    let strict = std::env::var("PERF_BUDGETS").as_deref() == Ok("strict");

    if !pkg.exists() {
        if strict {
            panic!(
                "PERF_BUDGETS=strict but no pkg/ at {} — run `ferro build` first",
                pkg.display()
            );
        }
        eprintln!(
            "skipping: no built admin bundle at {} (run `ferro build` to enforce)",
            pkg.display()
        );
        return;
    }

    let args = Args {
        site_dir,
        max_route_kb: 250,
        max_total_kb: 1024,
        verbose: true,
    };
    let report = collect(&args).expect("read pkg dir");
    if report.entries.is_empty() {
        if strict {
            panic!(
                "PERF_BUDGETS=strict but no .wasm.br files under {}",
                args.site_dir.join("pkg").display()
            );
        }
        eprintln!(
            "skipping: no .wasm.br files under {}",
            args.site_dir.join("pkg").display()
        );
        return;
    }

    eprintln!("{}", report.render());
    let violations = report.violations();
    assert!(violations.is_empty(), "perf budgets exceeded: {violations:#?}");
}
