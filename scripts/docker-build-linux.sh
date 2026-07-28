#!/usr/bin/env bash
# Build Linux desktop packages in a Docker container that mirrors the CI
# linux cells (ubuntu:22.04 + webkit2gtk-4.1 + Rust stable). Native build:
# the container arch matches the target arch, so no cross-compile toolchain.
#
# Usage (run from anywhere; resolves repo root from this script's location):
#   scripts/docker-build-linux.sh            # x86_64 (default)
#   scripts/docker-build-linux.sh --arm64    # aarch64 via qemu on an x64 host
#   scripts/docker-build-linux.sh --rebuild   # --no-cache full image rebuild
#   scripts/docker-build-linux.sh --clean     # rm -rf the target dir first
#   scripts/docker-build-linux.sh --mirror    # use China mirrors (rsproxy.cn + npmmirror)
#   scripts/docker-build-linux.sh -- <tauri-args>   # e.g. -- --bundles deb
#
# Prereqs: Docker. For --arm64 on an x64 host also needs binfmt/qemu:
#   docker run --privileged --rm tonistiigi/binfmt --install arm64
# and the buildx CLI plugin (ships with Docker Desktop; on Linux install
# `docker-buildx-plugin`). The script auto-detects buildx and falls back to
# `docker build` for the x64 path.
#
# Artifacts land in apps/desktop/src-tauri/target/<triple>/release/bundle/.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

ARCH="x86_64"
TARGET="x86_64-unknown-linux-gnu"
PLATFORM=""
REBUILD=0
CLEAN=0
MIRROR=0
TAURI_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arm64)  ARCH="aarch64"; TARGET="aarch64-unknown-linux-gnu"; PLATFORM="linux/arm64" ;;
    --x64)    ARCH="x86_64";  TARGET="x86_64-unknown-linux-gnu";  PLATFORM="" ;;
    --rebuild) REBUILD=1 ;;
    --clean)  CLEAN=1 ;;
    --mirror) MIRROR=1 ;;
    --no-mirror) MIRROR=0 ;;
    --)       shift; while [[ $# -gt 0 ]]; do TAURI_ARGS+=("$1"); shift; done ;;
    -h|--help)
      sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "unknown arg: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

MIRROR_SUFFIX=""
MIRROR_ARG=()
if [[ $MIRROR -eq 1 ]]; then
  MIRROR_SUFFIX="-cn"
  MIRROR_ARG=(--build-arg USE_CN_MIRROR=1)
fi
IMAGE="docvault-linux-builder:${ARCH}${MIRROR_SUFFIX}"
DOCKERFILE="apps/desktop/Dockerfile.linux-build"
BUNDLE_DIR="apps/desktop/src-tauri/target/${TARGET}/release/bundle"

# --- docker sanity ---
if ! docker info >/dev/null 2>&1; then
  echo "error: cannot talk to the docker daemon." >&2
  echo "  - is the daemon running? (systemctl status docker)" >&2
  echo "  - is your user in the 'docker' group? (sudo usermod -aG docker \$USER && re-login)" >&2
  exit 1
fi

# --- pick buildx vs plain docker build ---
USE_BUILDX=0
if [[ -n "$PLATFORM" ]]; then
  if docker buildx version >/dev/null 2>&1; then
    USE_BUILDX=1
  else
    echo "error: --arm64 needs the docker buildx plugin (docker-buildx-plugin)." >&2
    exit 1
  fi
fi

# --- optionally wipe prior target output ---
if [[ $CLEAN -eq 1 ]]; then
  echo ">> rm -rf apps/desktop/src-tauri/target"
  rm -rf "apps/desktop/src-tauri/target"
fi

# --- build the image ---
# Always invoke `docker build`: Docker's layer cache makes it a near no-op
# (~2s) when nothing changed, and - unlike an `image inspect` skip - this
# automatically picks up Dockerfile edits without needing --rebuild. --rebuild
# forwards --no-cache for a true full rebuild.
NO_CACHE=()
[[ $REBUILD -eq 1 ]] && NO_CACHE=(--no-cache)
echo ">> building image $IMAGE"
if [[ $USE_BUILDX -eq 1 ]]; then
  # --load imports the single-platform image into the local docker store so
  # the subsequent `docker run` can use it.
  docker buildx build --platform "$PLATFORM" --load "${NO_CACHE[@]}" "${MIRROR_ARG[@]}" \
    -f "$DOCKERFILE" -t "$IMAGE" .
else
  docker build "${NO_CACHE[@]}" "${MIRROR_ARG[@]}" -f "$DOCKERFILE" -t "$IMAGE" .
fi

# --- build the app (frontend + Rust + bundle) ---
# HOST_UID/GID: the container runs as root (so it can write to /root/.cargo
# and the rustup toolchain under /root), but bind-mount build output
# (node_modules, dist, target) lands on the host owned by root. The EXIT trap
# chowns it back to the invoking user so `rm -rf target` works without sudo.
echo ">> building $TARGET packages"
RUN_PLATFORM=()
[[ -n "$PLATFORM" ]] && RUN_PLATFORM=(--platform "$PLATFORM")

docker run --rm "${RUN_PLATFORM[@]}" \
  -v "$REPO_ROOT:/work" -w /work \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  "$IMAGE" \
  bash -lc "set -euo pipefail
trap 'chown -R \"\$HOST_UID:\$HOST_GID\" /work/apps/desktop/node_modules /work/apps/desktop/dist /work/apps/desktop/src-tauri/target 2>/dev/null || true' EXIT
cd apps/desktop
npm ci
npx --no-install tauri build --target $TARGET ${TAURI_ARGS[*]:-}"

# --- locate artifacts ---
echo ">> artifacts under $BUNDLE_DIR:"
find "$BUNDLE_DIR" -maxdepth 2 -type f \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' \) 2>/dev/null \
  | sed 's/^/  /' || true
