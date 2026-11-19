use sa2_asset_tools::{
    board_model::{self, BoardModel::*},
    model::{read_model, Model},
    visual::evaluate_static_model,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn comparable(m: &Model) -> Value {
    let node = |p| m.nodes.iter().position(|n| n.offset == p);
    let nodes: Vec<_> = m
        .nodes
        .iter()
        .map(|n| {
            json!([
                n.flags,
                n.position,
                n.rotation_bams,
                n.scale,
                node(n.child_offset),
                node(n.sibling_offset),
                n.parent_offset.and_then(node),
                m.attachments
                    .iter()
                    .position(|a| a.offset == n.attach_offset)
            ])
        })
        .collect();
    let attachments: Vec<_> = m
        .attachments
        .iter()
        .map(|a| {
            let vs: Vec<_> = a
                .vertex_chunks
                .iter()
                .map(|v| {
                    json!([
                        v.kind,
                        v.flags,
                        v.index_offset,
                        v.vertices.iter().map(|x| &x.raw_words).collect::<Vec<_>>()
                    ])
                })
                .collect();
            let ps: Vec<_> = a.polygon_chunks.iter().map(|p| &p.raw_words).collect();
            json!([a.bounds_center, a.bounds_radius, vs, ps])
        })
        .collect();
    json!([nodes, attachments])
}

#[test]
#[ignore = "requires installed PC game and independent SA Tools 786 split/PNG exports"]
fn exact_board_payloads_and_texture_pixels() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let reference = PathBuf::from(std::env::var_os("SA2_BOARD_REFERENCE").unwrap());
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    let rel_bytes = fs::read(game.join("resource/gd_PC/stg13D.rel")).unwrap();
    let rel = sa2_asset_tools::city_rel::CityRel::read(&rel_bytes).unwrap();
    let resources = rel.board_resources().unwrap();
    let data = rel.data(5).unwrap();
    let word =
        |off: u32| u32::from_be_bytes(data[off as usize..off as usize + 4].try_into().unwrap());
    let archive = fs::read(game.join("resource/gd_PC/objtex_stg13.prs")).unwrap();
    for (id, name) in [
        (Streetboard, "Streetboard"),
        (ShadowBoard, "ShadowBoard"),
        (AmyBoard, "AmyBoard"),
        (MetalBoard, "MetalSonicBoard"),
        (SonicBoard, "SonicBoard"),
        (CastShadow, "Board_Shadow"),
        (Modifier, "Board_Modifier"),
    ] {
        let x = board_model::extract(&exe, id).unwrap();
        let model = read_model(&x.sa2mdl).unwrap();
        let expected =
            fs::read(reference.join(format!("split/stg13_cityescape/models/{name}.sa2mdl")))
                .unwrap();
        let mut actual = comparable(&model);
        let oracle = comparable(&read_model(&expected).unwrap());
        let first = actual[1][0][3][0].as_array_mut().unwrap();
        if id == Modifier {
            // SA Tools strips the one native zero alignment word.
            assert_eq!(
                (first[0].as_u64(), first[1].as_u64(), first.pop()),
                (Some(56), Some(56), Some(json!(0)))
            );
            first[1] = json!(55);
        } else {
            // SA Tools serializes the unused high byte of Chunk ambient RGB as 0.
            assert_eq!(first[0], json!(0x2513));
            let ambient = first[5].as_u64().unwrap();
            assert_eq!(ambient >> 8, 255);
            first[5] = json!(ambient & 255);
        }
        assert_eq!(actual, oracle, "{name}");
        if !x.texture_names.is_empty() {
            let key = match id {
                Streetboard => "single_player",
                ShadowBoard => "two_player_shadow",
                AmyBoard => "two_player_amy",
                MetalBoard => "two_player_metal_sonic",
                SonicBoard => "two_player_default",
                _ => unreachable!(),
            };
            let rr = resources.iter().find(|r| r.variant == key).unwrap();
            assert_eq!(x.texture_names, rr.textures);
            let (sec, child) = rel.local_pointer(5, rr.model + 44).unwrap().unwrap();
            assert_eq!(sec, 5);
            assert_eq!(model.nodes.len(), 2);
            for (node, r) in model.nodes.iter().zip([rr.model, child]) {
                assert_eq!(node.flags, word(r));
                assert_eq!(node.flags & 0x40, 0);
                for j in 0..3 {
                    assert_eq!(node.position[j].to_bits(), word(r + 8 + 4 * j as u32));
                    assert_eq!(node.rotation_bams[j] as u32, word(r + 20 + 4 * j as u32));
                    assert_eq!(node.scale[j].to_bits(), word(r + 32 + 4 * j as u32));
                }
                assert!(rel.local_pointer(5, r + 48).unwrap().is_none());
            }
            assert!(rel.local_pointer(5, child + 44).unwrap().is_none());
            let texlist = fs::read_to_string(
                reference.join(format!("split/stg13_cityescape/tls/{name}.satex")),
            )
            .unwrap();
            let names: Vec<_> = texlist
                .lines()
                .filter(|l| l.starts_with("TextureNames["))
                .map(|l| l.split_once('=').unwrap().1)
                .collect();
            assert_eq!(x.texture_names, names);
        }
        for (n, image) in x
            .texture_names
            .iter()
            .zip(board_model::textures(&archive, &x.texture_names).unwrap())
        {
            let bytes = fs::read(reference.join(format!("objtex_stg13/{n}.png"))).unwrap();
            let mut decoder = png::Decoder::new(&bytes[..]);
            decoder.set_transformations(png::Transformations::EXPAND);
            let mut reader = decoder.read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            assert_eq!((info.width, info.height), (image.width, image.height));
            let rgba: Vec<u8> = match info.color_type {
                png::ColorType::Rgba => pixels,
                png::ColorType::Rgb => pixels
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .flat_map(|p| [p[0], p[1], p[2], 255])
                    .collect(),
                _ => panic!("unexpected oracle PNG"),
            };
            assert_eq!(image.rgba, rgba, "{n}");
        }
        if !x.texture_names.is_empty() {
            let glb = board_model::board_glb(&exe, &archive, id).unwrap();
            let n = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
            let doc: Value = serde_json::from_slice(&glb[20..20 + n]).unwrap();
            let materials = doc["materials"].as_array().unwrap();
            assert_eq!(
                materials
                    .iter()
                    .map(|m| m["alphaMode"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                if id == Streetboard {
                    vec!["OPAQUE", "OPAQUE", "BLEND"]
                } else {
                    vec!["OPAQUE"]
                }
            );
            let primitives = doc["meshes"][0]["primitives"].as_array().unwrap();
            let triangles: usize = primitives
                .iter()
                .map(|p| {
                    doc["accessors"][p["attributes"]["POSITION"].as_u64().unwrap() as usize]
                        ["count"]
                        .as_u64()
                        .unwrap() as usize
                        / 3
                })
                .sum();
            assert_eq!(triangles, if id == Streetboard { 60 } else { 124 });
        }
        eprintln!(
            "{name}: nodes={} visual={:?}",
            model.nodes.len(),
            evaluate_static_model(&model).map(|v| v
                .surfaces
                .iter()
                .map(|s| (
                    s.strip_kind,
                    s.strip_flags,
                    s.material.blend_flags,
                    s.strips.iter().map(|s| s.triangles.len()).sum::<usize>()
                ))
                .collect::<Vec<_>>())
        );
    }
}
