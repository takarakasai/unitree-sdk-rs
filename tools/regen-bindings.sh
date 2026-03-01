#!/usr/bin/env bash
# Regenerate the committed Cyclone DDS FFI bindings (crates/cyclonedds-sys/
# bindings/<arch>.rs).
#
# MAINTAINER-ONLY. Normal builds never need this — they use the committed
# bindings and require only a C compiler (gcc/g++), no clang/libclang.
# Re-run this only after bumping the vendored Cyclone DDS headers.
#
# Requires libclang (e.g. `apt install libclang-dev`). The bindings are
# architecture-specific, so run this once per target arch (the file is named
# after the host arch, e.g. aarch64.rs / x86_64.rs).
#
#   tools/regen-bindings.sh
#
# It builds cyclonedds-sys with the `buildtime-bindgen` feature, which runs
# bindgen and refreshes bindings/<arch>.rs in place.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "regenerating bindings with bindgen (needs libclang) ..."
cargo build -p cyclonedds-sys --features buildtime-bindgen

ARCH="$(uname -m)"
echo "done. Review and commit crates/cyclonedds-sys/bindings/${ARCH}.rs"
git -C "$ROOT" status --short crates/cyclonedds-sys/bindings/ || true
