use sa2_asset_tools::{
    archive, audio, city_replay,
    city_stage::CityLayout,
    collision, glb_cli,
    manifest::{self, Asset, Dependency, Fidelity, Manifest, Output, Representation, Source, Span},
    model, motion, object_table, pe, pose, set, stage, texture, visual, Error, Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const MAX_INPUT: u64 = 512 * 1024 * 1024;
mod player_commands;
mod story_effects;
fn city_collision(bytes: &[u8]) -> Result<stage::Stage> {
    if bytes.starts_with(b"MZ") {
        sa2_asset_tools::city_land::read_win32_collision_stage(bytes)
    } else {
        sa2_asset_tools::city_land::read_collision_stage(bytes)
    }
}

fn board_variant(value: &std::ffi::OsStr) -> Result<sa2_asset_tools::board_model::BoardModel> {
    use sa2_asset_tools::board_model::BoardModel::*;
    match value.to_str() {
        Some("street") => Ok(Streetboard),
        Some("shadow") => Ok(ShadowBoard),
        Some("amy") => Ok(AmyBoard),
        Some("metal") => Ok(MetalBoard),
        Some("sonic") => Ok(SonicBoard),
        Some("cast-shadow") => Ok(CastShadow),
        Some("modifier") => Ok(Modifier),
        _ => Err(Error::invalid(
            "expected board variant street, shadow, amy, metal, sonic, cast-shadow or modifier",
        )),
    }
}

const MAX_DECODED: usize = 128 * 1024 * 1024;

fn read_source(path: &Path) -> Result<Vec<u8>> {
    read_source_limited(path, MAX_INPUT)
}
fn read_source_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(Error::invalid("source link refused"));
    }
    let mut file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::invalid("expected regular input"));
    }
    if file.metadata()?.len() > limit {
        return Err(Error::invalid("input exceeds its configured byte limit"));
    }
    let mut bytes = vec![];
    (&mut file).take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::invalid("input limit"));
    }
    Ok(bytes)
}

/// All writes require an explicit .local path, refuse symlink ancestors and
/// existing destinations. Installed sources are never opened for writing.
fn local_output(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() && path.components().any(|c| matches!(c, Component::Prefix(_))) {
        return Err(Error::invalid("drive-relative output refused"));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if absolute
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(Error::invalid("output must not contain parent traversal"));
    }
    // A worktree may itself be inside the original checkout's .local folder.
    // Require .local below the NEAREST checkout to avoid writing tracked source.
    let root = absolute
        .ancestors()
        .find(|p| p.join(".git").exists())
        .ok_or_else(|| Error::invalid("output must belong to a Git checkout"))?;
    if !absolute
        .strip_prefix(root)
        .map_err(|e| Error::invalid(e.to_string()))?
        .components()
        .any(|c| c.as_os_str() == ".local")
    {
        return Err(Error::invalid(
            "output must be beneath the checkout's ignored .local directory",
        ));
    }
    let mut prefix = PathBuf::new();
    for c in absolute.components() {
        prefix.push(c.as_os_str());
        match fs::symlink_metadata(&prefix) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::invalid("output symlink ancestor refused"))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(absolute)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let path = local_output(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = result {
        fs::remove_file(&path)?;
        return Err(error.into());
    }
    Ok(())
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_new(path, &bytes)
}
fn basename(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::invalid("input basename must be UTF8"))?;
    manifest::logical_path(name)?;
    Ok(name.into())
}
fn source_label(path: &Path, root: Option<&Path>) -> Result<String> {
    if let Some(root) = root {
        let root = root.canonicalize()?;
        let path = path.canonicalize()?;
        let label = path
            .strip_prefix(root)
            .map_err(|_| Error::invalid("source is outside --source-root"))?
            .to_str()
            .ok_or_else(|| Error::invalid("source path encoding"))?
            .replace('\\', "/");
        manifest::logical_path(&label)?;
        Ok(label)
    } else {
        basename(path)
    }
}
fn output_record(path: &Path, format: &str, bytes: &[u8]) -> Result<Output> {
    Ok(Output {
        logical_path: basename(path)?,
        format: format.into(),
        byte_length: bytes.len() as u64,
        sha256: manifest::sha256(bytes),
    })
}
fn asset(
    source: &Source,
    id: String,
    category: &str,
    span: Span,
    output: Option<Output>,
    representation: Representation,
    unknowns: Vec<String>,
) -> Asset {
    Asset {
        id: format!("sa2.pc.asset.{}.{}", source.logical_path, id),
        category: category.into(),
        span,
        output,
        representation,
        fidelity: Fidelity::Unknown,
        dependencies: vec![],
        unknowns,
    }
}
fn whole_span(source: &Source) -> Span {
    Span {
        source_id: source.id.clone(),
        address_space: "file".into(),
        parent_id: None,
        offset: 0,
        byte_length: source.byte_length,
        sha256: source.sha256.clone(),
    }
}
fn sidecar(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".manifest.json");
    PathBuf::from(p)
}
fn save_conversion(
    input: &Path,
    label: &str,
    source_bytes: &[u8],
    output: &Path,
    output_bytes: &[u8],
    format: &str,
    unknowns: Vec<String>,
) -> Result<()> {
    save_named_conversion(
        input,
        label,
        source_bytes,
        output,
        output_bytes,
        format,
        "converted".into(),
        Representation::Decoded,
        unknowns,
    )
}
#[allow(clippy::too_many_arguments)]
fn save_named_conversion(
    input: &Path,
    label: &str,
    source_bytes: &[u8],
    output: &Path,
    output_bytes: &[u8],
    format: &str,
    id: String,
    representation: Representation,
    unknowns: Vec<String>,
) -> Result<()> {
    let source = Source::from_bytes(label, source_bytes)?;
    let mut m = Manifest::new(vec![source]);
    let source = &m.sources[0];
    m.assets.push(asset(
        source,
        id,
        format,
        whole_span(source),
        Some(output_record(output, format, output_bytes)?),
        representation,
        unknowns,
    ));
    m.validate()?;
    ensure_unchanged(input, source_bytes)?;
    write_pair(output, output_bytes, &m)
}

