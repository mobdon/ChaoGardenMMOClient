//! Proposed SA2 envelope, pending integration ownership of common asset types.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Component, Path},
};

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn logical_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(Error::invalid("expected a relative portable logical path"));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub logical_path: String,
    pub byte_length: u64,
    pub sha256: String,
}
impl Source {
    pub fn from_bytes(label: &str, bytes: &[u8]) -> Result<Self> {
        logical_path(label)?;
        Ok(Self {
            id: format!("sa2.pc.source.{label}"),
            logical_path: label.into(),
            byte_length: bytes.len() as u64,
            sha256: sha256(bytes),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Representation {
    Decoded,
    Opaque,
    Unknown,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Fidelity {
    Verified,
    Adapted,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Span {
    pub source_id: String,
    pub address_space: String,
    pub parent_id: Option<String>,
    pub offset: u64,
    pub byte_length: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Output {
    pub logical_path: String,
    pub format: String,
    pub byte_length: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dependency {
    pub source_id: String,
    pub role: String,
    pub offset: u64,
    pub byte_length: u64,
    pub sha256: String,
    pub bank: Option<usize>,
    pub entry_count: Option<usize>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub category: String,
    pub representation: Representation,
    pub fidelity: Fidelity,
    pub span: Span,
    pub output: Option<Output>,
    pub dependencies: Vec<Dependency>,
    pub unknowns: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub contract: String,
    pub game: String,
    pub revision: String,
    pub sources: Vec<Source>,
    pub assets: Vec<Asset>,
    pub policies: Vec<String>,
}
impl Manifest {
    pub fn new(sources: Vec<Source>) -> Self {
        Self { schema_version: 1, contract: "proposed-sa2-only".into(), game: "sonic-adventure-2-battle".into(),
            revision: "steam-pc; exact build identified per source SHA-256".into(), sources, assets: vec![],
            policies: vec!["Native coordinates, hierarchy, proportions and RGBA alpha retained; no awake-eye/body-opacity adaptation applied".into(),
                "Inventory candidates and decoded representation do not establish semantic use or gameplay parity".into(),
                "Original files, decoded values, converted outputs and detailed reports remain ignored/local".into()] }
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.contract != "proposed-sa2-only"
            || self.game != "sonic-adventure-2-battle"
            || self.revision.trim().is_empty()
        {
            return Err(Error::invalid(
                "unsupported manifest contract/game/revision",
            ));
        }
        if self.sources.len() > 100_000 || self.assets.len() > 100_000 {
            return Err(Error::invalid("manifest record limit"));
        }
        let sources = self
            .sources
            .iter()
            .map(|s| (s.id.as_str(), s))
            .collect::<BTreeMap<_, _>>();
        let assets = self
            .assets
            .iter()
            .map(|a| (a.id.as_str(), a))
            .collect::<BTreeMap<_, _>>();
        let mut ids = BTreeSet::new();
        for source in &self.sources {
            logical_path(&source.logical_path)?;
            if !source.id.starts_with("sa2.pc.")
                || !ids.insert(source.id.as_str())
                || !valid_hash(&source.sha256)
            {
                return Err(Error::invalid("invalid or duplicate source identity/hash"));
            }
        }
        for asset in &self.assets {
            if !asset.id.starts_with("sa2.pc.")
                || !ids.insert(asset.id.as_str())
                || !valid_hash(&asset.span.sha256)
            {
                return Err(Error::invalid("invalid or duplicate asset identity/hash"));
            }
            let source = sources
                .get(asset.span.source_id.as_str())
                .ok_or_else(|| Error::invalid("unknown span source"))?;
            let end = asset
                .span
                .offset
                .checked_add(asset.span.byte_length)
                .ok_or_else(|| Error::invalid("manifest span overflow"))?;
            match (&asset.span.parent_id, asset.span.address_space.as_str()) {
                (None, "file") if end <= source.byte_length => {
                    if asset.span.offset == 0
                        && asset.span.byte_length == source.byte_length
                        && asset.span.sha256 != source.sha256
                    {
                        return Err(Error::invalid("whole-source span hash mismatch"));
                    }
                }
                (Some(parent), "decoded-container") => {
                    let container = assets
                        .get(parent.as_str())
                        .ok_or_else(|| Error::invalid("unknown decoded container"))?;
                    if parent == &asset.id
                        || container.span.source_id != asset.span.source_id
                        || container.span.parent_id.is_some()
                        || container.representation != Representation::Decoded
                    {
                        return Err(Error::invalid("invalid decoded container relationship"));
                    }
                    let output = container
                        .output
                        .as_ref()
                        .ok_or_else(|| Error::invalid("decoded parent has no output identity"))?;
                    if end > output.byte_length || output.format != "prs-decoded" {
                        return Err(Error::invalid("decoded span outside supported container"));
                    }
                }
                _ => return Err(Error::invalid("invalid span address space/range")),
            }
            if let Some(output) = &asset.output {
                logical_path(&output.logical_path)?;
                if !valid_hash(&output.sha256) {
                    return Err(Error::invalid("invalid output hash"));
                }
            }
            for dependency in &asset.dependencies {
                let source = sources
                    .get(dependency.source_id.as_str())
                    .ok_or_else(|| Error::invalid("unknown dependency source"))?;
                let end = dependency
                    .offset
                    .checked_add(dependency.byte_length)
                    .ok_or_else(|| Error::invalid("dependency range overflow"))?;
                if end > source.byte_length || !valid_hash(&dependency.sha256) {
                    return Err(Error::invalid("invalid dependency range/hash"));
                }
                if dependency.bank.is_some() != dependency.entry_count.is_some()
                    || dependency.entry_count.is_some_and(|n| n != 16 && n != 256)
                {
                    return Err(Error::invalid("invalid palette dependency"));
                }
            }
            if asset.fidelity == Fidelity::Unknown && asset.unknowns.is_empty() {
                return Err(Error::invalid("unknown fidelity needs an explanation"));
            }
        }
        Ok(())
    }
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn file_hash(path: &Path) -> Result<(u64, String)> {
    let mut file = fs::File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(Error::invalid("source must be a regular file"));
    }
    let mut digest = Sha256::new();
    let mut buf = [0u8; 65536];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        digest.update(&buf[..n]);
    }
    let after = file.metadata()?;
    if size != before.len()
        || size != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(Error::invalid("source changed while hashing"));
    }
    Ok((size, format!("{:x}", digest.finalize())))
}

/// Filename hints, deliberately independent from a successful format decode.
pub fn category(path: &str) -> &'static str {
    let p = path.to_ascii_lowercase();
    if p.contains("odekake") || p.contains("transporter") {
        "transporter-candidate"
    } else if p.ends_with(".afs")
        || p.ends_with(".adx")
        || p.ends_with(".csb")
        || p.ends_with(".ahx")
    {
        "audio-candidate"
    } else if p.ends_with(".gvp") {
        "palette-candidate"
    } else if p.contains("al_tex_") || p.contains("al_icon") {
        "ui-candidate"
    } else if p.ends_with(".dds") || p.ends_with(".gvr") || p.contains("_tex") {
        "texture-candidate"
    } else if p.ends_with(".saanim") || p.contains("motion") {
        "motion-candidate"
    } else if p.ends_with(".sa2blvl")
        || p.ends_with(".sa2lvl")
        || p.contains("chaostg")
        || p.contains("landtable")
    {
        "stage-collision-candidate"
    } else if p.ends_with(".sa2mdl") || p.ends_with(".sa2bmdl") {
        "model-rig-candidate"
    } else if p.ends_with(".prs") || p.ends_with(".pak") {
        "archive-candidate"
    } else {
        "unknown"
    }
}

pub fn catalog(root: &Path) -> Result<Manifest> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(Error::invalid("catalog root cannot be a link"));
    }
    fn walk(root: &Path, dir: &Path, out: &mut Vec<Source>) -> Result<()> {
        let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let ty = entry.file_type()?;
            let path = entry.path();
            if ty.is_symlink() {
                return Err(Error::invalid("catalog refuses symlinks"));
            }
            if ty.is_dir() {
                walk(root, &path, out)?;
            } else if ty.is_file() {
                if out.len() >= 100_000 {
                    return Err(Error::invalid("catalog file limit"));
                }
                let label = path
                    .strip_prefix(root)
                    .map_err(|e| Error::invalid(e.to_string()))?
                    .to_str()
                    .ok_or_else(|| Error::invalid("non-UTF8 source path"))?
                    .replace('\\', "/");
                logical_path(&label)?;
                let (byte_length, sha256) = file_hash(&path)?;
                if out.len() == 100_000 {
                    return Err(Error::invalid("catalog file count limit"));
                }
                out.push(Source {
                    id: format!("sa2.pc.source.{label}"),
                    logical_path: label,
                    byte_length,
                    sha256,
                });
            } else {
                return Err(Error::invalid("catalog refuses nonregular files"));
            }
        }
        Ok(())
    }
    let mut sources = vec![];
    walk(root, root, &mut sources)?;
    let mut result = Manifest::new(sources);
    for source in &result.sources {
        result.assets.push(Asset {
            id: format!("sa2.pc.asset.{}", source.logical_path),
            category: category(&source.logical_path).into(),
            representation: Representation::Opaque,
            fidelity: Fidelity::Unknown,
            span: Span {
                source_id: source.id.clone(),
                address_space: "file".into(),
                parent_id: None,
                offset: 0,
                byte_length: source.byte_length,
                sha256: source.sha256.clone(),
            },
            output: None,
            dependencies: vec![],
            unknowns: vec![
                "Filename category only; internal resources and native use not established".into(),
            ],
        });
    }
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_paths_reject_private_paths_and_traversal() {
        for bad in [
            "/etc/file",
            "C:/private/file",
            "a\\b",
            "../file",
            "a/../b",
            "./file",
            "a//b",
            "a/./b",
            "",
        ] {
            assert!(logical_path(bad).is_err(), "{bad}");
        }
        assert!(logical_path("resource/gd_PC/al_body.prs").is_ok());
    }
    #[test]
    fn hashes_and_manifest_relationships_are_validated() {
        let s = Source::from_bytes("a.prs", b"abc").unwrap();
        assert_eq!(
            s.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let mut m = Manifest::new(vec![s]);
        m.validate().unwrap();
        let source = &m.sources[0];
        m.assets.push(Asset {
            id: "sa2.pc.asset.a".into(),
            category: "archive".into(),
            representation: Representation::Opaque,
            fidelity: Fidelity::Unknown,
            span: Span {
                source_id: source.id.clone(),
                address_space: "file".into(),
                parent_id: None,
                offset: 1,
                byte_length: 3,
                sha256: source.sha256.clone(),
            },
            output: None,
            dependencies: vec![],
            unknowns: vec!["not decoded".into()],
        });
        assert!(m.validate().is_err());
        m.assets[0].span.offset = 0;
        m.validate().unwrap();
        m.assets[0].span.sha256 = "A".repeat(64);
        assert!(m.validate().is_err());
    }
}
