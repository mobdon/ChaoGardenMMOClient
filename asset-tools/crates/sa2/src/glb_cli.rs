//! Read local, hash-bound texture inputs for the static GLB command.
use crate::{manifest, model::Model, visual, visual_glb, Error, Result};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub struct OwnedDependency {
    pub path: PathBuf,
    pub logical_path: String,
    pub bytes: Vec<u8>,
    pub role: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bindings {
    textures: Vec<Texture>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Texture {
    index: u16,
    path: String,
    sha256: String,
}

fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return Err(Error::invalid(
            "GLB dependency exceeds size limit or is not a file",
        ));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::invalid("GLB dependency grew past size limit"));
    }
    Ok(bytes)
}

/// Texture paths are relative to the bindings file directory, with forward slashes.
pub fn build(model: &Model, bindings_path: &Path) -> Result<(Vec<u8>, Vec<OwnedDependency>)> {
    let scene = visual::evaluate_static_model(model)?;
    let required: BTreeSet<_> = scene
        .surfaces
        .iter()
        .map(|s| {
            s.material
                .texture_id
                .ok_or_else(|| Error::invalid("surface lacks texture ID"))
        })
        .collect::<Result<_>>()?;
    let config = fs::canonicalize(bindings_path)?;
    let parent = config
        .parent()
        .ok_or_else(|| Error::invalid("missing bindings parent directory"))?;
    let config_name = config
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::invalid("invalid bindings filename"))?;
    manifest::logical_path(config_name)?;
    let config_bytes = read_limited(&config, 256 * 1024)?;
    let bindings: Bindings = serde_json::from_slice(&config_bytes)?;
    if bindings.textures.is_empty() || bindings.textures.len() > 256 {
        return Err(Error::invalid("unsupported texture binding count"));
    }
    let indices: BTreeSet<_> = bindings.textures.iter().map(|t| t.index).collect();
    if bindings.textures.len() != indices.len() || required != indices {
        return Err(Error::invalid(
            "texture bindings must match distinct native indices exactly",
        ));
    }
    let mut paths = BTreeSet::from([config.clone()]);
    let mut dependencies = vec![OwnedDependency {
        path: config.clone(),
        logical_path: format!("bindings/{config_name}"),
        bytes: config_bytes,
        role: "native-texture-bindings".into(),
    }];
    let mut pngs = BTreeMap::new();
    let mut input_files = Vec::new();
    let mut remaining = 64 * 1024 * 1024;
    for texture in bindings.textures {
        manifest::logical_path(&texture.path)?;
        if texture.sha256.len() != 64 || !texture.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::invalid("texture binding requires a SHA-256 digest"));
        }
        let actual = fs::canonicalize(parent.join(&texture.path))?;
        if !actual.starts_with(parent) || !paths.insert(actual.clone()) {
            return Err(Error::invalid("outside or repeated GLB dependency path"));
        }
        let bytes = read_limited(&actual, remaining.min(32 * 1024 * 1024))?;
        if !manifest::sha256(&bytes).eq_ignore_ascii_case(&texture.sha256) {
            return Err(Error::invalid(format!(
                "texture SHA-256 mismatch at native index {}",
                texture.index
            )));
        }
        remaining -= bytes.len();
        input_files.push((texture.index, actual, format!("bindings/{}", texture.path)));
        pngs.insert(texture.index, bytes);
    }
    let output = visual_glb::export(&scene, &pngs)?;
    for (index, path, logical_path) in input_files {
        dependencies.push(OwnedDependency {
            path,
            logical_path,
            bytes: pngs.remove(&index).expect("collected each input once"),
            role: format!("native-texture-index-{index}"),
        });
    }
    Ok((output, dependencies))
}