fn write_pair(output: &Path, data: &[u8], m: &Manifest) -> Result<()> {
    let receipt = local_output(&sidecar(output))?;
    if receipt.exists() {
        return Err(Error::invalid("manifest output exists"));
    }
    let mut encoded = serde_json::to_vec_pretty(m)?;
    encoded.push(b'\n');
    write_new(output, data)?;
    if let Err(error) = write_new(&receipt, &encoded) {
        fs::remove_file(local_output(output)?)?;
        return Err(error);
    }
    Ok(())
}
fn ensure_unchanged(input: &Path, bytes: &[u8]) -> Result<()> {
    let (size, hash) = manifest::file_hash(input)?;
    if size != bytes.len() as u64 || hash != manifest::sha256(bytes) {
        return Err(Error::invalid("source changed during conversion"));
    }
    Ok(())
}

struct RuntimeDependency<'a> {
    path: &'a Path,
    logical_path: String,
    bytes: &'a [u8],
    role: &'a str,
}

fn save_runtime_document(
    input: &Path,
    label: &str,
    bytes: &[u8],
    output: &Path,
    value: &impl Serialize,
    format: &str,
    dependencies: &[RuntimeDependency<'_>],
) -> Result<()> {
    save_runtime_bytes(
        input,
        label,
        bytes,
        output,
        &serde_json::to_vec_pretty(value)?,
        format,
        dependencies,
    )
}

fn save_runtime_bytes(
    input: &Path,
    label: &str,
    bytes: &[u8],
    output: &Path,
    data: &[u8],
    format: &str,
    dependencies: &[RuntimeDependency<'_>],
) -> Result<()> {
    let source = Source::from_bytes(label, bytes)?;
    let mut m = Manifest::new(vec![source]);
    let source = &m.sources[0];
    m.assets.push(asset(
        source,
        format!("native-evaluation.{format}"),
        format,
        whole_span(source),
        Some(output_record(output, format, data)?),
        Representation::Decoded,
        vec![
            "Supported source-backed native data evaluation only. Collision response, full renderer/skinning and retail gameplay remain separate; see the output's policies/unknowns and full-game tracker.".into(),
            "Generated-output identity requires the output SHA-256 as well as the source locator/hash: different frames, scales or queries may use the same source.".into(),
        ],
    ));
    for dependency in dependencies {
        ensure_unchanged(dependency.path, dependency.bytes)?;
        let source = Source::from_bytes(&dependency.logical_path, dependency.bytes)?;
        m.assets[0].dependencies.push(Dependency {
            source_id: source.id.clone(),
            role: dependency.role.into(),
            offset: 0,
            byte_length: source.byte_length,
            sha256: source.sha256.clone(),
            bank: None,
            entry_count: None,
        });
        m.sources.push(source);
    }
    m.validate()?;
    ensure_unchanged(input, bytes)?;
    write_pair(output, data, &m)
}

fn finite_argument(value: &std::ffi::OsStr) -> Result<f64> {
    value
        .to_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .ok_or_else(|| Error::invalid("expected a finite numeric argument"))
}
fn unit_scale() -> f64 {
    1.0
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RayRequest {
    origin: [f64; 3],
    direction: [f64; 3],
    max_distance: f64,
    #[serde(default = "unit_scale")]
    uniform_scale: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PoseDocument {
    schema_version: u32,
    requested_frame: f64,
    coordinate_space: &'static str,
    reference_profile: &'static str,
    frame_policy: &'static str,
    unknowns: [&'static str; 2],
    #[serde(flatten)]
    evaluated: pose::Pose,
}

fn inspect(bytes: &[u8], name: &str) -> Result<Value> {
    let name = name.to_ascii_lowercase();
    if bytes.starts_with(b"SA2MDL") || bytes.starts_with(b"SA2BMDL") {
        Ok(serde_json::to_value(model::read_model(bytes)?)?)
    } else if bytes.starts_with(b"SAANIM") {
        Ok(serde_json::to_value(motion::read_motion(bytes)?)?)
    } else if bytes.starts_with(b"SA2LVL") || bytes.starts_with(b"SA2BLVL") {
        Ok(serde_json::to_value(stage::read_stage(bytes)?)?)
    } else if bytes.starts_with(b"GVMH") {
        Ok(serde_json::to_value(archive::read_gvm(bytes)?)?)
    } else if bytes.starts_with(b"GVRT") || bytes.starts_with(b"GBIX") {
        Ok(serde_json::to_value(texture::read_gvr_header(bytes)?)?)
    } else if bytes.starts_with(b"GVPL") {
        Ok(serde_json::to_value(texture::read_gvp(bytes)?)?)
    } else if bytes.starts_with(b"AFS\0") {
        let mut rows = Vec::new();
        for entry in audio::read_afs(bytes)? {
            let header = audio::read_ahx_header(entry.bytes(bytes)?).ok();
            let mut row = serde_json::to_value(&entry)?;
            if let Some(header) = header {
                row["ahx"] = serde_json::to_value(header)?;
                row["ahx"]["frameCount"] =
                    serde_json::to_value(audio::ahx_frame_count(entry.bytes(bytes)?).ok())?;
            }
            rows.push(row);
        }
        Ok(Value::Array(rows))
    } else if bytes.starts_with(b"MZ") {
        let image = pe::read_pe(bytes)?;
        let chao = if image.exports.iter().any(|e| e.name == "AL_RootObject") {
            Some(pe::chao_candidates(bytes, &image)?)
        } else {
            None
        };
        Ok(json!({"image":image,"chaoCandidates":chao}))
    } else if name.ends_with(".ahx") {
        let mut value = serde_json::to_value(audio::read_ahx_header(bytes)?)?;
        value["frameCount"] = serde_json::to_value(audio::ahx_frame_count(bytes).ok())?;
        Ok(value)
    } else if name.ends_with(".adx") {
        Ok(serde_json::to_value(audio::read_adx_header(bytes)?)?)
    } else if name.ends_with(".pak") {
        Ok(serde_json::to_value(archive::read_pak(bytes)?)?)
    } else if bytes.starts_with(b"DDS ") {
        let image = texture::decode_dds(bytes)?;
        Ok(json!({"image":image,"rgbaSha256":manifest::sha256(&image.rgba),"mips":"base only"}))
    } else {
        Err(Error::invalid(
            "unsupported input format; preserve as opaque; use prs for explicit decompression",
        ))
    }
}

struct Entry<'a> {
    index: usize,
    name: String,
    offset: usize,
    data: &'a [u8],
    extension: &'static str,
    category: &'static str,
}
fn extract(input: &Path, label: &str, bytes: &[u8], directory: &Path) -> Result<()> {
    let directory = local_output(directory)?;
    if directory.exists() {
        return Err(Error::invalid(
            "extraction directory exists; choose a new .local directory",
        ));
    }
    let is_prs = input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("prs"));
    let decoded = if is_prs {
        archive::decompress_prs(bytes, MAX_DECODED)?
    } else {
        vec![]
    };
    let payload = if is_prs { decoded.as_slice() } else { bytes };
    let entries: Vec<Entry<'_>> = if payload.starts_with(b"GVMH") {
        archive::read_gvm(payload)?
            .into_iter()
            .enumerate()
            .map(|(index, e)| Entry {
                index,
                name: e.name,
                offset: e.offset,
                data: e.data,
                extension: "gvr",
                category: "texture",
            })
            .collect()
    } else if payload.starts_with(b"AFS\0") {
        audio::read_afs(payload)?
            .into_iter()
            .map(|e| {
                Ok(Entry {
                    index: e.index,
                    name: format!("AFS[{}]", e.index),
                    offset: e.offset,
                    data: e.bytes(payload)?,
                    extension: "bin",
                    category: "audio",
                })
            })
            .collect::<Result<_>>()?
    } else {
        archive::read_pak(payload)?
            .into_iter()
            .enumerate()
            .map(|(index, e)| Entry {
                index,
                name: e.name,
                offset: e.offset,
                data: e.data,
                extension: if e.data.starts_with(b"DDS ") {
                    "dds"
                } else {
                    "bin"
                },
                category: "opaque-pak-entry",
            })
            .collect()
    };
    let source = Source::from_bytes(label, bytes)?;
    let mut m = Manifest::new(vec![source]);
    let source = &m.sources[0];
    let parent = if is_prs {
        let decoded_path = directory.join("decoded.bin");
        let container = asset(
            source,
            "decoded-container".into(),
            "archive",
            whole_span(source),
            Some(output_record(&decoded_path, "prs-decoded", payload)?),
            Representation::Decoded,
            vec!["PRS decoding verified separately; payload semantics depend on its reader".into()],
        );
        let id = container.id.clone();
        m.assets.push(container);
        Some(id)
    } else {
        None
    };
    for e in &entries {
        let path = directory.join(format!("{:05}.{}", e.index, e.extension));
        m.assets.push(asset(
            source,
            format!("entry.{}", e.index),
            e.category,
            Span {
                source_id: source.id.clone(),
                address_space: if is_prs { "decoded-container" } else { "file" }.into(),
                parent_id: parent.clone(),
                offset: e.offset as u64,
                byte_length: e.data.len() as u64,
                sha256: manifest::sha256(e.data),
            },
            Some(output_record(&path, e.extension, e.data)?),
            Representation::Opaque,
            vec![format!(
                "Archive label: {}; raw entry extraction does not establish native use",
                e.name
            )],
        ));
    }
    m.validate()?;
    ensure_unchanged(input, bytes)?;
    if is_prs {
        write_new(&directory.join("decoded.bin"), payload)?;
    }
    for e in entries {
        write_new(
            &directory.join(format!("{:05}.{}", e.index, e.extension)),
            e.data,
        )?;
    }
    write_json(&directory.join("manifest.json"), &m)
}

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && args[0] == "--help" {
        println!("{}", player_commands::USAGE);
        println!("sa2-assets [--source-root ROOT] COMMAND INPUT OUTPUT\nCommands: catalog, inspect, prs, extract, texture, adx, ahx, event-cues, event-lighting, event-effects, chao-collision, chao-competition-tables, chao-karate-motions, transporter, validate\nevent-subtitles evmesBANK_LANG.prs OUTPUT.json [eNNNN_LANG.prs]\nevent-music EVENT.prs OUTPUT.json ADX_DIRECTORY\nevent-voices eNNNN_SUFFIX.prs OUTPUT.json sonic2app.exe\nmovie-video MOVIE.sfd OUTPUT.m1v EVENT_ID\nmovie-audio MOVIE.sfd OUTPUT.wav EVENT_ID c0|c1\ntexture INPUT OUTPUT.png [PALETTE.gvp BANK | --mip LEVEL]\npose MODEL OUTPUT.json [MOTION FRAME]\ncollision STAGE OUTPUT.json [UNIFORM_SCALE]\nraycast STAGE OUTPUT.json REQUEST.json\nvisual MODEL OUTPUT.json\nglb MODEL OUTPUT.glb BINDINGS.json\nset SET.bin OUTPUT.json\nobject-tables EXE OUTPUT.json ObjectLists.ini\nfinalhazard-boss-glb EXE OUTPUT.glb modeltex_bosslast2.prs CLIP_INDEX FRAME\nfinalhazard-weakpoint-glb EXE OUTPUT.glb modeltex_bosslast2.prs\ncity-player-keys stg13D.rel OUTPUT.json\ncity-player-motions stg13D.rel OUTPUT.json\ncity-board-inputs stg13D.rel OUTPUT.json\ncity-arrow-world SET OUTPUT.json EXE stg13D.rel REQUEST.json\ncity-arrow-display stg13D.rel OUTPUT.json MODE FRAME_COUNTER\ncity-scenery-descriptors stg13D.rel OUTPUT.json\ncity-scenery-proximity stg13D.rel OUTPUT.json LOCAL_POSITIONS.json\ncity-board-start SET OUTPUT.json EXE\ncity-layout SET OUTPUT.json EXE\ncity-replay SET OUTPUT.json EXE INDEX|bgm|board-route REQUEST.json\nUse --source-root INSTALL for catalog-compatible source IDs; otherwise inputs have isolated basename identities.\nAll output paths must be under the checkout's ignored .local; existing files are never replaced.");
        return Ok(());
    }
    let source_root = if args.first().is_some_and(|a| a == "--source-root") {
        if args.len() < 3 {
            return Err(Error::invalid("--source-root requires ROOT and a command"));
        }
        let root = PathBuf::from(&args[1]);
        args.drain(..2);
        Some(root)
    } else {
        None
    };
    if args.len() < 3 {
        return Err(Error::invalid(
            "usage: sa2-assets [--source-root ROOT] COMMAND INPUT OUTPUT (see --help)",
        ));
    }
    let command = args[0]
        .to_str()
        .ok_or_else(|| Error::invalid("command encoding"))?;
    if let Some(expected) = player_commands::arity(command) {
        if expected != args.len() {
            return Err(Error::invalid("unexpected arguments (see --help)"));
        }
        return player_commands::run(command, &args, source_root.as_deref());
    }
    if !matches!(
        command,
        "catalog"
            | "inspect"
            | "prs"
            | "extract"
            | "texture"
            | "movie-video"
            | "movie-audio"
            | "event-subtitles"
            | "event-music"
            | "event-voices"
            | "chao-collision"
            | "chao-competition-tables"
            | "chao-karate-motions"
            | "city-player-keys"
            | "city-player-motions"
            | "city-board-inputs"
            | "city-arrow-world"
            | "city-arrow-display"
            | "city-scenery-descriptors"
            | "city-scenery-proximity"
            | "event-effects"
            | "event-lighting"
            | "event-cues"
            | "ahx"
            | "adx"
            | "transporter"
            | "validate"
            | "pose"
            | "collision"
            | "raycast"
            | "set"
            | "visual"
            | "glb"
            | "object-tables"
            | "city-board-start"
            | "city-layout"
            | "finalhazard-boss-glb"
            | "finalhazard-weakpoint-glb"
            | "city-board-glb"
            | "city-board-model"
            | "city-collision"
            | "city-raycast"
            | "city-replay"
    ) {
        return Err(Error::invalid("unknown command (see --help)"));
    }
    let valid_arity = match command {
        "texture" | "pose" => matches!(args.len(), 3 | 5),
        "finalhazard-boss-glb" => args.len() == 6,
        "finalhazard-weakpoint-glb" => args.len() == 4,
        "city-board-glb" => args.len() == 5,
        "collision" | "city-collision" => matches!(args.len(), 3 | 4),
        "city-board-model" | "city-raycast" | "raycast" | "glb" | "object-tables"
        | "city-board-start" | "city-layout" => args.len() == 4,
        "city-scenery-proximity" | "event-voices" | "event-music" => args.len() == 4,
        "event-subtitles" => matches!(args.len(), 3 | 4),
        "movie-video" => args.len() == 4,
        "movie-audio" | "city-arrow-display" => args.len() == 5,
        "city-replay" | "city-arrow-world" => args.len() == 6,
        _ => args.len() == 3,
    };
    if !valid_arity {
        return Err(Error::invalid("unexpected arguments (see --help)"));
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    local_output(output)?;
    if command == "catalog" {
        if args.len() != 3 {
            return Err(Error::invalid("catalog expects INPUT OUTPUT"));
        }
        write_json(output, &manifest::catalog(input)?)?;
        println!("Wrote local source catalog");
        return Ok(());
    }
    let bytes = read_source(input)?;
    let label = source_label(input, source_root.as_deref())?;
    match command {
        "glb" => {
            let (glb, files) = glb_cli::build(&model::read_model(&bytes)?, Path::new(&args[3]))?;
            let deps = files
                .iter()
                .map(|f| {
                    Ok(RuntimeDependency {
                        path: &f.path,
                        logical_path: if source_root.is_some() {
                            source_label(&f.path, source_root.as_deref())?
                        } else {
                            f.logical_path.clone()
                        },
                        bytes: &f.bytes,
                        role: &f.role,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            save_runtime_bytes(
                input,
                &label,
                &bytes,
                output,
                &glb,
                "native-static-glb",
                &deps,
            )?;
        }
        "object-tables" => {
            let path = Path::new(&args[3]);
            let config = read_source_limited(path, 1024 * 1024)?;
            let tables = object_table::mapped_tables(
                &bytes,
                std::str::from_utf8(&config)
                    .map_err(|_| Error::invalid("object mapping is not UTF-8"))?,
            )?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &tables,
                "pc-object-tables-json",
                &[RuntimeDependency {
                    path,
                    logical_path: format!("mappings/{}", basename(path)?),
                    bytes: &config,
                    role: "object-list-mapping",
                }],
            )?;
        }
        "city-board-start" | "city-layout" | "city-replay" => {
            let path = Path::new(&args[3]);
            let image = read_source_limited(path, 128 * 1024 * 1024)?;
            let city = CityLayout::load(&bytes, &image)?;
            let mut deps = vec![RuntimeDependency {
                path,
                logical_path: source_label(path, source_root.as_deref())?,
                bytes: &image,
                role: "stage-object-executable",
            }];
            if command == "city-board-start" {
                let report = city_replay::board_start(&city, &image)?;
                save_runtime_document(
                    input,
                    &label,
                    &bytes,
                    output,
                    &report,
                    "city-board-start-json",
                    &deps,
                )?;
            } else if command == "city-layout" {
                save_runtime_document(
                    input,
                    &label,
                    &bytes,
                    output,
                    &city,
                    "city-escape-set-binding-json",
                    &deps,
                )?;
            } else {
                let request_path = Path::new(&args[5]);
                let request = read_source_limited(request_path, 1024 * 1024)?;
                deps.push(RuntimeDependency {
                    path: request_path,
                    logical_path: format!("queries/{}", basename(request_path)?),
                    bytes: &request,
                    role: "external-engine-observations",
                });
                let selector = args[4]
                    .to_str()
                    .ok_or_else(|| Error::invalid("invalid callback selector"))?;
                let doc = if selector == "board-route" {
                    json!({"sourceLayer":city.source_layer,"trace":city_replay::board_route(&city,&request)?})
                } else if selector == "bgm" {
                    json!({"sourceLayer":city.source_layer,"stageTask":"bgm","trace":city_replay::replay_bgm(&request)?})
                } else {
                    let index = selector.parse::<usize>().map_err(|_| {
                        Error::invalid("expected placement index, bgm or board-route")
                    })?;
                    let (p, d) = city.resolve(index)?;
                    json!({"sourceLayer":city.source_layer,"placementIndex":index,"placement":p,"definition":d,"trace":city_replay::replay(p,d,&request)?})
                };
                save_runtime_document(
                    input,
                    &label,
                    &bytes,
                    output,
                    &doc,
                    "city-escape-callback-json",
                    &deps,
                )?;
            }
        }
        "set" => {
            let placements = set::parse_set(&bytes)?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &placements,
                "stage-set-json",
                &[],
            )?;
        }
        "visual" => {
            let model = model::read_model(&bytes)?;
            let scene = visual::evaluate_static_model(&model)?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &scene,
                "native-static-visual-json",
                &[],
            )?;
        }
        "pose" => {
            let model = model::read_model(&bytes)?;
            let motion_bytes = if args.len() == 5 {
                Some(read_source(Path::new(&args[3]))?)
            } else {
                None
            };
            let motion = motion_bytes
                .as_deref()
                .map(motion::read_motion)
                .transpose()?;
            let frame = if args.len() == 5 {
                finite_argument(&args[4])?
            } else {
                0.0
            };
            let evaluated=PoseDocument {
                schema_version:1,
                requested_frame:frame,
                coordinate_space:"original native units; column-vector matrices",
                reference_profile:"SAIO 2.2.2 / SA3D.Modeling 1.2.2+f23c73df68c0c69a96f28746c41b6fc32643340a",
                frame_policy:"negative requests clamp to zero; supported channels clamp to their endpoint keys; no looping or inferred FPS",
                unknowns:["Weighted-mesh skinning, morphs, blending and native action-state behavior are not implemented here","Parity is against the pinned reference library; compiled retail gameplay is not executed or validated"],
                evaluated:pose::evaluate_model_pose(&model,motion.as_ref(),frame)?,
            };
            let dependencies = if let Some(ref data) = motion_bytes {
                let path = Path::new(&args[3]);
                vec![RuntimeDependency {
                    path,
                    logical_path: source_label(path, source_root.as_deref())?,
                    bytes: data,
                    role: "native-motion",
                }]
            } else {
                vec![]
            };
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &evaluated,
                "native-pose-json",
                &dependencies,
            )?;
        }
        "finalhazard-boss-glb" | "finalhazard-weakpoint-glb" => {
            let archive = Path::new(&args[3]);
            let textures = read_source_limited(archive, 32 * 1024 * 1024)?;
            let glb = if command == "finalhazard-boss-glb" {
                let clip = args[4]
                    .to_str()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::invalid("expected boss clip index"))?;
                sa2_asset_tools::finalhazard::boss_glb(
                    &bytes,
                    &textures,
                    clip,
                    finite_argument(&args[5])?,
                )?
            } else {
                sa2_asset_tools::finalhazard::weakpoint_glb(&bytes, &textures)?
            };
            save_runtime_bytes(
                input,
                &label,
                &bytes,
                output,
                &glb,
                command,
                &[RuntimeDependency {
                    path: archive,
                    logical_path: source_label(archive, source_root.as_deref())?,
                    bytes: &textures,
                    role: "model-textures",
                }],
            )?;
        }
        "city-board-glb" => {
            let archive = Path::new(&args[4]);
            let textures = read_source_limited(archive, 1024 * 1024)?;
            let glb = sa2_asset_tools::board_model::board_glb(
                &bytes,
                &textures,
                board_variant(&args[3])?,
            )?;
            save_runtime_bytes(
                input,
                &label,
                &bytes,
                output,
                &glb,
                "native-board-glb",
                &[RuntimeDependency {
                    path: archive,
                    logical_path: source_label(archive, source_root.as_deref())?,
                    bytes: &textures,
                    role: "board-textures",
                }],
            )?;
        }
        "city-board-model" => {
            let board = sa2_asset_tools::board_model::extract(&bytes, board_variant(&args[3])?)?;
            save_runtime_bytes(
                input,
                &label,
                &bytes,
                output,
                &board.sa2mdl,
                "native-board-sa2mdl",
                &[],
            )?;
        }
        "collision" | "city-collision" => {
            let scale = if args.len() == 4 {
                finite_argument(&args[3])?
            } else {
                1.0
            };
            let native = if command.starts_with("city-") {
                city_collision(&bytes)?
            } else {
                stage::read_stage(&bytes)?
            };
            let world = collision::bake_stage(&native, scale)?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &world,
                "native-collision-json",
                &[],
            )?;
        }
        "raycast" | "city-raycast" => {
            let request_path = Path::new(&args[3]);
            let request_bytes = read_source_limited(request_path, 64 * 1024)?;
            let request: RayRequest = serde_json::from_slice(&request_bytes)?;
            let native = if command.starts_with("city-") {
                city_collision(&bytes)?
            } else {
                stage::read_stage(&bytes)?
            };
            let world = collision::bake_stage(&native, request.uniform_scale)?;
            let hit = world.raycast(request.origin, request.direction, request.max_distance)?;
            let document = json!({"request":request,"hit":hit,"bounds":world.bounds,"unknowns":world.unknowns,
                "queryPolicy":"Static two-sided geometric query, not native collision response, movement or gameplay"});
            let dependencies = [RuntimeDependency {
                path: request_path,
                logical_path: format!("queries/{}", basename(request_path)?),
                bytes: &request_bytes,
                role: "ray-request",
            }];
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &document,
                "native-ray-query-json",
                &dependencies,
            )?;
        }
        "inspect" => {
            let value = inspect(&bytes, &basename(input)?)?;
            let data = serde_json::to_vec_pretty(&value)?;
            save_conversion(input,&label,&bytes,output,&data,"native-json",vec!["See decoded unknowns; JSON inspection does not imply complete geometry/shader/gameplay conversion".into()])?;
        }
        "prs" => {
            let data = archive::decompress_prs(&bytes, MAX_DECODED)?;
            save_conversion(
                input,
                &label,
                &bytes,
                output,
                &data,
                "prs-decoded",
                vec!["Decompressed payload semantics are not established by PRS alone".into()],
            )?;
        }
        "extract" => extract(input, &label, &bytes, output)?,
        "texture" => {
            let selecting_mip = args.len() == 5 && args[3] == "--mip";
            let level = if selecting_mip {
                if !bytes.starts_with(b"DDS ") {
                    return Err(Error::invalid("--mip requires DDS input"));
                }
                args[4]
                    .to_str()
                    .and_then(|s| s.parse::<u32>().ok())
                    .ok_or_else(|| Error::invalid("invalid mip level"))?
            } else {
                0
            };
            let palette_bytes = if selecting_mip {
                None
            } else if args.len() == 5 {
                Some(read_source(Path::new(&args[3]))?)
            } else if args.len() == 3 {
                None
            } else {
                return Err(Error::invalid("texture expects INPUT OUTPUT [GVP BANK]"));
            };
            let palette = palette_bytes
                .as_deref()
                .map(texture::read_gvp)
                .transpose()?;
            let bank = if palette.is_some() {
                args[4]
                    .to_str()
                    .and_then(|v| v.parse::<usize>().ok())
                    .ok_or_else(|| Error::invalid("invalid palette bank"))?
            } else {
                0
            };
            let entries = if bytes.starts_with(b"DDS ") {
                0
            } else {
                let header = texture::read_gvr_header(&bytes)?;
                if palette.is_some()
                    && (!matches!(header.data_format, 8 | 9) || header.flags & 8 != 0)
                {
                    return Err(Error::invalid(
                        "external palette supplied but not consumed by this texture",
                    ));
                }
                if header.data_format == 9 {
                    256
                } else {
                    16
                }
            };
            let image = if bytes.starts_with(b"DDS ") {
                if palette.is_some() {
                    return Err(Error::invalid("DDS does not consume a GVP palette"));
                }
                texture::decode_dds_mip(&bytes, level)?
            } else {
                texture::decode_gvr(
                    &bytes,
                    palette
                        .as_ref()
                        .map(|p| p.bank(bank, entries))
                        .transpose()?,
                )?
            };
            let mut png = vec![];
            image.write_png(&mut png)?;
            // Every palette input is fingerprinted in the manifest as a separate source.
            let source = Source::from_bytes(&label, &bytes)?;
            let mut m = Manifest::new(vec![source]);
            let source = &m.sources[0];
            let image_id = if level == 0 {
                "rgba".to_owned()
            } else {
                format!("rgba.mip.{level}")
            };
            m.assets.push(asset(source,image_id,"texture",whole_span(source),Some(output_record(output,"png-rgba8",&png)?),Representation::Decoded,vec![format!("Mip level {level}; pixels are unscaled straight RGBA. External palette bank: {}. No material opacity or eye adaptation.",if palette.is_some(){bank.to_string()}else{"none".into()})]));
            if let Some(p) = palette_bytes {
                let palette_path = Path::new(&args[3]);
                ensure_unchanged(palette_path, &p)?;
                let s =
                    Source::from_bytes(&source_label(palette_path, source_root.as_deref())?, &p)?;
                let palette = palette
                    .as_ref()
                    .ok_or_else(|| Error::invalid("palette dependency missing"))?;
                let color_size = if palette.format == 6 { 4 } else { 2 };
                let offset = 16 + bank * entries * color_size;
                let length = entries * color_size;
                let colors = p
                    .get(offset..offset + length)
                    .ok_or_else(|| Error::invalid("palette dependency range"))?;
                // A palette file and bank together select one rendered variant.
                m.assets[0].id =
                    format!("{}.palette.{}.bank.{bank}", m.assets[0].id, s.logical_path);
                m.assets[0].dependencies.push(Dependency {
                    source_id: s.id.clone(),
                    role: "external-palette".into(),
                    offset: offset as u64,
                    byte_length: length as u64,
                    sha256: manifest::sha256(colors),
                    bank: Some(bank),
                    entry_count: Some(entries),
                });
                m.sources.push(s);
            }
            m.validate()?;
            ensure_unchanged(input, &bytes)?;
            write_pair(output, &png, &m)?;
        }
        "movie-audio" | "movie-video" => {
            let id = args[3]
                .to_str()
                .and_then(|v| v.parse::<i16>().ok())
                .ok_or_else(|| Error::invalid("expected numeric movie event ID"))?;
            let (_, tracks, video) = sa2_asset_tools::story_movie::demux_movie(id, &bytes)?;
            if command == "movie-video" {
                save_named_conversion(input,&label,&bytes,output,&video,"mpeg1-video",format!("movie.{id}.e0"),Representation::Opaque,vec!["Original encoded E0 elementary stream; container timestamps and audio are separate.".into()])?;
            } else {
                let (index, key) = match args[4].to_str() {
                    Some("c0") => (0, "c0"),
                    Some("c1") => (1, "c1"),
                    _ => return Err(Error::invalid("expected c0 or c1")),
                };

                let track = &tracks[index];
                if track.is_empty() {
                    return Err(Error::invalid(
                        "movie does not contain requested audio track",
                    ));
                }
                let wav = audio::decode_adx(track, 8_000_000)?.to_wav()?;
                save_named_conversion(
                    input,
                    &label,
                    &bytes,
                    output,
                    &wav,
                    "wav-pcm16",
                    format!("movie.{id}.{key}"),
                    Representation::Decoded,
                    vec![
                    "Stream ID identifies the original track; language selection is unverified."
                        .into(),
                ],
                )?;
            }
        }
        "event-subtitles" => {
            let name = input
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let encoding = match name.as_bytes() {
                [b'e', b'v', b'm', b'e', b's', b'h' | b'd' | b'l', lang @ b'0'..=b'5'] => {
                    if *lang == b'0' {
                        encoding_rs::SHIFT_JIS
                    } else {
                        encoding_rs::WINDOWS_1252
                    }
                }
                _ => return Err(Error::invalid("expected evmes bank filename")),
            };
            let report =
                audio::event_subtitles(&archive::decompress_prs(&bytes, MAX_DECODED)?, encoding)?;
            if let Some(arg) = args.get(3) {
                let timing = Path::new(arg);
                let stem = timing.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let (digits, lang) = stem
                    .strip_prefix("e")
                    .and_then(|s| s.split_once("_"))
                    .ok_or_else(|| Error::invalid("expected eNNNN_LANG timing file"))?;
                if digits.len() != 4
                    || !digits.bytes().all(|c| c.is_ascii_digit())
                    || lang != &name[6..]
                {
                    return Err(Error::invalid("expected matching numbered event language"));
                }
                let id = digits
                    .parse::<u32>()
                    .map_err(|_| Error::invalid("invalid event ID"))?;
                let lines = report["events"][id.to_string()]
                    .as_array()
                    .ok_or_else(|| Error::invalid("event absent from subtitle bank"))?;
                let timing_bytes = read_source(timing)?;
                let mut cues =
                    audio::read_event_cues(&archive::decompress_prs(&timing_bytes, MAX_DECODED)?)?;
                let rows = cues["subtitles"]
                    .as_array_mut()
                    .ok_or_else(|| Error::invalid("missing subtitle timing rows"))?;
                for row in rows.iter_mut() {
                    let slot = row["slot"]
                        .as_u64()
                        .ok_or_else(|| Error::invalid("missing subtitle slot"))?
                        as usize;
                    row["subtitle"] = json!(lines.get(slot));
                }
                let result = json!({"eventId":id,"encoding":encoding.name(),"textSlotCount":lines.len(),"subtitles":rows,"matching":"same numbered language and physical slot; runtime visibility unverified"});
                return save_runtime_document(
                    input,
                    &label,
                    &bytes,
                    output,
                    &result,
                    "event-subtitle-slots-json",
                    &[RuntimeDependency {
                        path: timing,
                        logical_path: source_label(timing, source_root.as_deref())?,
                        bytes: &timing_bytes,
                        role: "subtitle-timing",
                    }],
                );
            }
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "event-subtitle-bank-json",
                &[],
            )?;
        }
        "event-music" => {
            let mut report =
                audio::read_event_cues(&archive::decompress_prs(&bytes, MAX_DECODED)?)?;
            let mut paths = std::collections::BTreeMap::new();
            for entry in fs::read_dir(Path::new(&args[3]))? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.ends_with(".adx")
                    && entry.file_type()?.is_file()
                    && paths.insert(name, entry.path()).is_some()
                {
                    return Err(Error::invalid("ambiguous case-insensitive ADX track name"));
                }
            }
            let mut tracks = std::collections::BTreeMap::new();
            let mut remaining = MAX_INPUT;
            for cue in report["audio"]
                .as_array_mut()
                .ok_or_else(|| Error::invalid("missing cue array"))?
            {
                let name = cue["musicName"].as_str().unwrap_or("").to_ascii_lowercase();
                if name.is_empty() {
                    continue;
                }
                let Some(path) = paths.get(&name) else {
                    cue["musicResolution"] = json!(if name.ends_with(".adx") {
                        "missing-track"
                    } else {
                        "uninterpreted-token"
                    });
                    continue;
                };
                if !tracks.contains_key(&name) {
                    let data = read_source_limited(path, remaining)?;
                    remaining -= data.len() as u64;
                    let header = audio::read_adx_header(&data)?;
                    let logical = source_label(path, source_root.as_deref())?;
                    let source = Source::from_bytes(&logical, &data)?;
                    tracks.insert(name.clone(),(path,logical,data,json!({"sourceId":source.id,"sha256":source.sha256,"sampleCount":header.sample_count,"sampleRate":header.sample_rate})));
                }
                cue["musicResolution"] = json!("matched-track");
                cue["musicTrack"] = tracks[&name].3.clone();
            }
            let deps = tracks
                .values()
                .map(|(path, logical, data, _)| RuntimeDependency {
                    path,
                    logical_path: logical.clone(),
                    bytes: data,
                    role: "event-music-track",
                })
                .collect::<Vec<_>>();
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "event-music-json",
                &deps,
            )?;
        }
        "event-voices" => {
            let exe_path = Path::new(&args[3]);
            let exe = read_source(exe_path)?;
            let table = pe::cutscene_voice_table(&exe)?;
            let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let id = stem
                .strip_prefix("e")
                .and_then(|s| s.split_once("_"))
                .filter(|(id, _)| id.len() == 4 && id.bytes().all(|c| c.is_ascii_digit()))
                .and_then(|(id, _)| id.parse::<u32>().ok())
                .ok_or_else(|| Error::invalid("expected event filename eNNNN_SUFFIX.prs"))?;
            let mut report =
                audio::read_event_cues(&archive::decompress_prs(&bytes, MAX_DECODED)?)?;
            for cue in report["audio"]
                .as_array_mut()
                .ok_or_else(|| Error::invalid("missing cue array"))?
            {
                let local = cue["afsVoiceIndex"]
                    .as_u64()
                    .ok_or_else(|| Error::invalid("missing voice index"))?
                    as u32;
                if local != 65535 {
                    let key = id * 1000 + local;
                    cue["internalVoiceId"] = json!(key);
                    cue["voiceFileId"] = json!(table.get(&key));
                }
            }
            report["eventId"] = json!(id);
            save_runtime_bytes(
                input,
                &label,
                &bytes,
                output,
                &serde_json::to_vec_pretty(&report)?,
                "event-voice-lookup-json",
                &[RuntimeDependency {
                    path: exe_path,
                    logical_path: source_label(exe_path, source_root.as_deref())?,
                    bytes: &exe,
                    role: "cutscene-voice-table",
                }],
            )?;
        }
        "chao-competition-tables" => {
            // Native Chao Race / Karate tables from the user's own sonic2app.exe.
            let karate = sa2_asset_tools::chao_karate::Tables::read_exe(&bytes)?;
            let race = sa2_asset_tools::chao_race::read_course_rows_exe(&bytes)?;
            let (race_bots, rival_sets) = sa2_asset_tools::chao_race::read_rivals_exe(&bytes)?;
            let report = serde_json::json!({
                "karate": karate,
                "raceCourses": race,
                "raceBots": race_bots,
                "rivalSets": rival_sets,
                "source": {
                    "karateDescriptors": format!("{:#010x}", sa2_asset_tools::chao_karate::EXE_DESCRIPTORS),
                    "karateSkills": format!("{:#010x}", sa2_asset_tools::chao_karate::EXE_SKILLS),
                    "raceCourses": format!("{:#010x}", sa2_asset_tools::chao_race::EXE_COURSE_ROWS),
                    "raceBots": format!("{:#010x}", sa2_asset_tools::chao_race::EXE_RACE_BOTS),
                    "rivalSets": format!("{:#010x}", sa2_asset_tools::chao_race::EXE_RIVAL_SETS),
                },
            });
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "chao-competition-tables-json",
                &[],
            )?;
        }
        "chao-karate-motions" => {
            // ChaoStgKarate .data 0x168A0 from the user's own ChaoStgKarate.prs.
            let rel = if bytes.starts_with(&[0, 0, 0]) {
                bytes.clone()
            } else {
                sa2_asset_tools::archive::decompress_prs(&bytes, 16 << 20)?
            };
            let rows = sa2_asset_tools::chao_karate::read_motion_table(&rel)?;
            let report = serde_json::json!({ "karateMotions": rows, "source": ".data 0x168a0" });
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "chao-karate-motions-json",
                &[],
            )?;
        }
        "chao-collision" => {
            let report = sa2_asset_tools::chao_collision(&bytes)?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "chao-collision-json",
                &[],
            )?;
        }
        "city-player-keys" | "city-player-motions" => {
            let rel = sa2_asset_tools::city_rel::CityRel::read(&bytes)?;
            let keys = command == "city-player-keys";
            let bindings = if keys {
                rel.player_motion_keys()?
            } else {
                rel.player_motion_bindings()?
            };
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &bindings,
                if keys {
                    "city-player-motion-keys-json"
                } else {
                    "city-player-motion-bindings-json"
                },
                &[],
            )?;
        }
        "city-board-inputs" => {
            let report =
                sa2_asset_tools::city_rel::CityRel::read(&bytes)?.board_input_handoffs()?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "city-board-input-handoffs-json",
                &[],
            )?;
        }
        "city-arrow-world" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Query {
                player_world: Option<[f32; 3]>,
                frame_counter: u32,
            }
            let exe_path = Path::new(&args[3]);
            let rel_path = Path::new(&args[4]);
            let query_path = Path::new(&args[5]);
            let exe = read_source(exe_path)?;
            let rel = read_source(rel_path)?;
            let query = read_source_limited(query_path, 65536)?;
            let q: Query = serde_json::from_slice(&query)?;
            let layout = CityLayout::load(&bytes, &exe)?;
            let report = sa2_asset_tools::city_rel::CityRel::read(&rel)?.placed_arrows(
                &layout,
                q.player_world,
                q.frame_counter,
            )?;
            let deps = [
                RuntimeDependency {
                    path: exe_path,
                    logical_path: source_label(exe_path, source_root.as_deref())?,
                    bytes: &exe,
                    role: "object-table",
                },
                RuntimeDependency {
                    path: rel_path,
                    logical_path: source_label(rel_path, source_root.as_deref())?,
                    bytes: &rel,
                    role: "scenery-module",
                },
                RuntimeDependency {
                    path: query_path,
                    logical_path: format!("queries/{}", basename(query_path)?),
                    bytes: &query,
                    role: "world-observation",
                },
            ];
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "city-arrow-world-json",
                &deps,
            )?;
        }
        "city-arrow-display" => {
            let mode = args[3]
                .to_str()
                .and_then(|s| s.parse::<i8>().ok())
                .ok_or_else(|| Error::invalid("expected signed task mode"))?;
            let tick = args[4]
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(|| Error::invalid("expected unsigned frame counter"))?;
            let report =
                sa2_asset_tools::city_rel::CityRel::read(&bytes)?.arrow_display(mode, tick)?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "city-arrow-display-json",
                &[],
            )?;
        }
        "city-scenery-descriptors" => {
            let report = sa2_asset_tools::city_rel::CityRel::read(&bytes)?.scenery_descriptors()?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "city-scenery-descriptors-json",
                &[],
            )?;
        }
        "city-scenery-proximity" => {
            let path = Path::new(&args[3]);
            let input_bytes = read_source_limited(path, 65536)?;
            let positions: Vec<Option<[f32; 3]>> = serde_json::from_slice(&input_bytes)?;
            if positions.is_empty() || positions.len() > 2048 {
                return Err(Error::invalid("expected 1..2048 local observations"));
            }
            let module = sa2_asset_tools::city_rel::CityRel::read(&bytes)?;
            let result = positions
                .into_iter()
                .map(|p| module.scenery_proximity(p))
                .collect::<Result<Vec<_>>>()?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &result,
                "city-scenery-proximity-json",
                &[RuntimeDependency {
                    path,
                    logical_path: format!("queries/{}", basename(path)?),
                    bytes: &input_bytes,
                    role: "local-player-observations",
                }],
            )?;
        }
        "event-effects" => {
            let report = story_effects::read(&archive::decompress_prs(&bytes, MAX_DECODED)?)?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "event-effects-json",
                &[],
            )?;
        }
        "event-lighting" => {
            let report = audio::event_lighting(&archive::decompress_prs(&bytes, MAX_DECODED)?)?;
            save_runtime_document(
                input,
                &label,
                &bytes,
                output,
                &report,
                "event-lighting-json",
                &[],
            )?;
        }
        "event-cues" => {
            let data = archive::decompress_prs(&bytes, MAX_DECODED)?;
            let report = audio::read_event_cues(&data)?;
            save_conversion(input, &label, &bytes, output, &serde_json::to_vec_pretty(&report)?, "event-cues-json", vec!["Big-endian event language timing prefix; additional effect data is not interpreted. Voice indices are raw source values.".into()])?;
        }
        "ahx" => {
            let wav = audio::decode_ahx_wav(&bytes, MAX_DECODED)?;
            save_conversion(
                input,
                &label,
                &bytes,
                output,
                &wav,
                "wav-float32",
                vec!["Fixed mono AHX; trimmed to declared sample count. Float output preserves peaks outside [-1, 1].".into()],
            )?;
        }
        "adx" => {
            let pcm = audio::decode_adx(&bytes, MAX_DECODED / 2)?;
            let wav = pcm.to_wav()?;
            save_conversion(input,&label,&bytes,output,&wav,"wav-pcm16",vec!["One pass at native rate/channels; declared loop points are stored in the WAV sampler chunk. CSB and encrypted ADX remain unsupported.".into()])?;
        }
        "transporter" => {
            let text = std::str::from_utf8(&bytes).map_err(|e| Error::invalid(e.to_string()))?;
            let data = serde_json::to_vec_pretty(&pe::read_transporter_mapping(text)?)?;
            save_conversion(input,&label,&bytes,output,&data,"transporter-mapping-json",vec!["Mapping addresses are virtual/source addresses, not file offsets. Assembly, collision, menu, save/handshake and device transfer behavior remain unimplemented.".into()])?;
        }
        "validate" => {
            let m: Manifest = serde_json::from_slice(&bytes)?;
            m.validate()?;
            write_json(
                output,
                &json!({"structurallyValid":true,"contentVerified":false,"manifestSha256":manifest::sha256(&bytes),"assets":m.assets.len(),"contract":m.contract}),
            )?;
        }
        _ => return Err(Error::invalid("unknown command (see --help)")),
    }
    println!("Wrote local {command} output");
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("sa2-assets: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_outputs_outside_local_and_parent_traversal() {
        for path in ["art.png", "resource/native.gvr", ".local/../native.png"] {
            assert!(local_output(Path::new(path)).is_err());
        }
        assert!(local_output(Path::new(".local/sa2-check/output.png")).is_ok());
    }
}
