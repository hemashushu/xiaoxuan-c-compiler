// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use ancpp::{error::PreprocessFileError, position::Position};

#[derive(Debug, PartialEq)]
pub enum CompileError {
    PreprocessError(PreprocessFileError),
    Message(/* file_number */ usize, /* message */ String),
    MessageWithPosition(
        /* file_number */ usize,
        /* position */ Position,
        /* message */ String,
    ),
    UnexpectedEndOfDocument(/* file_number */ usize, /* message */ String),
}
