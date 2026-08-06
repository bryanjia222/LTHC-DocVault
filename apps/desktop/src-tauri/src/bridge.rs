//! Local HTTP bridge that Office/WPS add-ins POST the active document to, so a
//! user can save straight from the editor into the vault. Listens on
//! `127.0.0.1:8765` (never exposed beyond localhost) and guards every API call
//! with a per-session token, so a random local webpage (DNS-rebinding / CSRF)
//! cannot write into the vault.
//!
//! Uploaded bytes are written to a temp file and pushed through the same
//! two-phase pipeline as the `commit_document` command (`phase_a_commit` + an
//! `Archive` job with the standard `job:update` emitter), so the desktop UI
//! picks up new documents/versions with **no frontend changes** - the existing
//! `subscribeJobs` listener already reloads the document list when an archive
//! job succeeds.
//!
//! The bridge also serves the add-in task-pane UI at `/` (built from
//! `shared/addin-web`; a placeholder page until that lands), injecting the
//! session token into the served page so the task pane is same-origin with the
//! API and needs no CORS handling.

use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use docvault_core::DocVault;
use docvault_jobs::{JobEventCallback, JobKind, JobOutcome, JobRegistry};
use docvault_storage::DocumentRef;
use docvault_types::{CommitMetadata, Version};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tiny_http::{Header, Response, ResponseBox, Server};

use crate::jobs::executors::{execute_archive, make_emitter, phase_a_commit};
use crate::state::{self, AppState};

/// Fixed loopback address so the add-in manifests can point at a stable URL.
/// Loopback-only: never bound to a network interface.
pub const BRIDGE_ADDR: &str = "127.0.0.1:8765";
/// Upper bound on a single uploaded document. Defensive - Office.js already
/// caps at ~20MB, and the WPS add-in (Phase 2) will send a path instead of
/// bytes, so nothing legitimate approaches this.
const MAX_BODY_BYTES: u64 = 100 * 1024 * 1024;
/// How often the accept loop polls the stop flag so exit is prompt.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Per-session token so a random webpage cannot POST into the vault. Derived
/// from startup time + pid + a counter via SHA-256 (the crate already depends
/// on sha2) - unpredictable to a remote page, cheap, and rotated every launch.
fn generate_token() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(nanos.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A request's route, parsed from its method + URL. Pure so the routing table
/// is unit-tested without a live server.
#[derive(Debug, PartialEq, Eq)]
enum Route {
    Health,
    Documents,
    Import {
        file_name: String,
        ext: Option<String>,
        author: Option<String>,
    },
    CommitVersion {
        doc_id: String,
        ext: Option<String>,
        note: Option<String>,
    },
    TaskPane,
    NotFound,
}

/// Read one percent-encoded `key` from a query string (the value is UTF-8
/// decoded; missing key => `None`). Keys are ASCII, so only the value is
/// decoded.
fn query_value(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == key {
            return percent_encoding::percent_decode_str(v)
                .decode_utf8()
                .ok()
                .map(|s| s.into_owned());
        }
    }
    None
}

fn parse_route(method: &str, url: &str) -> Route {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    match path {
        "/api/health" if method == "GET" => Route::Health,
        "/api/documents" if method == "GET" => Route::Documents,
        "/api/documents/import" if method == "POST" => {
            let file_name = match query_value(query, "fileName") {
                Some(name) if !name.trim().is_empty() => name.trim().to_owned(),
                _ => return Route::NotFound,
            };
            Route::Import {
                file_name,
                ext: query_value(query, "ext"),
                author: query_value(query, "author"),
            }
        }
        "/" if method == "GET" => Route::TaskPane,
        _ if method == "POST" => {
            if let Some(doc_id) = path
                .strip_prefix("/api/documents/")
                .and_then(|rest| rest.strip_suffix("/versions"))
            {
                if !doc_id.is_empty() {
                    return Route::CommitVersion {
                        doc_id: doc_id.to_owned(),
                        ext: query_value(query, "ext"),
                        note: query_value(query, "note"),
                    };
                }
            }
            Route::NotFound
        }
        _ => Route::NotFound,
    }
}

