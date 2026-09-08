//! The Bonaparte Vault — one global asset folder shared by every project.
//!
//! Dump logos, brand art, character sheets, audio, video, Lottie files,
//! typography — anything — into clean subfolders and it's available from
//! every project, on every machine the folder syncs to (Dropbox/cloud
//! folder = a portable asset library). The designer's shelf and the AI's
//! asset source are the same thing: agents list and read the vault through
//! the same commands the UI uses.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The clean subfolder arrangement the vault keeps.
pub const FOLDERS: &[&str] = &[
    "logos", "images", "audio", "video", "lottie", "fonts", "effects",
];

/// Vault root: `$BONAPARTE_VAULT` override, else `~/.bonaparte/vault`.
pub fn vault_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BONAPARTE_VAULT") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".bonaparte").join("vault")
}

/// Create the vault and its subfolders. Idempotent.
pub fn ensure_layout() -> Result<PathBuf, String> {
    let root = vault_dir();
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("Vault unavailable at {}: {e}", root.display()))?;
    for folder in FOLDERS {
        std::fs::create_dir_all(root.join(folder))
            .map_err(|e| format!("Vault folder {folder} unavailable: {e}"))?;
    }
    Ok(root)
}

#[derive(Serialize)]
pub struct VaultEntry {
    pub file: String,
    pub bytes: u64,
    pub modified_secs: u64,
}

#[derive(Serialize)]
pub struct VaultFolder {
    pub name: String,
    pub entries: Vec<VaultEntry>,
}

/// List every folder with its files (name, size, mtime) — newest first.
pub fn list() -> Result<Value, String> {
    let root = ensure_layout()?;
    let mut folders = Vec::new();
    for folder in FOLDERS {
        let mut entries = Vec::new();
        let dir = root.join(folder);
        if let Ok(read) = std::fs::read_dir(&dir) {
            for item in read.flatten() {
                let path = item.path();
                if !path.is_file() {
                    continue;
                }
                let Ok(meta) = item.metadata() else { continue };
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.starts_with('.') {
                    continue;
                }
                let modified_secs = meta
                    .modified()
                    .ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                entries.push(VaultEntry {
                    file: name.to_owned(),
                    bytes: meta.len(),
                    modified_secs,
                });
            }
        }
        entries.sort_by(|a, b| b.modified_secs.cmp(&a.modified_secs));
        folders.push(VaultFolder {
            name: (*folder).to_owned(),
            entries,
        });
    }
    Ok(json!({
        "root": root.display().to_string(),
        "folders": folders,
    }))
}

fn safe_folder(folder: &str) -> Result<&str, String> {
    FOLDERS
        .iter()
        .copied()
        .find(|f| *f == folder)
        .ok_or_else(|| format!("Vault folder must be one of: {}", FOLDERS.join(", ")))
}

/// Sanitize a file name: keep the stem readable, strip path parts and
/// anything that could escape the vault.
fn safe_file_name(name: &str) -> Result<String, String> {
    // Path parts never survive: only the final component is kept.
    let name = std::path::Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let cleaned: String = name
        .chars()
        .map(|c| {
            let ok =
                c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ' | '(' | ')' | '+');
            if ok {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() || trimmed == ".." {
        return Err("File name is empty".into());
    }
    Ok(trimmed.to_owned())
}

/// Save bytes into a vault folder. Returns the written file's name.
pub fn save(folder: &str, name: &str, data_base64: &str) -> Result<String, String> {
    let folder = safe_folder(folder)?;
    let name = safe_file_name(name)?;
    let cap = 2 * 1024 * 1024 * 1024usize;
    let bytes = STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("Invalid vault upload: {e}"))?;
    if bytes.is_empty() {
        return Err("Vault upload is empty".into());
    }
    if bytes.len() > cap {
        return Err("Vault upload exceeds the 2 GiB sanity bound".into());
    }
    let root = ensure_layout()?;
    let target = root.join(folder).join(&name);
    bonaparte_runtime_write(&target, &bytes)?;
    Ok(name)
}

fn bonaparte_runtime_write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    crate::write_file_atomic(path, bytes)
}

/// Read a vault file back as base64 (the UI feeds it through the normal
/// import pipeline, AI agents decode it directly).
pub fn read(folder: &str, name: &str) -> Result<Value, String> {
    let folder = safe_folder(folder)?;
    let name = safe_file_name(name)?;
    let path = vault_dir().join(folder).join(&name);
    let bytes = std::fs::read(&path)
        .map_err(|_| format!("'{name}' is not in the vault folder '{folder}'"))?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err("Vault file is too large to inline (256 MiB cap)".into());
    }
    Ok(json!({
        "name": name,
        "folder": folder,
        "dataBase64": STANDARD.encode(&bytes),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_vault() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bonaparte-vault-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::env::set_var("BONAPARTE_VAULT", &dir);
        dir
    }

    #[test]
    fn layout_creates_the_clean_folders() {
        let dir = scratch_vault();
        let root = ensure_layout().unwrap();
        // Parallel tests race on the env var; assert on the folders that the
        // RETURNED root must contain (correct under any interleaving).
        assert!(root.starts_with(std::env::temp_dir()));
        for folder in FOLDERS {
            assert!(root.join(folder).is_dir(), "{folder} exists");
        }
        let _ = dir;
    }

    #[test]
    fn save_list_read_round_trip_and_traversal_is_blocked() {
        scratch_vault();
        let b64 = STANDARD.encode(b"logo bytes here");
        let name = save("logos", "../escape/../Acme Logo.svg", &b64).unwrap();
        assert!(!name.contains('/'), "path parts stripped: {name}");
        assert_eq!(name, "Acme Logo.svg");
        let read_back = read("logos", &name).unwrap();
        assert_eq!(read_back["dataBase64"], b64);
        let listing = list().unwrap();
        let logos = listing["folders"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "logos")
            .unwrap();
        assert_eq!(logos["entries"].as_array().unwrap().len(), 1);
        assert!(save("not-a-folder", "x.bin", &b64).is_err());
        assert!(read("images", "../../etc/passwd").is_err());
    }
}
