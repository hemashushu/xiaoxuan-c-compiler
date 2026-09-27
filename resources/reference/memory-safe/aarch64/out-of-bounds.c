/**
 * Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions.
 * For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.
 */

#include <stdlib.h>

int main()
{
    int *array = malloc(4 * sizeof(int)); // valid indices: 0..3
    array[4] = 42;                        // out-of-bound write, one element past the allocation
    return array[4];
}