//! Linux graphics-compatibility boot prep.
//!
//! WebKit2GTK segfaults on hosts where EGL can't initialize (no GPU / a broken
//! DRI driver): the chain is `egl: failed to create dri2 screen` ->
//! `EGLDisplay Initialization failed: EGL_NOT_INITIALIZED` -> `Cannot create
//! EGL context` -> SIGSEGV. To run out-of-the-box on such hosts without a
//! manual env override, this module probes EGL once on first launch, persists
//! the verdict, and - when hardware EGL is unavailable - injects the
//! software-rendering env (`LIBGL_ALWAYS_SOFTWARE=1` +
//! `WEBKIT_DISABLE_DMABUF_RENDERER=1`) before the Tauri builder / webview come
//! up.
//!
//! The probe runs in a throwaway *subprocess* (we re-exec the current binary
//! with `--probe-egl`), so a segfault during EGL init is contained and can't
//! kill the main process. That mode dlopens `libEGL.so.1` and calls
//! `eglGetDisplay` / `eglInitialize`, exiting `0` on success / non-zero (or
//! signal) on failure.
//!
//! Mode resolution order: explicit `DOCVAULT_SOFTWARE_RENDERING` env (not
//! persisted - an operational knob) -> the persisted verdict -> the first-run
//! probe (whose result is then persisted). The pref lives in a self-contained
//! `graphics.json` under the XDG config dir, resolved without an `AppHandle`
//! (this runs before the Tauri builder exists).
//!
//! All of this is Linux-only and sits behind `platform::prepare_boot()` so the
//! main flow carries no platform knowledge.

use std::path::PathBuf;
use std::process::Command;

/// Subprocess arg that selects the EGL probe mode (re-exec'd by the parent).
const PROBE_ARG: &str = "--probe-egl";
/// Operational override env: `1|true|yes` forces software, `0|false|no`
/// forces hardware. Any other value (or unset) falls through to the persisted
/// verdict / probe. Never persisted.
const ENV_OVERRIDE: &str = "DOCVAULT_SOFTWARE_RENDERING";
const ENV_SOFTWARE: &str = "LIBGL_ALWAYS_SOFTWARE";
const ENV_DMABUF: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
const PREF_FILE: &str = "graphics.json";
/// Matches Tauri's `app_config_dir` on Linux (`${config_dir}/<identifier>`).
const APP_CONFIG_SUBDIR: &str = "com.lthc.docvault";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Auto,
    Software,
    Hardware,
}

impl Mode {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Mode::Auto),
            "software" => Some(Mode::Software),
            "hardware" => Some(Mode::Hardware),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Software => "software",
            Mode::Hardware => "hardware",
        }
    }

    fn is_software(self) -> bool {
        matches!(self, Mode::Software)
    }
}

/// Entry point from `platform::prepare_boot`. Either serves the probe
/// subprocess (and exits) or resolves the render mode and injects env.
pub(super) fn prepare_boot() {
    // Probe subprocess: dlopen libEGL, try to init EGL, exit with the verdict.
    if std::env::args().any(|a| a == PROBE_ARG) {
        std::process::exit(if egl_probe_ok() { 0 } else { 1 });
    }

    if resolve_mode().is_software() {
        set_if_unset(ENV_SOFTWARE, "1");
        set_if_unset(ENV_DMABUF, "1");
    }
}

/// Decide the render mode. Override env -> persisted verdict -> first-run probe
/// (whose result is then persisted).
fn resolve_mode() -> Mode {
    if let Some(mode) = override_mode(std::env::var(ENV_OVERRIDE).ok().as_deref()) {
        return mode;
    }
    if let Some(mode) = read_pref()
        .and_then(|p| Mode::from_str(&p.mode))
        .filter(|m| *m != Mode::Auto)
    {
        return mode;
    }
    let probed = if probe_hardware_egl() {
        Mode::Hardware
    } else {
        Mode::Software
    };
    write_pref(&Pref {
        mode: probed.as_str().to_owned(),
    });
    probed
}

