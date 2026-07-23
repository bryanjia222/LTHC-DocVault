//! On-disk preview cache: the rendered HTML for a document version, persisted
//! under the Tauri cache dir so a reopened preview paints instantly even after
//! a restart. This is the second tier of the frontend's
//! memory-LRU -> disk -> re-render lookup; the in-memory LRU lives in the
//! frontend (`utils/previewCache.ts`).
//!
//! Layout: `<app_cache_dir>/docvault-preview-cache/<canonical_vault_key>/<hex(key)>.html`.
//! The cache is scoped by the canonicalized vault root (two vaults never share
//! previews, and `clear_preview_cache` wipes only the active vault's dir). The
//! frontend cache key contains `|` and `:` (e.g. `doc1|v:v2`) which are illegal
//! path components on Windows, so each key byte is written as two lowercase hex
//! chars - a reversible encoding, so `list_preview_cache` can hand the frontend
//! the original keys alongside the HTML at startup.

use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, State};

use crate::local_state::current_vault_root;
use crate::state::AppState;

/// Subdirectory of the cache dir holding all preview caches.
const CACHE_DIR_NAME: &str = "docvault-preview-cache";

/// The active vault's cache directory (may not exist yet; `write` creates it).
/// `None` when no vault is open or the cache dir is unavailable.
fn vault_cache_dir(app: &AppHandle, vault_key: &str) -> Option<PathBuf> {
    app.path()
        .app_cache_dir()
        .ok()
        .map(|dir| dir.join(CACHE_DIR_NAME).join(vault_key))
}

/// The stable on-disk filename for `key`: every byte of the key's UTF-8
/// encoding as two lowercase hex chars, then `.html`. Reversible (see
/// `cache_key_from_name`); never contains the `|`/`:` the raw key holds.
pub(crate) fn cache_file_name(key: &str) -> String {
    let hex: String = key.bytes().map(|b| format!("{:02x}", b)).collect();
    format!("{hex}.html")
}

/// Inverse of `cache_file_name`: decode a `<hex>.html` filename back to the
/// key. `None` when the stem is not even-length lowercase hex (a stray file in
/// the cache dir, or a corrupt name).
pub(crate) fn cache_key_from_name(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".html")?;
    if stem.len() % 2 != 0 {
        return None;
    }
    let bytes = (0..stem.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&stem[i..i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}

/// Path for `key` under a vault cache dir. Pure helper for unit tests.
pub(crate) fn cache_file_path(cache_dir: &Path, key: &str) -> PathBuf {
    cache_dir.join(cache_file_name(key))
}

/// One cached preview, as returned to the frontend by `list_preview_cache`.
#[derive(serde::Serialize)]
pub struct PreviewCacheEntry {
    pub key: String,
    pub html: String,
}

/// Read a cached preview's HTML. `Ok(None)` is a miss (or no vault open).
#[tauri::command]
pub async fn read_preview_cache(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    let Some(vault_key) = current_vault_root(&state) else {
        return Ok(None);
    };
    let Some(dir) = vault_cache_dir(&app, &vault_key) else {
        return Ok(None);
    };
    let path = cache_file_path(&dir, &key);
    tauri::async_runtime::spawn_blocking(move || match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    })
    .await
    .map_err(|e| format!("preview cache read failed: {e}"))?
}

/// Write a cached preview's HTML (overwrite), creating the dir if needed.
/// No-op (Ok) when no vault is open.
#[tauri::command]
pub async fn write_preview_cache(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    html: String,
) -> Result<(), String> {
    let Some(vault_key) = current_vault_root(&state) else {
        return Ok(());
    };
    let Some(dir) = vault_cache_dir(&app, &vault_key) else {
        return Ok(());
    };
    let path = cache_file_path(&dir, &key);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(&path, html).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("preview cache write failed: {e}"))?
}

/// Clear the active vault's entire preview cache (its vault-keyed subdir only;
/// other vaults' caches are left alone). No-op when no vault is open or the dir
/// does not yet exist.
#[tauri::command]
pub async fn clear_preview_cache(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let Some(vault_key) = current_vault_root(&state) else {
        return Ok(());
    };
    let Some(dir) = vault_cache_dir(&app, &vault_key) else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || match fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    })
    .await
    .map_err(|e| format!("preview cache clear failed: {e}"))?
}

/// List the active vault's cached previews as `{ key, html }`, sorted by
/// modification time ascending (oldest first). The frontend prefetches them in
/// this order into its LRU; because the LRU inserts in recency order, the
/// newest entries land last and become the most-recently-used, so when the LRU
/// exceeds its capacity the stalest caches are the ones evicted. Returns an
/// empty vec when no vault is open or the cache dir does not yet exist.
#[tauri::command]
pub async fn list_preview_cache(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<PreviewCacheEntry>, String> {
    let Some(vault_key) = current_vault_root(&state) else {
        return Ok(Vec::new());
    };
    let Some(dir) = vault_cache_dir(&app, &vault_key) else {
        return Ok(Vec::new());
    };
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<PreviewCacheEntry>, String> {
        let read = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.to_string()),
        };
        // (mtime, key, html) collected then sorted by mtime ascending.
        let mut entries: Vec<(std::time::SystemTime, String, String)> = Vec::new();
        for entry in read {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(key) = cache_key_from_name(file_name) else {
                continue; // not a reversible cache filename - skip
            };
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            // `modified()` is unsupported on some platforms/filesystems; treat
            // those as the epoch so they sort first deterministically.
            let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            let html = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            entries.push((mtime, key, html));
        }
        entries.sort_by_key(|(mtime, _, _)| *mtime);
        Ok(entries
            .into_iter()
            .map(|(_, key, html)| PreviewCacheEntry { key, html })
            .collect())
    })
    .await
    .map_err(|e| format!("preview cache list failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_file_name_is_reversible_and_path_safe() {
        let key = "doc1|v:v2";
        let name = cache_file_name(key);
        assert!(name.ends_with(".html"));
        let stem = name.strip_suffix(".html").expect("ends with .html");
        // Only lowercase hex before the extension - no `|`/`:`/separators.
        assert!(
            stem.chars().all(|c| c.is_ascii_hexdigit()),
            "filename should be hex only: {name}"
        );
        // Reversible: decode the stem back to the original key.
        assert_eq!(cache_key_from_name(&name).as_deref(), Some(key));
    }

    #[test]
    fn cache_file_name_differs_for_different_keys() {
        assert_ne!(cache_file_name("doc1|v:v1"), cache_file_name("doc1|v:v2"));
        assert_ne!(
            cache_file_name("doc1|working"),
            cache_file_name("doc2|working")
        );
    }

    #[test]
    fn cache_key_from_name_rejects_non_cache_files() {
        // Stray non-hex file in the cache dir.
        assert_eq!(cache_key_from_name("not-a-cache-file.html"), None);
        // Odd-length hex.
        assert_eq!(cache_key_from_name("abc.html"), None);
        // Non-hex chars.
        assert_eq!(cache_key_from_name("zz.html"), None);
        // Missing the .html suffix.
        assert_eq!(cache_key_from_name("646f6331"), None);
        // Valid even-length hex round-trips to a key.
        assert_eq!(
            cache_key_from_name("646f6331.html").as_deref(),
            Some("doc1")
        );
    }

    #[test]
    fn cache_file_path_lives_under_the_vault_dir() {
        let dir = Path::new("/cache/docvault-preview-cache/vault-key");
        let p = cache_file_path(dir, "doc1|working");
        assert_eq!(p.parent(), Some(dir));
        assert!(p.file_name().unwrap().to_string_lossy().ends_with(".html"));
    }
}
