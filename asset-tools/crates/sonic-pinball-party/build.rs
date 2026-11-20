//! Finds the local Pinball Party test vectors (see `vectors!` in lib.rs).
use std::path::{Path, PathBuf};

const REL: &str = ".local/research/sonic-pinball-party-vectors";

fn main() {
    println!("cargo::rustc-check-cfg=cfg(spp_vectors)");
    println!("cargo::rerun-if-env-changed=SPP_VECTORS_DIR");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dir = std::env::var_os("SPP_VECTORS_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            manifest
                .ancestors()
                .map(|d| d.join(REL))
                .find(|p| p.is_dir())
        });
    match dir {
        Some(d) if d.is_dir() => {
            println!("cargo::rerun-if-changed={}", d.display());
            println!("cargo::rustc-cfg=spp_vectors");
            println!("cargo::rustc-env=SPP_VECTORS_DIR={}", abs(&d).display());
        }
        _ => {
            for d in manifest.ancestors() {
                println!("cargo::rerun-if-changed={}", d.join(".local").display());
            }
        }
    }
}

fn abs(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}
