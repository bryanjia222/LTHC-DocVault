//! Linux boot-time graphics adaptation.
//!
//! WebKit2GTK's DMA-BUF renderer is the modern zero-copy path for sharing the
//! rendered buffer between the web and UI processes. It is also the brittle
//! one: on many GPU/driver combinations its GBM platform display fails to
//! initialize (`egl: failed to create dri2 screen` -> `EGLDisplay
//! Initialization failed: EGL_NOT_INITIALIZED` -> `Cannot create EGL context`
//! -> SIGSEGV), even though the legacy EGL path works fine on the same host.
//! The two paths are distinct, so probing the legacy path can't tell us
//! whether the DMA-BUF path will succeed -- a probe is liable to return a
//! false "hardware" verdict and leave the crash unguarded.
//!
//! So instead of probing we disable the DMA-BUF renderer unconditionally and
//! let WebKit fall back to the broadly-compatible legacy EGL path. The cost is
//! one GPU->CPU->GPU readback per composited frame, imperceptible for the
//! static document UI this app shows; GL itself stays hardware-accelerated.
//!
//! For the rare host where even the legacy EGL path can't come up with a
//! hardware DRI driver (headless server, container, broken driver), the
//! `DOCVAULT_SOFTWARE_RENDERING` env var forces Mesa's software rasterizer so
//! EGL initializes via swrast instead.
//!
//! All of this is Linux-only and sits behind `platform::prepare_boot()` so the
//! non-Linux build sees nothing of it.

/// Env var that forces software GL (Mesa swrast). Accepts `1`/`true`/`yes`
/// (case-insensitive); anything else (including unset) means "don't force".
/// Never persisted -- it is a per-launch escape hatch.
const ENV_OVERRIDE: &str = "DOCVAULT_SOFTWARE_RENDERING";

/// Forces Mesa to use the software rasterizer (llvmpipe/softpipe).
const ENV_SOFTWARE: &str = "LIBGL_ALWAYS_SOFTWARE";

/// Disables WebKit2GTK's DMA-BUF renderer so it falls back to the legacy EGL
/// path. Set unconditionally (see module docs).
const ENV_DMABUF: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

/// Entry point from `platform::prepare_boot`. Runs before the Tauri builder so
/// the env vars are in place before the webview initializes.
pub(super) fn prepare_boot() {
    // The DMA-BUF renderer is the brittle path on many Linux GPU/driver
    // setups. Disable it universally; WebKit falls back to the legacy EGL
    // path, which is far more broadly compatible.
    set_if_unset(ENV_DMABUF, "1");

    // Manual escape hatch for hosts where even legacy EGL can't init with a
    // hardware DRI driver (headless / no-DRI / broken driver): force Mesa
    // swrast so EGL comes up via the software path.
    if is_software_forced() {
        set_if_unset(ENV_SOFTWARE, "1");
    }

    // Boot diagnostic to stderr. We deliberately use `eprintln!` rather than
    // `tracing`: the subscriber isn't installed yet (it comes up in `setup`),
    // and stderr is unbuffered so the line survives the early SIGSEGV we are
    // diagnosing -- a non-blocking file appender could lose its buffer on the
    // crash. Run from a terminal and read the line:
    //   - line absent          -> this binary predates the fix (stale build);
    //                             rebuild from current main.
    //   - =1                   -> the var IS set in-process; if it still
    //                             crashes, dmabuf-disable alone is insufficient
    //                             on this host (would need COMPOSITING_MODE).
    //   - =<other, e.g. 0/''>  -> the session pre-set the var and set_if_unset
    //                             respected it -> switch to unconditional.
    eprintln!(
        "[docvault:boot] {}={} {}={}",
        ENV_DMABUF,
        std::env::var(ENV_DMABUF).unwrap_or_else(|_| "<unset>".into()),
        ENV_SOFTWARE,
        std::env::var(ENV_SOFTWARE).unwrap_or_else(|_| "<unset>".into()),
    );
}

/// Whether the user asked (via `DOCVAULT_SOFTWARE_RENDERING`) to force software
/// GL this launch.
fn is_software_forced() -> bool {
    parse_override(&std::env::var(ENV_OVERRIDE).unwrap_or_default())
}

/// Parse the `DOCVAULT_SOFTWARE_RENDERING` override value. Pure so it is
/// unit-testable without touching the process environment.
fn parse_override(value: &str) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => true,
        _ => false,
    }
}

/// Set `key=val` only if `key` isn't already set, so an explicit value from the
/// user's environment always wins over our default.
fn set_if_unset(key: &str, val: &str) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_accepts_affirmative_values() {
        assert!(parse_override("1"));
        assert!(parse_override("true"));
        assert!(parse_override("yes"));
        // Case-insensitive ...
        assert!(parse_override("TRUE"));
        assert!(parse_override("Yes"));
        // ... and tolerates surrounding whitespace.
        assert!(parse_override("  1  "));
    }

    #[test]
    fn override_rejects_other_values() {
        assert!(!parse_override(""));
        assert!(!parse_override("0"));
        assert!(!parse_override("false"));
        assert!(!parse_override("no"));
        assert!(!parse_override("hardware"));
        assert!(!parse_override("auto"));
    }

    #[test]
    fn set_if_unset_sets_when_absent() {
        let key = "DOCVAULT_TEST_SET_IF_UNSET_ABSENT";
        std::env::remove_var(key);
        set_if_unset(key, "1");
        assert_eq!(std::env::var(key).unwrap(), "1");
        std::env::remove_var(key);
    }

    #[test]
    fn set_if_unset_respects_existing() {
        let key = "DOCVAULT_TEST_SET_IF_UNSET_PRESENT";
        std::env::set_var(key, "0");
        set_if_unset(key, "1");
        assert_eq!(std::env::var(key).unwrap(), "0");
        std::env::remove_var(key);
    }
}
