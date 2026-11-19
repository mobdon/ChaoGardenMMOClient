use sonic_advance_assets::{binary, catalog};
use std::collections::BTreeSet;

#[test]
fn public_catalog_is_bounded_hash_metadata_and_covers_requested_domains() {
    let records = catalog::records().unwrap();
    assert_eq!(records.len(), 667);
    let mut ids = BTreeSet::new();
    let mut domains = BTreeSet::new();
    for r in &records {
        assert!(ids.insert(&r.id), "duplicate id {}", r.id);
        assert!(r.id.starts_with("sonic-advance.asoe.r0."));
        assert!(r
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-_".contains(c)));
        domains.insert(r.domain.as_str());
        let cap = match r.address_space.as_str() {
            "rom" => binary::ROM_LEN,
            "garden-decoded" => catalog::GARDEN_LEN,
            _ => panic!("unknown space"),
        };
        assert!(r.length > 0 && r.offset.checked_add(r.length).unwrap() <= cap);
        assert_eq!(r.sha256.len(), 64);
        assert!(r
            .sha256
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
        assert!(matches!(r.status.as_str(), "decoded" | "opaque"));
        assert!(matches!(r.codec.as_str(), "raw" | "lz77" | "rle"));
        assert!(!r.evidence.is_empty());
    }
    for domain in [
        "sprites",
        "palettes",
        "tiles",
        "maps",
        "animations",
        "ui",
        "audio",
        "saves",
        "transfers",
        "tiny-garden",
    ] {
        assert!(domains.contains(domain), "{domain}");
    }
    let p: serde_json::Value = serde_json::from_str(include_str!("../provenance.json")).unwrap();
    assert_eq!(p["catalogRecordCount"].as_u64(), Some(records.len() as u64));
    assert_eq!(p["romSha256"], binary::ROM_SHA256);
}
