use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

use crate::locations::Locations;
use crate::marshal::unix_now;

const ALIASES_ENTRY: &str = "aliases.json";
const MANIFEST_ENTRY: &str = "manifest.json";

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ManifestFileEntry {
    relative_path: String,
    sha256: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ExportManifest {
    app_version: String,
    timestamp: u64,
    files: Vec<ManifestFileEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportAnalysis {
    pub new_files: Vec<String>,
    pub conflicts: Vec<String>,
    pub unchanged: Vec<String>,
    pub total_files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
    pub backed_up_count: usize,
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn sha256_of_file(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    Ok(sha256_hex(&data))
}

fn eve_root(locations: &Locations) -> Result<PathBuf, String> {
    locations
        .eve_root()
        .ok_or_else(|| "EVE settings directory not found".into())
}

// Archive paths always use '/', whatever the platform they were written on.
fn relative_path(path: &Path, root: &Path) -> Result<String, String> {
    let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
    Ok(rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn is_exportable(file_name: &str, in_backups: bool) -> bool {
    if in_backups {
        file_name.ends_with(".bak")
    } else {
        (file_name.starts_with("core_") && file_name.ends_with(".dat")) || file_name == "prefs.ini"
    }
}

fn collect_exportable_files(eve_root: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut files = Vec::new();
    let mut collect_dir = |dir: &Path, in_backups: bool| -> Result<(), String> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_file() && is_exportable(name, in_backups) {
                let rel = relative_path(&path, eve_root)?;
                files.push((path, rel));
            }
        }
        Ok(())
    };

    for server_entry in fs::read_dir(eve_root).map_err(|e| e.to_string())?.flatten() {
        let server_path = server_entry.path();
        if !server_path.is_dir() {
            continue;
        }
        let Ok(profiles) = fs::read_dir(&server_path) else {
            continue;
        };
        for profile_entry in profiles.flatten() {
            let profile_path = profile_entry.path();
            let is_profile = profile_path.is_dir()
                && profile_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("settings_"));
            if !is_profile {
                continue;
            }
            collect_dir(&profile_path, false)?;
            collect_dir(&profile_path.join("backups"), true)?;
        }
    }

    Ok(files)
}

// Returns the number of files written into the archive.
pub fn export_settings(locations: &Locations, dest: &Path) -> Result<usize, String> {
    let eve_root = eve_root(locations)?;
    if !eve_root.exists() {
        return Err("EVE settings directory does not exist".into());
    }

    let mut sources = collect_exportable_files(&eve_root)?;
    let aliases_path = locations.aliases_file();
    if aliases_path.exists() {
        sources.push((aliases_path, ALIASES_ENTRY.to_string()));
    }

    let file = fs::File::create(dest).map_err(|e| format!("Failed to create zip: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut manifest_files = Vec::with_capacity(sources.len());
    for (abs_path, rel_path) in &sources {
        let data = fs::read(abs_path)
            .map_err(|e| format!("Failed to read {}: {}", abs_path.display(), e))?;
        zip.start_file(rel_path, options)
            .map_err(|e| format!("Failed to add to zip: {}", e))?;
        zip.write_all(&data)
            .map_err(|e| format!("Failed to write to zip: {}", e))?;
        manifest_files.push(ManifestFileEntry {
            relative_path: rel_path.clone(),
            sha256: sha256_hex(&data),
        });
    }

    let manifest = ExportManifest {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: unix_now(),
        files: manifest_files,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    zip.start_file(MANIFEST_ENTRY, options)
        .map_err(|e| format!("Failed to add manifest: {}", e))?;
    zip.write_all(manifest_json.as_bytes())
        .map_err(|e| format!("Failed to write manifest: {}", e))?;
    zip.finish()
        .map_err(|e| format!("Failed to finalize zip: {}", e))?;

    Ok(manifest.files.len())
}

fn read_manifest(archive: &mut ZipArchive<fs::File>) -> Result<ExportManifest, String> {
    let mut manifest_file = archive
        .by_name(MANIFEST_ENTRY)
        .map_err(|_| "No manifest.json found in archive - not a valid EVE Wrench export")?;
    let mut content = String::new();
    manifest_file
        .read_to_string(&mut content)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid manifest: {}", e))
}

fn open_archive(path: &Path) -> Result<ZipArchive<fs::File>, String> {
    let file = fs::File::open(path).map_err(|e| format!("Failed to open zip: {}", e))?;
    ZipArchive::new(file).map_err(|e| format!("Invalid zip file: {}", e))
}

// Rejects absolute paths and ".." so an archive can't write outside the EVE
// settings folder.
fn target_for(rel: &str, eve_root: &Path, locations: &Locations) -> Result<PathBuf, String> {
    if rel == ALIASES_ENTRY {
        return Ok(locations.aliases_file());
    }
    let rel_path = Path::new(rel);
    let safe = rel_path
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)));
    if !safe {
        return Err(format!("Unsafe path in archive: {}", rel));
    }
    Ok(eve_root.join(rel_path))
}

pub fn analyze_import(locations: &Locations, import_path: &Path) -> Result<ImportAnalysis, String> {
    let eve_root = eve_root(locations)?;
    let manifest = read_manifest(&mut open_archive(import_path)?)?;

    let mut analysis = ImportAnalysis {
        new_files: Vec::new(),
        conflicts: Vec::new(),
        unchanged: Vec::new(),
        total_files: manifest.files.len(),
    };

    for entry in &manifest.files {
        let local_path = target_for(&entry.relative_path, &eve_root, locations)?;
        let bucket = if !local_path.exists() {
            &mut analysis.new_files
        } else if sha256_of_file(&local_path)? != entry.sha256 {
            &mut analysis.conflicts
        } else {
            &mut analysis.unchanged
        };
        bucket.push(entry.relative_path.clone());
    }

    Ok(analysis)
}

// Writes new files, and conflicting files only when listed in
// `overwrite_paths`; overwritten settings files are backed up first.
pub fn execute_import(
    locations: &Locations,
    import_path: &Path,
    overwrite_paths: &[String],
) -> Result<ImportResult, String> {
    let eve_root = eve_root(locations)?;
    let mut archive = open_archive(import_path)?;
    let manifest = read_manifest(&mut archive)?;

    let overwrite: HashSet<&str> = overwrite_paths.iter().map(String::as_str).collect();
    let timestamp = unix_now();
    let mut result = ImportResult {
        imported_count: 0,
        skipped_count: 0,
        backed_up_count: 0,
    };

    for entry in &manifest.files {
        let rel = entry.relative_path.as_str();
        let Ok(mut zip_file) = archive.by_name(rel) else {
            result.skipped_count += 1;
            continue;
        };
        let mut data = Vec::new();
        zip_file
            .read_to_end(&mut data)
            .map_err(|e| format!("Failed to read {} from archive: {}", rel, e))?;

        let target_path = target_for(rel, &eve_root, locations)?;
        if target_path.exists() {
            if sha256_of_file(&target_path)? == entry.sha256 || !overwrite.contains(rel) {
                result.skipped_count += 1;
                continue;
            }
            if rel != ALIASES_ENTRY
                && let (Some(parent), Some(file_name)) = (
                    target_path.parent(),
                    target_path.file_name().and_then(|n| n.to_str()),
                )
            {
                let backup_dir = parent.join("backups");
                let _ = fs::create_dir_all(&backup_dir);
                let backup_path =
                    backup_dir.join(format!("pre_import_{}_{}", file_name, timestamp));
                if fs::copy(&target_path, &backup_path).is_ok() {
                    result.backed_up_count += 1;
                }
            }
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
        }
        fs::write(&target_path, &data).map_err(|e| format!("Failed to write {}: {}", rel, e))?;
        result.imported_count += 1;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_paths_cannot_escape_the_eve_root() {
        let locations = Locations::new(PathBuf::from("/data"), None);
        let root = Path::new("/eve");
        assert!(target_for("../etc/passwd", root, &locations).is_err());
        assert!(target_for("/etc/passwd", root, &locations).is_err());
        assert_eq!(
            target_for("tq/settings_Default/core_user_1.dat", root, &locations).unwrap(),
            root.join("tq/settings_Default/core_user_1.dat")
        );
        assert_eq!(
            target_for(ALIASES_ENTRY, root, &locations).unwrap(),
            locations.aliases_file()
        );
    }

    #[test]
    fn export_then_import_round_trip() {
        let base = std::env::temp_dir().join("eve-wrench-archive-test");
        let _ = fs::remove_dir_all(&base);
        let eve = base.join("eve");
        let profile = eve.join("c_eve_sharedcache_tq_tranquility/settings_Default");
        fs::create_dir_all(profile.join("backups")).unwrap();
        fs::write(profile.join("core_user_1.dat"), b"one").unwrap();
        fs::write(profile.join("prefs.ini"), b"x=1").unwrap();
        fs::write(profile.join("backups/b_user_1_5.bak"), b"old").unwrap();
        fs::write(profile.join("ignored.txt"), b"no").unwrap();

        let locations = Locations::new(base.join("data"), Some(eve.clone()));
        let zip_path = base.join("export.zip");
        assert_eq!(export_settings(&locations, &zip_path).unwrap(), 3);

        fs::write(profile.join("core_user_1.dat"), b"changed").unwrap();
        fs::remove_file(profile.join("prefs.ini")).unwrap();

        let analysis = analyze_import(&locations, &zip_path).unwrap();
        assert_eq!(analysis.total_files, 3);
        assert_eq!(analysis.conflicts.len(), 1);
        assert_eq!(analysis.new_files.len(), 1);
        assert_eq!(analysis.unchanged.len(), 1);

        let result = execute_import(&locations, &zip_path, &analysis.conflicts).unwrap();
        assert_eq!(result.imported_count, 2);
        assert_eq!(result.backed_up_count, 1);
        assert_eq!(fs::read(profile.join("core_user_1.dat")).unwrap(), b"one");
        assert!(profile.join("prefs.ini").exists());

        fs::remove_dir_all(&base).ok();
    }
}