/// The session token that guards the API. Present only on the task-pane page
/// (injected by the server), never on any other origin.
fn check_bearer(header: Option<&str>, expected_token: &str) -> bool {
    let Some(value) = header else {
        return false;
    };
    let Some(rest) = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
    else {
        return false;
    };
    rest == expected_token
}

/// Read the whole request body, aborting with [`ReadError::TooLarge`] once it
/// exceeds `limit` so a hostile upload cannot exhaust memory.
#[derive(Debug, PartialEq, Eq)]
enum ReadError {
    TooLarge,
    Io,
}

fn read_limited_body(reader: &mut dyn Read, limit: u64) -> Result<Vec<u8>, ReadError> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).map_err(|_| ReadError::Io)?;
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() as u64 > limit {
            return Err(ReadError::TooLarge);
        }
    }
}

/// Validate a client-supplied document extension (`docx`, `xlsx`, ...): a
/// leading dot is stripped, the rest must be short ASCII alphanumerics. The
/// temp file the upload is written to carries this extension so the OOXML
/// manifest sniffing downstream recognizes the format.
fn normalize_ext(ext: Option<&str>) -> Result<String, String> {
    let raw = ext.ok_or("missing ext query parameter")?;
    let raw = raw.trim().trim_start_matches('.');
    if raw.is_empty() || raw.len() > 16 || !raw.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(format!("invalid document extension: {raw}"));
    }
    Ok(raw.to_ascii_lowercase())
}

/// Everything the bridge accept-loop thread needs to serve requests. All
/// members are `Send + Sync` (mirroring the command handlers), so the thread
/// can call the same commit pipeline as the IPC commands. The `on_event`
/// callback is pre-built (production wires it to the `job:update` emitter; a
/// test injects a no-op), so the commit path needs no `AppHandle`.
struct Bridge {
    on_event: JobEventCallback,
    vault: Arc<Mutex<Option<DocVault>>>,
    jobs: JobRegistry,
    token: String,
    stop: Arc<AtomicBool>,
}

/// Bind the loopback server and spawn the accept loop. Called from `run()`'s
/// setup after the vault is opened; a bind failure (port in use) disables the
/// bridge for the session - the add-in then sees "DocVault not running".
pub fn start(app: AppHandle, state: &AppState) -> Result<(), String> {
    let token = generate_token();
    *state.bridge_token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
    let server = Server::http(BRIDGE_ADDR).map_err(|e| e.to_string())?;
    let bridge = Bridge {
        on_event: make_emitter(app),
        vault: state.vault.clone(),
        jobs: state.jobs.clone(),
        token,
        stop: state.bridge_stop.clone(),
    };
    std::thread::Builder::new()
        .name("docvault-bridge".to_owned())
        .spawn(move || accept_loop(server, bridge))
        .map_err(|e| e.to_string())?;
    tracing::info!(addr = BRIDGE_ADDR, "add-in bridge listening");
    Ok(())
}

fn accept_loop(server: Server, bridge: Bridge) {
    while !bridge.stop.load(Ordering::Relaxed) {
        match server.recv_timeout(POLL_INTERVAL) {
            Ok(Some(mut request)) => {
                let route = parse_route(request.method().as_str(), request.url());
                let response = dispatch(route, &mut request, &bridge);
                if let Err(e) = request.respond(response) {
                    tracing::warn!(error = %e, "bridge respond failed");
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, "bridge recv failed");
                break;
            }
        }
    }
    tracing::info!("add-in bridge stopped");
}

