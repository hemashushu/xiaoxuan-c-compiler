#!/usr/bin/env bash
set -euo pipefail

# Builds AArch64 Android ELF executables with Termux's native Clang toolchain.
# MTE requires Android 12 (API 31) or newer and compatible hardware (e.g.
# Google Tensor G3+ or Google Pixel 8 and newer) support.

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

# Builds three aarch64 ELF executable variants for each source file:
# - baseline: default AArch64 target, without explicitly enabling PAC or MTE.
# - pac:      Armv8.3-A with PAC return-address signing enabled.
# - memtag:   PAC return-address signing + MTE and clang's memtag sanitizers.

TARGET="aarch64-linux-android${ANDROID_API}"
CFLAGS_COMMON=(--target="$TARGET" -std=c23 -O0 -fno-unwind-tables -fno-asynchronous-unwind-tables)
CFLAGS_PAC=(-march=armv8.3-a+pauth -mbranch-protection=pac-ret)
CFLAGS_MEMTAG=(-march=armv8.5-a+memtag -mbranch-protection=pac-ret -fsanitize=memtag-stack,memtag-heap)

VARIANTS=(baseline pac memtag)
SOURCES=(return-address-overwrite out-of-bounds use-after-free double-free wild-pointer)

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.o "$OUT_DIR"/*.elf

for name in "${SOURCES[@]}"; do
    for variant in "${VARIANTS[@]}"; do
        variant_cflags=("${CFLAGS_COMMON[@]}")
        case "$variant" in
            pac)
                variant_cflags+=("${CFLAGS_PAC[@]}")
                ;;
            memtag)
                variant_cflags+=("${CFLAGS_MEMTAG[@]}")
                ;;
        esac

        clang "${variant_cflags[@]}" -c -o "$OUT_DIR/$name.$variant.o" "$SCRIPT_DIR/$name.c"
        clang "${variant_cflags[@]}" -o "$OUT_DIR/$name.$variant.elf" "$OUT_DIR/$name.$variant.o"
    done
done
