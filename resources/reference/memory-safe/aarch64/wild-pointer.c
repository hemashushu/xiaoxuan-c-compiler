/**
 * Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions.
 * For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.
 */

int main()
{
    int *p;  // wild pointer: declared but never initialized
    *p = 42; // dereferences an indeterminate address
    return *p;
}