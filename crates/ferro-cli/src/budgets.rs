//! Performance-budget enforcement for the built admin bundle.
//!
//! Walks `<site_dir>/pkg/` for brotli-compressed wasm chunks (`*.wasm.br`)
//! and compares each one against a per-route budget plus the aggregate
//! against a total budget. Defaults match `DESIGN.md §10`:
//!
//! - per-route brotli wasm: <= 250 KB (content-edit budget)
//! - aggregate brotli wasm: <= 1024 KB (room to grow until split lands)
//!
//! Login is intended to fall under 120 KB once `cargo leptos --split`
//! emits a dedicated chunk; today the admin runs in CSR with a single
//! shared bundle, so the per-route check applies to that bundle.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;

const DEFAULT_PER_ROUTE_KB: u64 = 250;
const DEFAULT_TOTAL_KB: u64 = 1024;

#[derive(Debug, Clone, ClapArgs)]
pub struct Args {
    /// `cargo-leptos` site root. Brotli wasm bundles are read from
    /// `<site-dir>/pkg/*.wasm.br`.
    #[arg(long, default_value = "target/site")]
    pub site_dir: PathBuf,

    /// Per-route budget in kilobytes (1024-byte units). Defaults to 250 KB
    /// (matches the content-edit route budget in DESIGN.md §10).
    #[arg(long, default_value_t = DEFAULT_PER_ROUTE_KB)]
    pub max_route_kb: u64,

    /// Aggregate budget across every wasm chunk in kilobytes. Default 1 MB.
    #[arg(long, default_value_t = DEFAULT_TOTAL_KB)]
    pub max_total_kb: u64,

    /// Print the size table even when budgets are met.
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Debug)]
pub struct BudgetReport {
    pub entries: Vec<RouteSize>,
    pub total_bytes: u64,
    pub max_route_bytes: u64,
    pub max_total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct RouteSize {
    pub name: String,
    pub bytes: u64,
}

impl BudgetReport {
    pub fn passes(&self) -> bool {
        self.violations().is_empty()
    }

    pub fn violations(&self) -> Vec<String> {
        let mut v = Vec::new();
        for entry in &self.entries {
            if entry.bytes > self.max_route_bytes {
                v.push(format!(
                    "{} = {} (over per-route budget {})",
                    entry.name,
                    fmt_bytes(entry.bytes),
                    fmt_bytes(self.max_route_bytes)
                ));
            }
        }
        if self.total_bytes > self.max_total_bytes {
            v.push(format!(
                "aggregate = {} (over total budget {})",
                fmt_bytes(self.total_bytes),
                fmt_bytes(self.max_total_bytes)
            ));
        }
        v
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("brotli wasm sizes:\n");
        for e in &self.entries {
            out.push_str(&format!("  {} {}\n", fmt_bytes(e.bytes), e.name));
        }
        out.push_str(&format!(
            "  ---------\n  {} total (budget {} per route, {} aggregate)\n",
            fmt_bytes(self.total_bytes),
            fmt_bytes(self.max_route_bytes),
            fmt_bytes(self.max_total_bytes),
        ));
        out
    }
}

/// Walk `<site_dir>/pkg/` for `*.wasm.br` and return a [`BudgetReport`].
/// Returns an error if the directory is missing — callers in test contexts
/// should check existence first and skip if not built.
pub fn collect(args: &Args) -> Result<BudgetReport> {
    let pkg_dir = args.site_dir.join("pkg");
    if !pkg_dir.exists() {
        return Err(anyhow!("no pkg dir at {} — run `ferro build` first", pkg_dir.display()));
    }
    let mut entries: Vec<RouteSize> = Vec::new();
    for dirent in
        std::fs::read_dir(&pkg_dir).with_context(|| format!("reading {}", pkg_dir.display()))?
    {
        let dirent = dirent?;
        let path = dirent.path();
        if !is_brotli_wasm(&path) {
            continue;
        }
        let bytes = dirent.metadata()?.len();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
            .unwrap_or_else(|| path.display().to_string());
        entries.push(RouteSize { name, bytes });
    }
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let total_bytes = entries.iter().map(|e| e.bytes).sum();
    Ok(BudgetReport {
        entries,
        total_bytes,
        max_route_bytes: args.max_route_kb * 1024,
        max_total_bytes: args.max_total_kb * 1024,
    })
}

pub async fn run(args: Args) -> Result<()> {
    let report = collect(&args)?;
    if report.entries.is_empty() {
        println!(
            "no .wasm.br files under {} — run `ferro build` first",
            args.site_dir.join("pkg").display()
        );
        return Ok(());
    }
    if args.verbose || !report.passes() {
        print!("{}", report.render());
    }
    let violations = report.violations();
    if violations.is_empty() {
        println!(
            "✓ perf budgets met ({} files, {} aggregate)",
            report.entries.len(),
            fmt_bytes(report.total_bytes)
        );
        Ok(())
    } else {
        for v in &violations {
            eprintln!("✗ {}", v);
        }
        Err(anyhow!("perf budgets exceeded"))
    }
}

fn is_brotli_wasm(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).map(|n| n.ends_with(".wasm.br")).unwrap_or(false)
}

fn fmt_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if n >= MB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn write_br(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn report_passes_when_under_budget() {
        let tmp = tempdir().unwrap();
        let pkg = tmp.path().join("pkg");
        write_br(&pkg.join("admin.wasm.br"), 100 * 1024);
        let args = Args {
            site_dir: tmp.path().to_path_buf(),
            max_route_kb: 250,
            max_total_kb: 1024,
            verbose: false,
        };
        let report = collect(&args).unwrap();
        assert_eq!(report.entries.len(), 1);
        assert!(report.passes(), "violations: {:?}", report.violations());
    }

    #[test]
    fn report_fails_when_route_over_budget() {
        let tmp = tempdir().unwrap();
        let pkg = tmp.path().join("pkg");
        write_br(&pkg.join("big.wasm.br"), 300 * 1024);
        let args = Args {
            site_dir: tmp.path().to_path_buf(),
            max_route_kb: 250,
            max_total_kb: 4096,
            verbose: false,
        };
        let report = collect(&args).unwrap();
        assert!(!report.passes());
        assert!(report.violations()[0].contains("over per-route budget"));
    }

    #[test]
    fn report_fails_when_total_over_budget() {
        let tmp = tempdir().unwrap();
        let pkg = tmp.path().join("pkg");
        write_br(&pkg.join("a.wasm.br"), 200 * 1024);
        write_br(&pkg.join("b.wasm.br"), 200 * 1024);
        let args = Args {
            site_dir: tmp.path().to_path_buf(),
            max_route_kb: 250,
            max_total_kb: 300,
            verbose: false,
        };
        let report = collect(&args).unwrap();
        assert!(!report.passes());
    }

    #[test]
    fn ignores_non_brotli_wasm_files() {
        let tmp = tempdir().unwrap();
        let pkg = tmp.path().join("pkg");
        write_br(&pkg.join("admin.wasm.br"), 50 * 1024);
        write_br(&pkg.join("admin.wasm"), 999_999);
        write_br(&pkg.join("admin.js.br"), 999_999);
        let args = Args {
            site_dir: tmp.path().to_path_buf(),
            max_route_kb: 250,
            max_total_kb: 1024,
            verbose: false,
        };
        let report = collect(&args).unwrap();
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].name, "admin.wasm.br");
    }
}
