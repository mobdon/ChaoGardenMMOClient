use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};
use tiny_game_presentation::{Result, render, write_export};
fn local_new(path: &Path) -> Result<PathBuf> {
    if path.components().any(|part| part == Component::ParentDir) {
        return Err("parent traversal is not allowed".into());
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("tool layout changed")?
        .canonicalize()?;
    let parent = path
        .parent()
        .ok_or("output needs a parent")?
        .canonicalize()?;
    if !parent.starts_with(root.join(".local")) || path.try_exists()? {
        return Err("output must be NEW under this checkout's ignored.local".into());
    }
    Ok(parent.join(path.file_name().ok_or("missing output name")?))
}
fn run() -> Result<()> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.len()==4 && args[0]=="--garden-rom" && args[2]=="--output" {
        if fs::metadata(&args[1])?.len()!=16*1024*1024{return Err("Expected A2NE ROM".into());}
        return tiny_game_presentation::export_garden(&fs::read(&args[1])?,&local_new(Path::new(&args[3]))?);
    }
    if args.len() == 4 && args[0] == "--decode-a3ve-rom" && args[2] == "--write-a3ve" {
        let output = local_new(Path::new(&args[3]))?;
        let decoded = tiny_game_presentation::decode_a3ve(&fs::read(&args[1])?)?;
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)?;
        file.write_all(&decoded)?;
        file.sync_all()?;
        println!(
            "{}",
            serde_json::json!({"result":"pass","payload":output,"bytes":decoded.len(),"sha256":tiny_game_presentation::A3VE_PAYLOAD_SHA256})
        );
        return Ok(());
    }
    if args.len() != 8 && args.len() != 10 {
        return Err("Usage: --a2ne-rom PATH --asoe-payload PATH --a3ve-rom PATH --output NEW_LOCAL_DIRECTORY [--write-a3ve NEW_LOCAL_FILE]".into());
    }
    if args[0] != "--a2ne-rom"
        || args[2] != "--asoe-payload"
        || args[4] != "--a3ve-rom"
        || args[6] != "--output"
        || (args.len() == 10 && args[8] != "--write-a3ve")
    {
        return Err("unexpected argument order".into());
    }
    for index in [1, 3, 5] {
        if fs::metadata(&args[index])?.len() > 32 * 1024 * 1024 {
            return Err("source exceeds bounded32MiB".into());
        }
    }
    let export = render(
        &fs::read(&args[1])?,
        &fs::read(&args[3])?,
        &fs::read(&args[5])?,
    )?;
    let output = local_new(Path::new(&args[7]))?;
    let decoded = if args.len() == 10 {
        Some(local_new(Path::new(&args[9]))?)
    } else {
        None
    };
    let manifest = write_export(&output, &export)?;
    if let Some(path) = decoded {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        file.write_all(&export.decoded_a3ve)?;
        file.sync_all()?;
    }
    println!(
        "{}",
        serde_json::json!({"result":"pass","assets":manifest["assets"].as_array().unwrap().len(),"output":output})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
