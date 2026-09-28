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
 * - https://oliviagallucci.com/the-anatomy-of-a-mach-o-structure-code-signing-and-pac/
 */

#include <stdio.h>
#include <ptrauth.h>

static int foo(int x)
{
    return x + 1;
}

int main()
{
    void *raw = (void *)&foo;
    void *signedp = ptrauth_sign_unauthenticated(raw,
                                                 ptrauth_key_process_dependent_code, 0);
    printf("raw    = %p\n", raw);
    printf("signed = %p\n", signedp);
}