#!/usr/bin/env bash
# Regenerate the committed Cyclone DDS topic descriptors.
#
# MAINTAINER-ONLY. Normal builds never need this — the generated
# crates/unitree-dds/csrc/*.{c,h} are committed and compiled with cc.
# Re-run this only after changing the .msg snapshots in crates/unitree-msgs/msgs/.
#
# Requires `idlc` (Cyclone DDS IDL compiler) matching the vendored libddsc
# version (currently 0.10.2). Build it from source if you don't have it:
#
#   git clone --depth 1 --branch 0.10.2 \
#       https://github.com/eclipse-cyclonedds/cyclonedds.git
#   cmake -S cyclonedds -B cyclonedds/build -DCMAKE_BUILD_TYPE=Release \
#       -DBUILD_IDLC=ON -DBUILD_TESTING=OFF
#   cmake --build cyclonedds/build
#   # idlc -> cyclonedds/build/bin/idlc, libs -> cyclonedds/build/lib
#
# Then point IDLC / IDLC_LIB at them and run this script:
#
#   IDLC=.../cyclonedds/build/bin/idlc \
#   IDLC_LIB=.../cyclonedds/build/lib \
#   tools/regen-descriptors.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IDLC="${IDLC:-idlc}"
CSRC="$ROOT/crates/unitree-dds/csrc"

if [[ -n "${IDLC_LIB:-}" ]]; then
  export LD_LIBRARY_PATH="$IDLC_LIB:${LD_LIBRARY_PATH:-}"
fi

# Generate the .idl from the .msg by building unitree-msgs (its build.rs writes
# per-package .idl into OUT_DIR), then locate them.
echo "building unitree-msgs to (re)generate .idl ..."
cargo build -p unitree-msgs >/dev/null
IDL_DIR="$(find "$ROOT/target" -path '*unitree-msgs*/out/idl' -type d | head -1)"
if [[ -z "$IDL_DIR" ]]; then
  echo "error: could not locate generated .idl under target/" >&2
  exit 1
fi

mkdir -p "$CSRC"
rm -f "$CSRC"/*.idl "$CSRC"/*.c "$CSRC"/*.h
cp "$IDL_DIR"/*.idl "$CSRC"/

echo "running idlc ($("$IDLC" -v 2>/dev/null || echo '?')) ..."
cd "$CSRC"
for idl in *.idl; do
  "$IDLC" -l c "$idl"
  echo "  $idl -> ${idl%.idl}.c"
done

echo "done. Review and commit crates/unitree-dds/csrc/*.{idl,c,h}"
