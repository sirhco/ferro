//! Hello-world Ferro plugin.
//!
//! Subscribes to `content.published` and logs the slug via the host `log`
//! import. Demonstrates: WIT bindgen, capability-gated host calls, hook
//! filtering through the manifest.

wit_bindgen::generate!({
    world: "plugin",
    path: "../../crates/ferro-plugin/wit",
});

use exports::ferro::cms::guest::Guest;
use ferro::cms::host::{log, LogLevel};
use ferro::cms::types::HookEvent;

struct Component;

impl Guest for Component {
    fn init() -> Result<(), String> {
        log(LogLevel::Info, "plugin-hello", "loaded");
        Ok(())
    }

    fn on_event(evt: HookEvent) -> Result<(), String> {
        if let HookEvent::ContentPublished(c) = evt {
            log(
                LogLevel::Info,
                "plugin-hello",
                &format!("published {}", c.slug),
            );
        }
        Ok(())
    }
}

export!(Component);
