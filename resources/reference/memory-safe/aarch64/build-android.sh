#!/usr/bin/env bash
set -euo pipefail

# Builds AArch64 Android ELF executables with Termux's native Clang toolchain.
# MTE requires Android 12 (API 31) or newer and compatible hardware/kernel support.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUT_DIR="$SCRIPT_DIR/android"
ANDROID_API="${ANDROID_API:-31}"

if [[ -z "${PREFIX:-}" || ! -d "$PREFIX" ]]; then
    printf 'Run this script from Termux; PREFIX must point to the Termux prefix.\n' >&2
    exit 1
fi

if ! command -v clang >/dev/null 2>&1; then
    printf 'clang was not found; install it in Termux first.\n' >&2
    exit 1
fi

if [[ ! "$ANDROID_API" =~ ^[0-9]+$ ]] || (( ANDROID_API < 31 )); then
    printf 'ANDROID_API must be 31 or newer for MTE (got: %s).\n' "$ANDROID_API" >&2
    exit 1
fi

TARGET="aarch64-linux-android${ANDROID_API}"
CFLAGS_COMMON=(--target="$TARGET" -std=c23 -O0 -fno-unwind-tables -fno-asynchronous-unwind-tables)
CFLAGS_BASELINE=(-march=armv8.3-a+pauth)
CFLAGS_MEMTAG=(-march=armv8.5-a+memtag -fsanitize=memtag-stack,memtag-heap)

VARIANTS=(baseline memtag)
SOURCES=(simple pac out-of-bounds use-after-free double-free wild-pointer)

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.o "$OUT_DIR"/*.elf

for name in "${SOURCES[@]}"; do
    for variant in "${VARIANTS[@]}"; do
        variant_cflags=("${CFLAGS_COMMON[@]}")
        if [[ "$variant" == "memtag" ]]; then
            variant_cflags+=("${CFLAGS_MEMTAG[@]}")
        else
            variant_cflags+=("${CFLAGS_BASELINE[@]}")
        fi

        clang "${variant_cflags[@]}" -c -o "$OUT_DIR/$name.$variant.o" "$SCRIPT_DIR/$name.c"
        clang "${variant_cflags[@]}" -o "$OUT_DIR/$name.$variant.elf" "$OUT_DIR/$name.$variant.o"
    done
done
