//! Intentional-panic plugin.
//!
//! On every `content.created` event, this plugin panics. The host catches
//! the trap inside `call_on_event`, logs it, and keeps the user request flowing
//! (see `crates/ferro-plugin/src/runtime.rs:173-191`). After observing the
//! error in the admin Plugins UI, click Disable to demonstrate hot-swap
//! recovery without restarting the server.

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
        log(
            LogLevel::Warn,
            "plugin-panic",
            "loaded — will panic on content.created (intentional)",
        );
        Ok(())
    }

    fn on_event(_evt: HookEvent) -> Result<(), String> {
        panic!("intentional fault for hot-swap demo");
    }
}

export!(Component);
