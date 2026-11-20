//! Version-specific A3VE asset tools and incomplete Rust game source port.
//! No native save importer or MMO authority is installed by this crate.

/// A test vector file from the local, git-ignored vectors directory.
///
/// The vector files are generated from the user's own ROM by the
/// `tests/*_reference.py` scripts, so they are not committed. build.rs looks
/// for them in `SPP_VECTORS_DIR` or `.local/research/sonic-pinball-party-vectors/`
/// and sets `cfg(spp_vectors)` when they are there. Tests that read vectors
/// are ignored without them.
#[cfg(spp_vectors)]
#[allow(unused_macros)]
macro_rules! vectors {
    ($name:literal) => {
        include_str!(concat!(env!("SPP_VECTORS_DIR"), "/", $name))
    };
}
#[cfg(not(spp_vectors))]
#[allow(unused_macros)]
macro_rules! vectors {
    ($name:literal) => {
        $crate::missing_vectors($name)
    };
}

/// Called by `vectors!` when the local vector files are absent. Tests that
/// reach it are ignored unless run with `--ignored`.
#[cfg(not(spp_vectors))]
#[doc(hidden)]
#[allow(dead_code)]
pub fn missing_vectors(name: &str) -> &'static str {
    panic!("needs local Pinball Party vectors: {name} (set SPP_VECTORS_DIR)")
}
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