/// Parse the `DOCVAULT_SOFTWARE_RENDERING` override value. Pure so it's
/// unit-testable without touching the real environment.
fn override_mode(value: Option<&str>) -> Option<Mode> {
    match value? {
        "1" | "true" | "yes" => Some(Mode::Software),
        "0" | "false" | "no" => Some(Mode::Hardware),
        _ => None,
    }
}

/// Spawn `current_exe --probe-egl` and return whether hardware EGL initialized.
/// On any failure to locate/spawn self we stay optimistic (return `true`): a
/// spawn glitch is misconfiguration, not evidence of a GPU-less host, and
/// forcing software on every healthy host that can't spawn the probe would be
/// worse than letting the normal path run.
fn probe_hardware_egl() -> bool {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(_) => return true,
    };
    match Command::new(exe).arg(PROBE_ARG).output() {
        Ok(out) => out.status.success(),
        Err(_) => true,
    }
}

/// dlopen `libEGL.so.1`, call `eglGetDisplay(EGL_DEFAULT_DISPLAY)` +
/// `eglInitialize`. `true` when EGL initializes; `false` when libEGL is
/// missing, the display can't be obtained, or init returns `EGL_FALSE`. Runs
/// in the probe subprocess, so a segfault here is contained.
fn egl_probe_ok() -> bool {
    use std::ffi::c_void;
    let lib = match libloading::Library::new("libEGL.so.1") {
        Ok(lib) => lib,
        Err(_) => return false,
    };
    let egl_get_display: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> *mut c_void> =
        match lib.get(b"eglGetDisplay\0") {
            Ok(f) => f,
            Err(_) => return false,
        };
    let egl_initialize: libloading::Symbol<
        unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> u32,
    > = match lib.get(b"eglInitialize\0") {
        Ok(f) => f,
        Err(_) => return false,
    };
    // EGL_DEFAULT_DISPLAY is NULL on Linux.
    let display = unsafe { egl_get_display(std::ptr::null_mut()) };
    if display.is_null() {
        return false;
    }
    let mut major: i32 = 0;
    let mut minor: i32 = 0;
    let ok = unsafe { egl_initialize(display, &mut major, &mut minor) };
    ok != 0
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Pref {
    #[serde(default)]
    mode: String,
}

/// XDG config dir without an `AppHandle` (this runs pre-builder). Mirrors
/// `dirs::config_dir` / Tauri's Linux `app_config_dir` base: `$XDG_CONFIG_HOME`
/// if set and absolute, else `$HOME/.config`.
fn config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() && PathBuf::from(&xdg).is_absolute() {
            return Some(PathBuf::from(xdg));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config"))
}

fn pref_path() -> Option<PathBuf> {
    Some(config_dir()?.join(APP_CONFIG_SUBDIR).join(PREF_FILE))
}

fn read_pref() -> Option<Pref> {
    let path = pref_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_pref(pref: &Pref) {
    let Some(path) = pref_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(pref) {
        let _ = std::fs::write(&path, text);
    }
}

/// Set an env var only when the caller hasn't already, so an explicit user /
/// launcher value is always respected.
fn set_if_unset(key: &str, value: &str) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_round_trips() {
        for m in [Mode::Auto, Mode::Software, Mode::Hardware] {
            assert_eq!(Mode::from_str(m.as_str()), Some(m));
        }
        assert_eq!(Mode::from_str(""), None);
        assert_eq!(Mode::from_str("gpu"), None);
    }

    #[test]
    fn override_parses_truthy_and_falsy() {
        for yes in ["1", "true", "yes"] {
            assert_eq!(override_mode(Some(yes)), Some(Mode::Software), "{yes}");
        }
        for no in ["0", "false", "no"] {
            assert_eq!(override_mode(Some(no)), Some(Mode::Hardware), "{no}");
        }
        assert_eq!(override_mode(Some("maybe")), None);
        assert_eq!(override_mode(None), None);
    }

    #[test]
    fn only_software_is_software() {
        assert!(Mode::Software.is_software());
        assert!(!Mode::Hardware.is_software());
        assert!(!Mode::Auto.is_software());
    }
}
