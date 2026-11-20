use std::{fs,path::{Path,Component}};
type Result<T> = std::result::Result<T,Box<dyn std::error::Error>>;

/// Asset writes require an explicit .local directory and reject symlink/junction
/// ancestors. Generated source data cannot accidentally enter a normal source path.
pub(crate) fn local_output(path: &Path) -> Result<()> {
    if path.components().any(|p| matches!(p, Component::ParentDir))
        || !path.parent().is_some_and(|dir| dir.components().any(|p| p.as_os_str() == ".local"))
    {
        return Err("Extracted assets must use an explicit .local directory without '..'".into());
    }
    let mut ancestor = path.parent();
    while let Some(parent) = ancestor {
        if parent.as_os_str().is_empty() {
            break;
        }
        if let Ok(metadata) = fs::symlink_metadata(parent) {
            if metadata.file_type().is_symlink() {
                return Err("Asset output has a symlink ancestor".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("Asset output has a reparse-point ancestor".into());
                }
            }
        }
        ancestor = parent.parent();
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}
