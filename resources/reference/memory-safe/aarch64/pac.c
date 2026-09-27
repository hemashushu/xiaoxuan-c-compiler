/**
 * Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions.
 * For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.
 */

/*
 * Check if the current CPU supports Pointer Authentication (PAC) and other related features.
 *
 * $ sysctl -a | grep hw.optional.arm.FEAT
 * hw.optional.arm.FEAT_PAuth:  1       // Pointer Authentication (PAC) support (arm64e, since A12/M1)
 * hw.optional.arm.FEAT_PAuth2: 1       // Enhanced Pointer Authentication (arm64e)
 * hw.optional.arm.FEAT_CPA2:   0/1     // Context Pointer Authentication (CPA) support (arm64e.x1, since A19/M5)
 * hw.optional.arm.FEAT_MTE:    0/1     // Hardware Memory Tagging Extension (MTE) support  support (arm64e.x1)
 *
 * References:
 * - https://learn.arm.com/learning-paths/servers-and-cloud-computing/pac/
 */

#include <stdio.h>
#include <stdint.h>

void hello_world(void)
{
    puts("Hello, world!");
}

int main()
{
    void (*fp)(void) = hello_world; // arm64e ABI signs the pointer when it's stored in memory
    fp();                           // authenticates on call -> succeeds

    // Flip a bit that lives in the pointer's PAC signature field (above the
    // 48-bit virtual address range), corrupting the signature only.
    uintptr_t bits;
    __builtin_memcpy(&bits, &fp, sizeof(bits));
    bits ^= (1ULL << 50);
    __builtin_memcpy(&fp, &bits, sizeof(bits));

    puts("invoking corrupted pointer");
    fp(); // authenticates on call -> fails -> crashes

    puts("unreachable");
    return 0;
}