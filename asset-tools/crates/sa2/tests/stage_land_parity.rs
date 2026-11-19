use sa2_asset_tools::{collision, stage, stage_land};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

#[derive(Deserialize)]
struct Row {
    filename: String,
    symbol: String,
    oracle: PathBuf,
}

fn compare(test_level: bool, a: &stage::Stage, b: &stage::Stage) -> Result<[usize; 4], String> {
    macro_rules! same {
        ($x:expr,$y:expr,$loc:expr) => {
            if $x != $y {
                return Err(format!("{}: {}", $loc, stringify!($x)));
            }
        };
    }
    same!(a.entries.len(), b.entries.len(), "table");
    let reference_index = |i: usize| if test_level { [3, 0, 4, 1, 2, 5][i] } else { i };
    if test_level {
        same!(a.entries.len(), 6, "test level");
        same!((a.visual_count, b.visual_count), (-1, 3), "test level");
    } else {
        same!(a.visual_count, b.visual_count, "table");
    }
    same!(a.far_clip, b.far_clip, "table");
    for x in &a.entries {
        let y = &b.entries[reference_index(x.index)];
        let id = x.index;
        same!(
            (&x.geometry_format, x.surface_flags),
            (&y.geometry_format, y.surface_flags),
            id
        );
        same!(x.bounds_center, y.bounds_center, id);
        same!(x.bounds_radius, y.bounds_radius, id);
        same!(x.nodes.len(), y.nodes.len(), id);
        for (n, m) in x.nodes.iter().zip(&y.nodes) {
            // SAModel sets evaluation skip bits when an authored vector is identity.
            let mut normalized = n.flags;
            if n.position == [0.; 3] {
                normalized |= 1;
            }
            if n.rotation_bams == [0; 3] {
                normalized |= 2;
            }
            let allowed = normalized ^ n.flags;
            same!(m.flags & !allowed, n.flags, id);
            same!(n.position, m.position, id);
            same!(n.rotation_bams, m.rotation_bams, id);
            same!(n.scale, m.scale, id);
        }
    }
    let a = collision::bake_stage(a, 1.).map_err(|e| format!("native bake: {e}"))?;
    let b = collision::bake_stage(b, 1.).map_err(|e| format!("SA Tools bake: {e}"))?;
    same!(a.meshes.len(), b.meshes.len(), "collision");
    let mut out = [a.entries.len(), a.meshes.len(), 0, 0];
    for (x, y) in a.meshes.iter().zip(&b.meshes) {
        same!(reference_index(x.entry_index), y.entry_index, "mesh order");
        same!(x.vertices, y.vertices, x.entry_index);
        same!(x.triangles, y.triangles, x.entry_index);
        out[2] += x.vertices.len();
        out[3] += x.triangles.len();
    }
    Ok(out)
}

#[test]
#[ignore = "requires local original game and independently split SA Tools landtable files"]
fn installed_tables_match_independent_splits() {
    let root = PathBuf::from(std::env::var_os("SA2_SET_DIR").expect("SA2_SET_DIR"));
    let manifest =
        fs::read(std::env::var_os("SA2_LAND_ORACLES").expect("SA2_LAND_ORACLES")).unwrap();
    let rows: Vec<Row> = serde_json::from_slice(&manifest).unwrap();
    let mut refmap = BTreeMap::new();
    for row in rows {
        assert!(refmap
            .insert((row.filename.to_ascii_lowercase(), row.symbol), row.oracle)
            .is_none());
    }
    let mut files = BTreeSet::new();
    let mut total = [0; 4];
    let mut errors = Vec::new();
    let mut proved = 0;
    let mut unsupported = 0;
    for c in stage_land::catalog() {
        let key = (c.filename.to_ascii_lowercase(), c.symbol.to_owned());
        let oracle = refmap
            .remove(&key)
            .expect("catalog entry missing from oracle manifest");
        files.insert(c.filename);
        let data = fs::read(root.join(c.filename)).unwrap();
        let native = stage_land::read_named(&data, c.symbol);
        if !c.supported {
            unsupported += 1;
            if native.is_ok() {
                errors.push(format!("{}:{} unsupported accepted", c.filename, c.symbol));
            }
            continue;
        }
        let result = (|| {
            let native = native.map_err(|e| format!("native decode: {e}"))?;
            let bytes = fs::read(&oracle).map_err(|e| format!("missing independent split: {e}"))?;
            let stage =
                stage::read_stage(&bytes).map_err(|e| format!("SA Tools split decode: {e}"))?;
            compare(c.filename == "stg00D.rel", &native, &stage)
        })();
        match result {
            Ok(counts) => {
                proved += 1;
                for (i, n) in counts.iter().enumerate() {
                    total[i] += n;
                }
                println!("OK {} {} {counts:?}", c.filename, c.symbol);
            }
            Err(e) => errors.push(format!("{}:{} {e}", c.filename, c.symbol)),
        }
    }
    println!("{} files, {proved} tables, {unsupported} unsupported, entries/meshes/vertices/triangles: {total:?}",files.len());
    assert!(
        refmap.is_empty(),
        "oracle manifest has entries missing from catalog"
    );
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    assert_eq!((files.len(), proved, unsupported), (79, 102, 0));
    assert_eq!(total, [27936, 8816, 258977, 297210]);
}
