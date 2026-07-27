//! Platform-specific boot adaptation, isolated from the main flow.
//!
//! The desktop's `run()` calls [`prepare_boot`] once, before the Tauri builder
//! comes up. Everything platform-specific lives here; on non-Linux targets the
//! call is a compile-time no-op, so `lib.rs` carries no platform knowledge.
//!
//! This is also the home for the small platform helpers the restic resource
//! resolution needs ([`restic_binary_name`], [`host_target_triple`]) - keeping
//! every `cfg!(windows)` / OS-ARCH branch in one place rather than scattered
//! across `lib.rs`.

#[cfg(target_os = "linux")]
mod linux;

/// The platform-appropriate restic executable filename (`restic.exe` on Windows,
/// `restic` elsewhere). Mirrors the name `build.rs` stages; the Tauri resource
/// glob (`resources/restic*`) matches.
pub(crate) fn restic_binary_name() -> &'static str {
    if cfg!(windows) {
        "restic.exe"
    } else {
        "restic"
    }
}

/// Map the running host to the target triple we vendor restic for. `None` for
/// hosts we don't ship a binary for (the storage layer then falls back to PATH).
pub(crate) fn host_target_triple() -> Option<&'static str> {
    use std::env::consts::{ARCH, OS};
    match (OS, ARCH) {
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        _ => None,
    }
}

/// Platform boot prep. Linux: probe EGL once on first launch, persist the
/// verdict, and inject software-rendering env when hardware EGL is unavailable
/// (so WebKit2GTK doesn't segfault on GPU-less / broken-DRI hosts). No-op
/// elsewhere - the `linux` submodule isn't even compiled off Linux.
pub(crate) fn prepare_boot() {
    #[cfg(target_os = "linux")]
    linux::prepare_boot();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restic_binary_name_matches_platform() {
        assert_eq!(
            restic_binary_name(),
            if cfg!(windows) {
                "restic.exe"
            } else {
                "restic"
            }
        );
    }

    #[test]
    fn host_target_triple_resolves_on_supported_hosts() {
        use std::env::consts::{ARCH, OS};
        // Every (OS, ARCH) we expect to build on must map to a triple, and no
        // other combination may claim one.
        let triple = host_target_triple();
        let supported = matches!(
            (OS, ARCH),
            ("windows", "x86_64")
                | ("macos", "x86_64")
                | ("macos", "aarch64")
                | ("linux", "x86_64")
                | ("linux", "aarch64")
        );
        assert_eq!(
            triple.is_some(),
            supported,
            "host ({OS}, {ARCH}) -> {triple:?}, expected Some == {supported}"
        );
    }
}
