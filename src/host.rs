//! The plugin's view of SolidCraft: run an app command with JSON in and out.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// An app command failed.
    Command { command: String, reason: String },
    /// A plugin input was wrong.
    Input { field: String, reason: String },
    /// Anything else (traps, refusals) passed through from the host.
    Host(String),
}

impl Error {
    pub fn input(field: &str, reason: impl Into<String>) -> Self {
        Error::Input { field: field.into(), reason: reason.into() }
    }
    pub fn command(command: &str, reason: impl Into<String>) -> Self {
        Error::Command { command: command.into(), reason: reason.into() }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait Host {
    fn run(&mut self, command: &str, args: &Value) -> Result<Value>;
    fn log(&mut self, _message: &str) {}
}
