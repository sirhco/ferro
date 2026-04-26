//! Audit-log plugin.
//!
//! Subscribes to all four content lifecycle events and appends a JSON line
//! to `/data/audit.log` (preopened by the host as `<plugin_dir>/data/audit.log`).
//! Demonstrates: multi-hook subscription, sandboxed file I/O.

wit_bindgen::generate!({
    world: "plugin",
    path: "../../crates/ferro-plugin/wit",
});

use exports::ferro::cms::guest::Guest;
use ferro::cms::host::{log, LogLevel};
use ferro::cms::types::HookEvent;

use std::fs::OpenOptions;
use std::io::Write;

struct Component;

impl Guest for Component {
    fn init() -> Result<(), String> {
        log(LogLevel::Info, "plugin-audit", "loaded");
        Ok(())
    }

    fn on_event(evt: HookEvent) -> Result<(), String> {
        let line = match &evt {
            HookEvent::ContentCreated(c) => {
                format!(
                    r#"{{"event":"content.created","type":"{}","slug":"{}","status":"{}"}}"#,
                    escape(&c.type_slug),
                    escape(&c.slug),
                    escape(&c.status)
                )
            }
            HookEvent::ContentUpdated(c) => {
                format!(
                    r#"{{"event":"content.updated","type":"{}","slug":"{}","status":"{}"}}"#,
                    escape(&c.type_slug),
                    escape(&c.slug),
                    escape(&c.status)
                )
            }
            HookEvent::ContentPublished(c) => {
                format!(
                    r#"{{"event":"content.published","type":"{}","slug":"{}"}}"#,
                    escape(&c.type_slug),
                    escape(&c.slug)
                )
            }
            HookEvent::ContentDeleted(d) => {
                format!(
                    r#"{{"event":"content.deleted","content_id":"{}","slug":"{}"}}"#,
                    escape(&d.content_id),
                    escape(&d.slug)
                )
            }
        };
        if let Err(e) = append(&line) {
            log(
                LogLevel::Warn,
                "plugin-audit",
                &format!("audit append failed: {e}"),
            );
        }
        Ok(())
    }
}

fn append(line: &str) -> Result<(), std::io::Error> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/data/audit.log")?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

export!(Component);