fn dispatch(route: Route, request: &mut tiny_http::Request, bridge: &Bridge) -> ResponseBox {
    let auth = request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("authorization"))
        .map(|h| h.value.as_str());
    match route {
        Route::Health => json_response(
            200,
            &serde_json::json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "vaultOpen": state::lock_vault(&bridge.vault).is_some(),
            }),
        ),
        Route::TaskPane => taskpane_response(&bridge.token),
        Route::Documents => {
            if !check_bearer(auth, &bridge.token) {
                return json_response(401, &serde_json::json!({ "error": "unauthorized" }));
            }
            match list_documents(bridge) {
                Ok(documents) => {
                    json_response(200, &serde_json::json!({ "documents": documents }))
                }
                Err(message) => {
                    json_response(503, &serde_json::json!({ "error": message }))
                }
            }
        }
        Route::Import {
            file_name,
            ext,
            author,
        } => {
            if !check_bearer(auth, &bridge.token) {
                return json_response(401, &serde_json::json!({ "error": "unauthorized" }));
            }
            let ext = match normalize_ext(ext.as_deref()) {
                Ok(ext) => ext,
                Err(message) => return json_response(400, &serde_json::json!({ "error": message })),
            };
            let bytes = match read_body(request) {
                Ok(bytes) => bytes,
                Err(error) => return body_error_response(error),
            };
            match commit_import(bridge, &bytes, &file_name, &ext, author.as_deref()) {
                Ok(payload) => json_response(200, &payload),
                Err(message) => json_response(400, &serde_json::json!({ "error": message })),
            }
        }
        Route::CommitVersion { doc_id, ext, note } => {
            if !check_bearer(auth, &bridge.token) {
                return json_response(401, &serde_json::json!({ "error": "unauthorized" }));
            }
            let ext = match normalize_ext(ext.as_deref()) {
                Ok(ext) => ext,
                Err(message) => return json_response(400, &serde_json::json!({ "error": message })),
            };
            let bytes = match read_body(request) {
                Ok(bytes) => bytes,
                Err(error) => return body_error_response(error),
            };
            match commit_version(bridge, &bytes, &doc_id, &ext, note.as_deref()) {
                Ok(payload) => json_response(200, &payload),
                Err(message) => json_response(400, &serde_json::json!({ "error": message })),
            }
        }
        Route::NotFound => json_response(404, &serde_json::json!({ "error": "not found" })),
    }
}

fn body_error_response(error: ReadError) -> ResponseBox {
    match error {
        ReadError::TooLarge => {
            json_response(413, &serde_json::json!({ "error": "document too large" }))
        }
        ReadError::Io => {
            json_response(400, &serde_json::json!({ "error": "failed to read request body" }))
        }
    }
}

fn read_body(request: &mut tiny_http::Request) -> Result<Vec<u8>, ReadError> {
    read_limited_body(request.as_reader(), MAX_BODY_BYTES)
}

/// Write uploaded bytes to a temp file and run the same synchronous Phase A
/// commit as `commit_document`, then spawn the Archive job that finalizes it.
/// The temp file only needs to live until `phase_a_commit` durably copies the
/// source into the vault intake.
fn commit_import(
    bridge: &Bridge,
    bytes: &[u8],
    file_name: &str,
    ext: &str,
    author: Option<&str>,
) -> Result<serde_json::Value, String> {
    let (source_path, _temp_dir) = write_upload(bytes, ext)?;
    let metadata = CommitMetadata {
        author: author.map(str::to_owned),
        note: None,
    };
    let (document, version) = phase_a_commit(
        &bridge.vault,
        &source_path,
        DocumentRef::NewName(file_name.to_owned()),
        metadata,
    )?;
    let job_id = spawn_archive(bridge, file_name.to_owned(), version);
    Ok(serde_json::json!({
        "documentId": document.id.as_str(),
        "jobId": job_id,
    }))
}

/// Commit a new version of an existing document from uploaded bytes. The target
/// label (for the job bubble) is the document's display name; a missing
/// document fails fast here rather than spawning a doomed job.
fn commit_version(
    bridge: &Bridge,
    bytes: &[u8],
    doc_id: &str,
    ext: &str,
    note: Option<&str>,
) -> Result<serde_json::Value, String> {
    let (source_path, _temp_dir) = write_upload(bytes, ext)?;
    let target_label = {
        let vault = state::lock_vault(&bridge.vault);
        let vault = vault.as_ref().ok_or("vault not initialized")?;
        vault.document_name(doc_id).map_err(|e| e.to_string())?
    };
    let metadata = CommitMetadata {
        author: None,
        note: note.map(str::to_owned),
    };
    let (_document, version) = phase_a_commit(
        &bridge.vault,
        &source_path,
        DocumentRef::IdPrefix(doc_id.to_owned()),
        metadata,
    )?;
    let job_id = spawn_archive(bridge, target_label, version);
    Ok(serde_json::json!({ "jobId": job_id }))
}

