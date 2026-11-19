//! Local-only readers for the fingerprinted Sonic Advance USA ASOE revision 0.
#![forbid(unsafe_code)]

pub mod animation;
pub mod binary;
pub mod catalog;
pub mod garden;
pub mod graphics;
pub mod tables;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
