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
    int *p = malloc(sizeof(int));
    free(p);
    *p = 42; // use after free: `p` is dereferenced after being freed
    return *p;
}