use std::fs;
use std::path::{Path, PathBuf};

use crate::marshal::unix_now;
use crate::model::{BackupEntry, SettingsKind};
use crate::scan::{parse_backup_file, parse_core_filename};

fn kind_str(kind: SettingsKind) -> &'static str {
    match kind {
        SettingsKind::User => "user",
        SettingsKind::Char => "char",
    }
}

// Copies a settings file into its profile's backups folder using the shared
// "{name}_{kind}_{id}_{timestamp}.bak" naming scheme, so every backup shows
// up in the app's backup list.
pub(crate) fn auto_backup(path: &Path, name: &str) -> Result<PathBuf, String> {
    let (kind, id) = parse_core_filename(path)?;
    let backup_dir = path
        .parent()
        .ok_or("Could not determine profile directory")?
        .join("backups");
    fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;

    let dest = backup_dir.join(format!(
        "{}_{}_{}_{}.bak",
        name,
        kind_str(kind),
        id,
        unix_now()
    ));
    fs::copy(path, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

pub fn create_backup(source: &Path, name: &str) -> Result<BackupEntry, String> {
    if !source.exists() {
        return Err("Source file does not exist".into());
    }
    let dest = auto_backup(source, name)?;
    parse_backup_file(&dest).ok_or_else(|| "Backup name could not be parsed".into())
}

// Missing files are skipped rather than failing the whole batch.
pub fn delete_backups(paths: &[String]) -> usize {
    paths
        .iter()
        .map(Path::new)
        .filter(|p| p.exists() && fs::remove_file(p).is_ok())
        .count()
}

// Whole-file copy, used for restoring and applying backups.
pub fn copy_settings(source: &Path, targets: &[String]) -> Result<usize, String> {
    if !source.exists() {
        return Err("Source file not found".into());
    }

    let now = filetime::FileTime::now();
    Ok(targets
        .iter()
        .map(Path::new)
        .filter(|dest| *dest != source)
        .filter(|dest| {
            let copied = fs::copy(source, dest).is_ok();
            if copied {
                let _ = filetime::set_file_mtime(dest, now);
            }
            copied
        })
        .count())
}