fn write_upload(bytes: &[u8], ext: &str) -> Result<(std::path::PathBuf, tempfile::TempDir), String> {
    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source_path = temp_dir.path().join(format!("upload.{ext}"));
    std::fs::write(&source_path, bytes).map_err(|e| e.to_string())?;
    // The caller keeps the `TempDir` alive until `phase_a_commit` has durably
    // copied the source into the vault intake, then it drops and the temp file
    // is cleaned up.
    Ok((source_path, temp_dir))
}

fn spawn_archive(bridge: &Bridge, target_label: String, version: Version) -> String {
    let vault = bridge.vault.clone();
    let version_for_job = version;
    let on_event = bridge.on_event.clone();
    bridge.jobs.spawn(
        JobKind::Archive,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_archive(&vault, &version_for_job, cancel)
        },
    )
}

fn list_documents(bridge: &Bridge) -> Result<Vec<serde_json::Value>, String> {
    let vault = state::lock_vault(&bridge.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    let documents = vault.list_documents().map_err(|e| e.to_string())?;
    Ok(documents
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id.as_str(),
                "name": d.name,
            })
        })
        .collect())
}

/// The task-pane page. Until `shared/addin-web` is built, this is a minimal
/// placeholder that also proves the served page carries the session token
/// (`window.__DOCVAULT_TOKEN__`) that the built task pane will read.
fn taskpane_response(token: &str) -> ResponseBox {
    let html = format!(
        r#"<!doctype html>
<html lang="zh">
<head><meta charset="utf-8"><title>DocVault</title></head>
<body>
<script>window.__DOCVAULT_TOKEN__ = "{token}";</script>
<p>DocVault 桥接已就绪。任务窗格界面将在 addin-web 构建后提供。</p>
</body>
</html>"#
    );
    text_response(200, html.into_bytes(), "text/html; charset=utf-8")
}

fn json_response(status: u16, value: &serde_json::Value) -> ResponseBox {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    text_response(status, body, "application/json")
}

