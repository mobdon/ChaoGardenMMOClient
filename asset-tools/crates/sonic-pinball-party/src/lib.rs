//! Version-specific A3VE asset tools and incomplete Rust game source port.
//! No native save importer or MMO authority is installed by this crate.
pub mod audio;
pub mod binary;
pub mod catalog;
pub mod game;
pub mod garden;
pub mod graphics;
pub mod runtime_host;
pub mod tables;

use sha1::{Digest, Sha1};
use sha2::Sha256;

pub fn sha1(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
