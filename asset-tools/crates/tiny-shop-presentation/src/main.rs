use std::{
    env,
    path::{Component, Path, PathBuf},
};
use tiny_shop_presentation::{Result, read_source, render, write_export};

fn arguments() -> Result<(PathBuf, PathBuf)> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 4 || args[0] != "--source" || args[2] != "--output" {
        return Err(
            "Usage: tiny-shop-presentation --source VERIFIED_A2NE_ROM --output NEW_LOCAL_DIRECTORY"
                .into(),
        );
    }
    Ok((args[1].clone().into(), args[3].clone().into()))
}

fn local_output(path: &Path) -> Result<PathBuf> {
    if path.components().any(|part| part == Component::ParentDir) {
        return Err("Output may not contain parent-directory traversal".into());
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("Tool must retain its asset-tools/crates layout")?
        .canonicalize()?;
    let parent = path
        .parent()
        .ok_or("Output needs an existing local parent directory")?
        .canonicalize()?;
    if !parent.starts_with(repository.join(".local")) {
        return Err(
            "Native art output must remain under this checkout's ignored .local directory".into(),
        );
    }
    if path.try_exists()? {
        return Err("Existing outputs are never overwritten; choose a new directory".into());
    }
    Ok(parent.join(
        path.file_name()
            .ok_or("Output needs a new directory name")?,
    ))
}

fn run() -> Result<()> {
    let (source_path, output_path) = arguments()?;
    let source = read_source(&source_path)?;
    let export = render(&source)?;
    let output = local_output(&output_path)?;
    write_export(&output, &export)?;
    println!(
        "{}",
        serde_json::json!({"result":"PASS", "sprites":export.subjects.len(), "fontImages":export.font.images.len(), "nativeLabelsMatched":export.font.matched_native_labels, "output":output})
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
