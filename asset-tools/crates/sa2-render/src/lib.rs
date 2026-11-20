//! The draw path for sa2-runtime (milestone E7): landtable, walker and object
//! geometry from the Steam files, drawn with wgpu (WebGPU in the browser,
//! WebGL2 through wgpu's fallback, native backends for tests). Choices from
//! GX to the web are written down in `sa2/docs/RENDERER-GX-WEB.md`.
#![forbid(unsafe_code)]

pub mod gc;
pub mod gpu;
pub mod land;
pub mod textures;
