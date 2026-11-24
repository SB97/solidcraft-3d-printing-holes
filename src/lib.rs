//! SolidCraft plugin `solidcraft-3d-printing-holes`: holes designed for 3D printing.
//!
//! Entries (see `plugin/plugin.json`):
//! - `teardrop`: turn selected sketch circles into teardrop profiles so holes in vertical
//!   walls print without sagging.

pub mod geometry;
pub mod host;
pub mod teardrop;

use serde_json::Value;

/// Dispatch an entry by name with its JSON parameters.
pub fn execute(host: &mut dyn host::Host, entry: &str, params: &str) -> host::Result<String> {
    let params: Value = if params.trim().is_empty() {
        Value::Object(Default::default())
    } else {
        serde_json::from_str(params).map_err(|e| host::Error::input("params", format!("invalid JSON: {e}")))?
    };
    let out = match entry {
        "teardrop" => teardrop::run(host, &params)?,
        other => return Err(host::Error::input("entry", format!("unknown entry `{other}`"))),
    };
    Ok(out.to_string())
}

#[cfg(target_arch = "wasm32")]
mod component {
    wit_bindgen::generate!({
        world: "command-plugin",
        path: "wit",
    });

    use craft::plugin::types::{CommandFailure, FieldFailure, PluginError};
    use serde_json::Value;

    struct WasmHost;

    impl crate::host::Host for WasmHost {
        fn run(&mut self, command: &str, args: &Value) -> crate::host::Result<Value> {
            match craft::plugin::commands::run(command, &args.to_string()) {
                Ok(text) => serde_json::from_str(&text)
                    .map_err(|e| crate::host::Error::command(command, format!("invalid JSON reply: {e}"))),
                Err(PluginError::CommandFailed(f)) => Err(crate::host::Error::command(&f.command, f.reason)),
                Err(PluginError::InvalidInput(f)) => Err(crate::host::Error::command(command, format!("{}: {}", f.field, f.reason))),
                Err(other) => Err(crate::host::Error::Host(format!("{command}: {other:?}"))),
            }
        }
        fn log(&mut self, message: &str) {
            craft::plugin::log::log(craft::plugin::log::Level::Info, message);
        }
        fn warn(&mut self, message: &str) {
            craft::plugin::log::log(craft::plugin::log::Level::Warn, message);
        }
    }

    struct Plugin;

    impl exports::craft::plugin::entry::Guest for Plugin {
        fn execute(entry: String, params: String) -> Result<String, PluginError> {
            crate::execute(&mut WasmHost, &entry, &params).map_err(|e| match e {
                crate::host::Error::Command { command, reason } => {
                    PluginError::CommandFailed(CommandFailure { command, reason })
                }
                // SolidCraft 0.48 shows only `reason` of an invalid input, so name the field in it:
                // a script or MCP caller has no form to point at the field.
                crate::host::Error::Input { field, reason } => {
                    let reason = format!("{field}: {reason}");
                    PluginError::InvalidInput(FieldFailure { field, reason })
                }
                crate::host::Error::Host(m) => PluginError::Trap(m),
            })
        }
    }

    export!(Plugin);
}
