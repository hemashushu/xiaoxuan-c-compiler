// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use ancpp::{location::Location, position::Position, range::Range};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Indication {
    Position(usize, Position),
    Range(usize, Range),
}

impl Default for Indication {
    fn default() -> Self {
        Self::Position(0, Position::default())
    }
}

impl Indication {
    pub fn from_position(file_number: usize, position: &Position) -> Self {
        Self::Position(file_number, *position)
    }

    pub fn from_range(file_number: usize, range: &Range) -> Self {
        Self::Range(file_number, *range)
    }

    pub fn from_locations(start: &Location, end: &Location) -> Self {
        if start.file_number != end.file_number {
            Self::Position(start.file_number, start.range.start)
        } else {
            Self::Range(start.file_number, Range::merge(&start.range, &end.range))
        }
    }
}
