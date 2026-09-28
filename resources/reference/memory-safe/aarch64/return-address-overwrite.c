/**
 * Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions.
 * For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static void bar(void);

// Isolate return-address PAC from the compiler's stack-canary protection.
__attribute__((no_stack_protector)) int foo(void)
{
    char buffer[16];

    uintptr_t buffer_address = (uintptr_t)buffer;
    uintptr_t frame_address = (uintptr_t)__builtin_frame_address(0);
    uintptr_t return_address_slot = frame_address + sizeof(void *);
    if (return_address_slot <= buffer_address)
    {
        fputs("Unexpected stack layout; return-address overwrite skipped.\n", stderr);
        return 1;
    }

    size_t return_offset = (size_t)(return_address_slot - buffer_address);
    void (*target)(void) = bar;
    unsigned char payload[256];
    if (return_offset + sizeof(target) > sizeof(payload))
    {
        fputs("Return-address slot is outside the demonstration payload.\n", stderr);
        return 1;
    }

    memset(payload, 'A', sizeof(payload));
    memcpy(payload + return_offset, &target, sizeof(target));

    volatile unsigned char *overflow = (volatile unsigned char *)buffer;
    for (size_t index = 0; index < return_offset + sizeof(target); index++)
    {
        overflow[index] = payload[index];
    }

    return 0;
}

static void bar(void)
{
    exit(42);
}

int main()
{
    int result = foo();
    return result;
}