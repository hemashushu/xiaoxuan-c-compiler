#!/usr/bin/env bash
set -euo pipefail

# Builds two aarch64 ELF executable variants for each source file:
# - baseline: pointer authentication (PAC) ABI (armv8.3-a).
# - memtag:   PAC + Memory Tagging Extension (MTE) ABI (armv8.5-a+memtag),
#             instrumented with clang's memtag-stack/memtag-heap sanitizers.
#
# Targets native aarch64 Linux (e.g. Android Termux Ubuntu on Google Tensor G5,
# which supports arm64e.x1-equivalent PAC2 + MTE hardware features).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

OUT_DIR="$SCRIPT_DIR/linux"

CFLAGS_COMMON=(-std=c23 -O0 -fno-unwind-tables -fno-asynchronous-unwind-tables)
CFLAGS_BASELINE=(-march=armv8.3-a+pauth)
# memtag variant only: instrument stack/heap accesses with MTE tag-check instructions (irg/stg/...).
CFLAGS_MEMTAG=(-march=armv8.5-a+memtag -fsanitize=memtag-stack,memtag-heap)

VARIANTS=(baseline memtag)
SOURCES=(simple pac out-of-bounds use-after-free double-free wild-pointer)

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.o "$OUT_DIR"/*.elf || true

for name in "${SOURCES[@]}"; do
    for variant in "${VARIANTS[@]}"; do
        variant_cflags=("${CFLAGS_COMMON[@]}")
        # link_cflags omits -fsanitize=memtag*: clang's driver requires a target-specific
        # sanitizer runtime (only shipped for Android/Fuchsia) to *link* with that flag,
        # even though the instrumentation is already compiled into the object file.
        link_cflags=("${CFLAGS_COMMON[@]}")
        if [[ "$variant" == "memtag" ]]; then
            variant_cflags+=("${CFLAGS_MEMTAG[@]}")
            link_cflags+=(-march=armv8.5-a+memtag)
        else
            variant_cflags+=("${CFLAGS_BASELINE[@]}")
            link_cflags+=("${CFLAGS_BASELINE[@]}")
        fi
        clang "${variant_cflags[@]}" -c -o "$OUT_DIR/$name.$variant.o" "$SCRIPT_DIR/$name.c"
        clang "${link_cflags[@]}" -o "$OUT_DIR/$name.$variant.elf" "$OUT_DIR/$name.$variant.o"
    done
done
