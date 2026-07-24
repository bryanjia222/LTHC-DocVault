fn main() {
    // Stage before tauri_build::build() - the latter validates that every
    // `bundle.resources` path exists at compile time (the `resources/restic*`
    // glob must match the staged binary).
    stage_bundled_restic();
    tauri_build::build();
}

/// Stage the `third_party` restic binary for the *target* platform into
/// `src-tauri/resources/restic[.exe]` so the Tauri resource bundler ships it
/// next to the app without escaping the crate directory. The `third_party`
/// asset is the source of truth; this copy is generated (gitignored). Skipped
/// when the asset is absent - fetch it with `npm run restic:fetch`. A size
/// check avoids recopying the ~30 MB binary on every build.
///
/// Uses Cargo's `TARGET` triple (not `cfg!(target_os)`) so cross-compilation
/// stages the binary matching the target rather than the host. The executable
/// bit is (re)applied on Unix so the staged copy is runnable.
///
/// Windows-on-ARM (`aarch64-pc-windows-msvc`) has no upstream restic binary to
/// stage, so `staging_triple` falls back to the x86_64 build - Windows 11 on
/// ARM runs x86_64 binaries under emulation, and restic is a CLI subprocess.
fn stage_bundled_restic() {
    let Some(triple) = std::env::var("TARGET").ok().filter(|t| !t.is_empty()) else {
        return;
    };
    let binary = if triple.contains("windows") {
        "restic.exe"
    } else {
        "restic"
    };
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let staged_triple = staging_triple(&triple);
    if staged_triple != triple.as_str() {
        println!(
            "cargo:warning=bundling restic for {triple} from the {staged_triple} binary: restic has no upstream aarch64-windows release, so Windows-on-ARM runs the x86_64 build under emulation.",
        );
    }
    let src = manifest
        .join("../../../third_party/restic/0.19.1")
        .join(staged_triple)
        .join(binary);
    let dst = manifest.join("resources").join(binary);
    println!("cargo:rerun-if-changed={}", src.display());
    if !src.exists() {
        println!(
            "cargo:warning=restic binary for {triple} not found at {} - run `npm run restic:fetch` (it runs automatically on `npm install` and `tauri dev`/`tauri build`). The build fails until the host binary is present.",
            src.display()
        );
        return;
    }
    let src_len = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
    if src_len == 0 {
        return;
    }
    let needs_copy = std::fs::metadata(&dst)
        .map(|m| m.len() != src_len)
        .unwrap_or(true);
    if needs_copy {
        std::fs::create_dir_all(dst.parent().unwrap()).expect("create src-tauri/resources/ dir");
        std::fs::copy(&src, &dst).expect("stage restic binary into src-tauri/resources/");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755))
            .expect("set restic executable bit");
    }
}

/// Map a build `TARGET` triple to the triple whose restic binary we actually
/// stage. restic 0.19.1 publishes no `aarch64-pc-windows-msvc` build, so a
/// Windows-on-ARM build bundles the `x86_64-pc-windows-msvc` restic instead -
/// Windows 11 on ARM runs x86_64 binaries natively under its emulation layer,
/// and restic is a plain CLI subprocess the app spawns. Every other target
/// stages its own matching binary.
fn staging_triple(triple: &str) -> &str {
    match triple {
        "aarch64-pc-windows-msvc" => "x86_64-pc-windows-msvc",
        _ => triple,
    }
}
