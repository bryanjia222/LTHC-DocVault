//! On-disk preview cache: the rendered HTML for a document version, persisted
//! under the Tauri cache dir so a reopened preview paints instantly even after
//! a restart. This is the second tier of the frontend's
//! memory-LRU -> disk -> re-render lookup; the in-memory LRU lives in the
//! frontend (`utils/previewCache.ts`).
//!
//! Layout: `<app_cache_dir>/docvault-preview-cache/<canonical_vault_key>/<sha256(key)>.html`.
//! The cache is scoped by the canonicalized vault root (two vaults never share
//! previews, and `clear_preview_cache` wipes only the active vault's dir). The
//! frontend cache key contains `|` and `:` (e.g. `doc1|v:v2`) which are illegal
//! path components on Windows, so it is sha256-hashed into a hex filename.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use crate::local_state::current_vault_root;
use crate::state::AppState;

/// Subdirectory of the cache dir holding all preview caches.
const CACHE_DIR_NAME: &str = "docvault-preview-cache";
/// How many hex chars of `sha256(key)` form the filename. 32 chars is far past
/// any collision concern and keeps names short.
const HASH_LEN: usize = 32;

/// The active vault's cache directory (may not exist yet; `write` creates it).
/// `None` when no vault is open or the cache dir is unavailable.
fn vault_cache_dir(app: &AppHandle, vault_key: &str) -> Option<PathBuf> {
    app.path()
        .app_cache_dir()
        .ok()
        .map(|dir| dir.join(CACHE_DIR_NAME).join(vault_key))
}

/// The stable on-disk filename for `key`: a truncated sha256 hex digest + `.html`.
/// The raw key is never used as a path component (it holds `|`/`:`).
pub(crate) fn cache_file_name(key: &str) -> String {
    let hash = Sha256::digest(key.as_bytes());
    let hex = hash.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    format!("{}.html", &hex[..HASH_LEN])
}

/// Path for `key` under a vault cache dir. Pure helper for unit tests.
pub(crate) fn cache_file_path(cache_dir: &Path, key: &str) -> PathBuf {
    cache_dir.join(cache_file_name(key))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn cache_file_name_is_stable_and_path_safe() {
        let a = cache_file_name("doc1|v:v2");
        let b = cache_file_name("doc1|v:v2");
        assert_eq!(a, b);
        assert!(a.ends_with(".html"));
        // Only lowercase hex before the extension - no `|`/`:`/separators.
        let hex = a.strip_suffix(".html").expect("ends with .html");
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "filename should be hex only: {a}"
        );
        assert_eq!(hex.len(), HASH_LEN);
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
    fn cache_file_path_lives_under_the_vault_dir() {
        let dir = Path::new("/cache/docvault-preview-cache/vault-key");
        let p = cache_file_path(dir, "doc1|working");
        assert_eq!(p.parent(), Some(dir));
        assert!(p.file_name().unwrap().to_string_lossy().ends_with(".html"));
    }
}
