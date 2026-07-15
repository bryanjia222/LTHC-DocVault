fn main() {
    // Stage before tauri_build::build() - the latter validates that every
    // `bundle.resources` path exists at compile time.
    stage_bundled_restic();
    tauri_build::build();
}

/// Stage the `third_party` restic binary into `src-tauri/resources/restic.exe`
/// so the Tauri resource bundler ships it next to the app without escaping the
/// crate directory. The `third_party` asset is the source of truth; this copy is
/// generated (gitignored). Skipped off-Windows / when the asset is absent. A
/// size check avoids recopying the ~30 MB binary on every build.
fn stage_bundled_restic() {
    #[cfg(target_os = "windows")]
    {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let src = manifest
            .join("../../../third_party/restic/0.19.1/x86_64-pc-windows-msvc/restic.exe");
        let dst = manifest.join("resources/restic.exe");
        println!("cargo:rerun-if-changed={}", src.display());
        if !src.exists() {
            return;
        }
        let src_len = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
        let dst_len = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
        if src_len == 0 || src_len == dst_len {
            return;
        }
        std::fs::create_dir_all(dst.parent().unwrap())
            .expect("create src-tauri/resources/ dir");
        std::fs::copy(&src, &dst).expect("stage restic.exe into src-tauri/resources/");
    }
}
