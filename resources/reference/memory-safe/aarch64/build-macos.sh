#!/usr/bin/env bash
set -euo pipefail

# Builds fat (universal) Mach-O executables combining two arm64e variants:
# - arm64e:    baseline pointer authentication (PAC) ABI.
# - arm64e.x1: PAC version 2 (CPA2) + Memory Tagging Extension (MTE) ABI.
# Both are recognized by clang/lipo/otool as distinct CPU_SUBTYPE_ARM64E capability variants.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SDK="$(xcrun --sdk macosx --show-sdk-path)"
# Match the deployment target to the host SDK to avoid ld's version-mismatch warning.
MIN_VERSION="$(xcrun --sdk macosx --show-sdk-version)"

OUT_DIR="$SCRIPT_DIR/macos"

CFLAGS_COMMON=(-isysroot "$SDK" -mmacosx-version-min="$MIN_VERSION" -std=c23 -O0 -fno-unwind-tables -fno-asynchronous-unwind-tables)
# arm64e.x1 only: instrument stack/heap accesses with MTE tag-check instructions (irg/stg/...).
CFLAGS_ARM64E_X1=(-fsanitize=memtag-stack,memtag-heap -march=armv9a+memtag)

ARCHS=(arm64e arm64e.x1)
SOURCES=(simple pac out-of-bounds use-after-free double-free wild-pointer)

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.o "$OUT_DIR"/*.macho || true

for name in "${SOURCES[@]}"; do
    slices=()
    for arch in "${ARCHS[@]}"; do
        arch_cflags=("${CFLAGS_COMMON[@]}")
        if [[ "$arch" == "arm64e.x1" ]]; then
            arch_cflags+=("${CFLAGS_ARM64E_X1[@]}")
        fi
        clang -arch "$arch" "${arch_cflags[@]}" -c -o "$OUT_DIR/$name.$arch.o" "$SCRIPT_DIR/$name.c"
        clang -arch "$arch" "${arch_cflags[@]}" -o "$OUT_DIR/$name.$arch.macho" "$OUT_DIR/$name.$arch.o"
        slices+=("$OUT_DIR/$name.$arch.macho")
    done

    # Pack the per-arch slices into a single fat binary.
    lipo -create -output "$OUT_DIR/$name.macho" "${slices[@]}"

    # Ad-hoc (self) signing, required for execution/debugging on Apple Silicon.
    # codesign -s - -f "$OUT_DIR/$name.macho"
done
