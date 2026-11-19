//! Local, bounded readers. Source decoding and gameplay implementation are separate.
pub mod archive;
pub mod audio;
pub mod manifest;
pub mod model;
pub mod motion;
pub mod pe;
pub mod stage;
pub mod texture;

use std::fmt;

#[derive(Debug)]
pub struct Error(String);

impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self(value.to_string())
    }
}
pub type Result<T> = std::result::Result<T, Error>;
