//! Static recompiler for the GSNE8P main.dol and disc RELs.
//!
//! The tool (`sa2-recomp`) reads the player's local main.dol and the decomp's
//! symbols.txt and writes Rust into an ignored folder under `.local`. No game
//! bytes or generated code are part of this crate.

pub mod dol;
pub mod gen;
pub mod host;
pub mod rel;
pub mod relgen;
pub mod os;
pub mod rt;
