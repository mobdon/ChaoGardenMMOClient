use sa2_asset_tools::event;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Reference {
    scenes: Vec<Scene>,
    motions: BTreeMap<i32, Option<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Scene {
    frame_count: u32,
    entities: Vec<Entity>,
    camera_motions: Vec<Option<String>>,
    particle_motions: Vec<Option<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Entity {
    model: String,
    #[serde(rename = "GCModel")]
    gc_model: String,
    shadow_model: String,
    motion: String,
    shape_motion: String,
    position: String,
    flags: u32,
    layer: u32,
}

fn game_events() -> PathBuf {
    PathBuf::from(std::env::var_os("SA2_SET_DIR").expect("set SA2_SET_DIR to resource/gd_PC"))
        .join("event")
}

#[test]
#[ignore = "requires the local game and SA2_EVENT_REFERENCE containing splitEvent exports"]
fn compares_original_story_events_with_sa_tools() {
    let reference = PathBuf::from(std::env::var_os("SA2_EVENT_REFERENCE").unwrap());
    for id in ["e0000", "e0100", "e0200", "e0207"] {
        let path = reference.join(id).join(id).join(format!("{id}.json"));
        let source = std::fs::read_to_string(path).unwrap();
        let expected: Reference =
            serde_json::from_str(source.trim_start_matches('\u{feff}')).unwrap();
        let bytes = std::fs::read(game_events().join(format!("{id}.prs"))).unwrap();
        let actual = event::read_prs(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(actual.scenes.len(), expected.scenes.len(), "{id}");
        for (scene_id, (scene, reference)) in actual.scenes.iter().zip(expected.scenes).enumerate()
        {
            assert_eq!(
                scene.frames, reference.frame_count,
                "{id} scene {scene_id} duration"
            );
            assert_eq!(
                scene.entities.len(),
                reference.entities.len(),
                "{id} scene {scene_id} entities"
            );
            for (kind, indices, names) in [
                ("camera", &scene.camera_motions, reference.camera_motions),
                (
                    "particle",
                    &scene.particle_motions,
                    reference.particle_motions,
                ),
            ] {
                let resolved: Vec<_> = indices
                    .iter()
                    .map(|i| {
                        expected
                            .motions
                            .get(i)
                            .unwrap_or_else(|| {
                                panic!("{id} scene {scene_id}: unknown {kind} motion {i}")
                            })
                            .clone()
                    })
                    .collect();
                assert_eq!(resolved, names, "{id} scene {scene_id} {kind}");
            }
            for (index, (entity, ref_entity)) in
                scene.entities.iter().zip(reference.entities).enumerate()
            {
                assert_eq!(
                    (entity.flags, entity.layer),
                    (ref_entity.flags, ref_entity.layer),
                    "{id} scene {scene_id} entity {index}"
                );
                for (offset, name) in [
                    (entity.model_offset, &ref_entity.model),
                    (entity.gc_model_offset, &ref_entity.gc_model),
                    (entity.shadow_model_offset, &ref_entity.shadow_model),
                ] {
                    let reference = if name == "null" {
                        None
                    } else {
                        Some(
                            usize::from_str_radix(name.strip_prefix("object_").unwrap(), 16)
                                .unwrap(),
                        )
                    };
                    assert_eq!(
                        offset, reference,
                        "{id} scene {scene_id} entity {index} model"
                    );
                }
                for (motion, name) in [
                    (entity.motion_index, &ref_entity.motion),
                    (entity.shape_motion_index, &ref_entity.shape_motion),
                ] {
                    let reference = expected
                        .motions
                        .get(&motion)
                        .unwrap_or_else(|| panic!("unknown entity motion {motion}"));
                    assert_eq!(
                        reference.as_deref().unwrap_or("null"),
                        name,
                        "{id} scene {scene_id} entity {index} motion"
                    );
                }
                let coords: Vec<f64> = ref_entity
                    .position
                    .split(',')
                    .map(|v| v.trim().parse().unwrap())
                    .collect();
                assert_eq!(coords.len(), 3);
                for (axis, (v, reference)) in entity.position.iter().zip(coords).enumerate() {
                    // SA Tools writes Vertex.ToString with seven significant decimal digits.
                    let tolerance = if reference == 0.0 {
                        0.0
                    } else {
                        0.500001 * 10f64.powi(reference.abs().log10().floor() as i32 - 6)
                    };
                    assert!(
                        (f64::from(*v) - reference).abs() <= tolerance,
                        "{id} scene {scene_id} entity {index} axis {axis}: {v} vs {reference}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires all numbered original PC event files in SA2_SET_DIR/event"]
fn parses_all_numbered_original_event_files() {
    let mut total = 0;
    let mut scenes = 0;
    let mut entities = 0;
    let mut errors = Vec::new();
    for entry in std::fs::read_dir(game_events()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        let bytes = name.as_bytes();
        if bytes.len() != 9
            || bytes[0] != b'e'
            || !bytes[1..5].iter().all(u8::is_ascii_digit)
            || &bytes[5..] != b".prs"
        {
            continue;
        }
        total += 1;
        match event::read_prs(&std::fs::read(entry.path()).unwrap()) {
            Ok(event) => {
                scenes += event.scenes.len();
                entities += event.scenes.iter().map(|s| s.entities.len()).sum::<usize>();
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    errors.sort();
    eprintln!("{total} files; {scenes} scenes, {entities} entities in successful reads");
    assert_eq!(total, 67);
    assert_eq!(errors, ["e0350.prs: expected a retail BATTLE event image"]);
    assert_eq!((scenes, entities), (741, 8132));
}
