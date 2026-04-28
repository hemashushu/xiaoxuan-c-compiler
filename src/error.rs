// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use ancpp::error::PreprocessFileError;

use crate::indication::Indication;

pub enum CompileError {
    PreprocessError(PreprocessFileError),
    Message(/* file_number */ usize, /* message */ String),
    MessageWithIndication(
        /* indication */ Indication,
        /* message */ String,
    ),
    UnexpectedEndOfDocument(/* file_number */ usize, /* message */ String),
}