fn text_response(status: u16, body: Vec<u8>, content_type: &str) -> ResponseBox {
    Response::from_data(body)
        .with_header(
            Header::from_bytes(b"Content-Type".as_slice(), content_type.as_bytes())
                .expect("valid content-type header"),
        )
        .with_status_code(status)
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn token_is_unique_hex() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn query_value_decodes_cjk_and_missing() {
        assert_eq!(
            query_value("fileName=%E6%8A%A5%E5%91%8A.docx&ext=docx", "fileName"),
            Some("报告.docx".to_owned())
        );
        assert_eq!(query_value("fileName=a&ext=docx", "author"), None);
        assert_eq!(query_value("", "ext"), None);
    }

    #[test]
    fn routes_health_documents_and_taskpane() {
        assert_eq!(parse_route("GET", "/api/health"), Route::Health);
        assert_eq!(parse_route("GET", "/api/documents"), Route::Documents);
        assert_eq!(parse_route("GET", "/"), Route::TaskPane);
        // Wrong method for a known path is not that route.
        assert_eq!(parse_route("POST", "/api/health"), Route::NotFound);
    }

    #[test]
    fn route_import_parses_query() {
        assert_eq!(
            parse_route("POST", "/api/documents/import?fileName=a.docx&ext=docx&author=Li"),
            Route::Import {
                file_name: "a.docx".to_owned(),
                ext: Some("docx".to_owned()),
                author: Some("Li".to_owned()),
            }
        );
        // Missing file name is rejected (not a valid import route).
        assert_eq!(
            parse_route("POST", "/api/documents/import?ext=docx"),
            Route::NotFound
        );
    }

    #[test]
    fn route_commit_version_parses_id() {
        assert_eq!(
            parse_route("POST", "/api/documents/abc123/versions?ext=xlsx&note=fix"),
            Route::CommitVersion {
                doc_id: "abc123".to_owned(),
                ext: Some("xlsx".to_owned()),
                note: Some("fix".to_owned()),
            }
        );
        assert_eq!(parse_route("POST", "/api/documents//versions"), Route::NotFound);
    }

    #[test]
    fn bearer_check_is_exact_and_case_tolerant() {
        assert!(check_bearer(Some("Bearer secret"), "secret"));
        assert!(check_bearer(Some("bearer secret"), "secret"));
        assert!(!check_bearer(Some("Bearer wrong"), "secret"));
        assert!(!check_bearer(None, "secret"));
        assert!(!check_bearer(Some("secret"), "secret"));
    }

    #[test]
    fn body_limit_enforced() {
        let mut data = Cursor::new(vec![0u8; 10]);
        assert_eq!(read_limited_body(&mut data, 100).unwrap().len(), 10);

        let mut too_big = Cursor::new(vec![0u8; 10]);
        assert_eq!(read_limited_body(&mut too_big, 5), Err(ReadError::TooLarge));
    }

    #[test]
    fn ext_normalization() {
        assert_eq!(normalize_ext(Some(".DOCX")).unwrap(), "docx");
        assert_eq!(normalize_ext(Some("xlsx")).unwrap(), "xlsx");
        assert!(normalize_ext(None).is_err());
        assert!(normalize_ext(Some("")).is_err());
        assert!(normalize_ext(Some("../../evil")).is_err());
    }

    /// End-to-end smoke: a real loopback server + a temp vault; an authorized
    /// upload creates a document that both the bridge's document list and the
    /// vault agree on, and an unauthorized upload is rejected. Uses
    /// `tauri::async_runtime::block_on` (async reqwest is already a dependency;
    /// the `blocking` feature is not enabled).
    #[test]
    fn import_via_http_creates_document_and_lists_it() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let state = AppState::new();
        state::connect_vault_core(&state, root.to_str().unwrap(), "local-copy", None).unwrap();

        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap().to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let bridge = Bridge {
            on_event: Arc::new(|_| {}),
            vault: state.vault.clone(),
            jobs: state.jobs.clone(),
            token: "test-token".to_owned(),
            stop: stop.clone(),
        };
        std::thread::spawn(move || accept_loop(server, bridge));

        let client = reqwest::Client::new();

        // Health is token-free and reflects the open vault.
        let health =
            tauri::async_runtime::block_on(client.get(format!("http://{addr}/api/health")).send())
                .unwrap();
        let health_text = tauri::async_runtime::block_on(health.text()).unwrap();
        let body: serde_json::Value = serde_json::from_str(&health_text).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["vaultOpen"], true);

        // Without the session token the upload is rejected.
        let denied = tauri::async_runtime::block_on(
            client
                .post(format!("http://{addr}/api/documents/import?fileName=a.txt&ext=txt"))
                .body(b"x".to_vec())
                .send(),
        )
        .unwrap();
        assert_eq!(denied.status().as_u16(), 401);

        // An authorized upload commits synchronously (Phase A) and returns the id.
        let imported = tauri::async_runtime::block_on(
            client
                .post(format!("http://{addr}/api/documents/import?fileName=note.txt&ext=txt"))
                .header("Authorization", "Bearer test-token")
                .body(b"hello world".to_vec())
                .send(),
        )
        .unwrap();
        assert_eq!(imported.status().as_u16(), 200);
        let imported_text = tauri::async_runtime::block_on(imported.text()).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&imported_text).unwrap();
        let doc_id = payload["documentId"].as_str().expect("documentId in payload");

        // The bridge's document list and the vault agree on the new doc.
        let listed = tauri::async_runtime::block_on(
            client
                .get(format!("http://{addr}/api/documents"))
                .header("Authorization", "Bearer test-token")
                .send(),
        )
        .unwrap();
        let listed_text = tauri::async_runtime::block_on(listed.text()).unwrap();
        let list: serde_json::Value = serde_json::from_str(&listed_text).unwrap();
        let names: Vec<&str> = list["documents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"note.txt"), "bridge documents: {list}");

        let vault = state::lock_vault(&state.vault);
        let vault = vault.as_ref().unwrap();
        let docs = vault.list_documents().unwrap();
        assert!(
            docs.iter().any(|d| d.id.as_str() == doc_id),
            "vault documents: {docs:?}"
        );

        stop.store(true, Ordering::Relaxed);
    }
}
