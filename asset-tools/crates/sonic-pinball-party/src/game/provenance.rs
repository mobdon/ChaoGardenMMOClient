//! Reproducible offsets/hashes for source-port evidence, never a completion score.
use crate::{binary, catalog, sha256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRegistry {
    pub schema_version: u32,
    pub rom_sha256: String,
    pub whole_game_ported: bool,
    pub scope: String,
    pub spans: Vec<SourceSpan>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub fixture: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub space: SourceSpace,
    pub offset: usize,
    pub length: usize,
    pub sha256: String,
}
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceSpace {
    #[default]
    Rom,
    Garden,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReport {
    pub source_sha256: &'static str,
    pub verified_evidence_spans: usize,
    pub whole_game_ported: bool,
}
pub fn registry() -> Result<SourceRegistry, String> {
    let registry: SourceRegistry = serde_json::from_str(include_str!("../../engine-sources.json"))
        .map_err(|e| e.to_string())?;
    if registry.schema_version != 1
        || registry.rom_sha256 != catalog::ROM_SHA256
        || registry.whole_game_ported
    {
        return Err("unsupported engine evidence registry".into());
    }
    let mut keys = std::collections::BTreeSet::new();
    for span in &registry.spans {
        if span.length == 0
            || !span.offset.checked_add(span.length).is_some_and(|end| {
                end <= match span.space {
                    SourceSpace::Rom => catalog::ROM_LEN,
                    SourceSpace::Garden => catalog::GARDEN_LEN,
                }
            })
            || span.sha256.len() != 64
            || !span.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || !keys.insert((&span.fixture, &span.key))
        {
            return Err(format!(
                "invalid engine source span {}:{}",
                span.fixture, span.key
            ));
        }
    }
    Ok(registry)
}
pub fn verify_rom(rom: &[u8]) -> Result<SourceReport, String> {
    catalog::validate_rom(rom).map_err(|e| e.to_string())?;
    let registry = registry()?;
    let garden = if registry
        .spans
        .iter()
        .any(|s| s.space == SourceSpace::Garden)
    {
        Some(catalog::extract_garden(rom).map_err(|e| e.to_string())?)
    } else {
        None
    };
    for span in &registry.spans {
        let source = match span.space {
            SourceSpace::Rom => rom,
            SourceSpace::Garden => &garden.as_ref().expect("Garden requested above").bytes,
        };
        let bytes = binary::span(source, span.offset, span.length).map_err(|e| e.to_string())?;
        if sha256(bytes) != span.sha256 {
            return Err(format!(
                "engine source fingerprint differs: {}:{}",
                span.fixture, span.name
            ));
        }
    }
    Ok(SourceReport {
        source_sha256: catalog::ROM_SHA256,
        verified_evidence_spans: registry.spans.len(),
        whole_game_ported: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_bounded_and_does_not_assert_completion() {
        let value = registry().unwrap();
        assert!(!value.spans.is_empty());
        assert!(!value.whole_game_ported);
        assert!(verify_rom(&[]).is_err());
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; every engine evidence span"]
    fn all_engine_evidence_spans_match_local_native_source() {
        let path = std::env::var_os("SPP_ROM").expect("Set SPP_ROM");
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .unwrap()
            .take((catalog::ROM_LEN + 1) as u64)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(!verify_rom(&bytes).unwrap().whole_game_ported);
    }
}
