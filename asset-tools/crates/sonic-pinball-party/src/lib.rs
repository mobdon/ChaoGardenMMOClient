//! Version-specific, read-only native asset tools. No game engine or save importer.
pub mod audio;
pub mod binary;
pub mod catalog;
pub mod garden;
pub mod graphics;
pub mod tables;

use sha1::{Digest, Sha1};
use sha2::Sha256;

pub fn sha1(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
