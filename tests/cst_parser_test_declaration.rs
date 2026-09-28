// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

mod common;

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
};

use ancpp::{
    consts::{C23_KEYWORD_STRS, FILE_NUMBER_SOURCE_FILE_BEGIN},
    context::HeaderFileCache,
    memory_file_provider::MemoryFileProvider,
    position::Position,
};

use xiaoxuan_c_compiler::{
    cst::{
        Declaration, DeclarationSpecifier, Declarator, DirectDeclarator, ExternalDeclaration,
        InitDeclarator, Pointer, Spanned, StorageClassSpecifier, TranslationUnit, TypeSpecifier,
    },
    cst_parser::{ParseResult, parse},
    error::CompileError,
    file_position::FilePosition,
};

use pretty_assertions::assert_eq;

fn parse_source(source: &str) -> Result<ParseResult, CompileError> {
    let mut file_cache = HeaderFileCache::new();
    let mut file_provider = MemoryFileProvider::new();

    let main_file_path = Path::new("src/main.c");
    let project_root = Path::new("/projects/hello");

    file_provider.add_user_text_file(main_file_path, source);

    let compile_features = HashMap::new();
    let suppress_linters = HashSet::new();
    let predefinitions: HashMap<String, String> = HashMap::new();

    parse(
        &file_provider,
        &mut file_cache,
        &C23_KEYWORD_STRS,
        &compile_features,
        &suppress_linters,
        &predefinitions,
        FILE_NUMBER_SOURCE_FILE_BEGIN,
        main_file_path,
        project_root.join(main_file_path).as_path(),
    )
}

fn parse_source_and_get_translation_unit(source: &str) -> TranslationUnit {
    let parse_result = parse_source(source).unwrap();
    parse_result.cst
}

#[test]
fn test_cst_parse_declaration_variable() {
    // simple variable declaration
    // `int x;`

    assert_eq!(
        parse_source_and_get_translation_unit("int x;"),
        TranslationUnit {
            external_declarations: vec![Spanned::<ExternalDeclaration> {
                pos: FilePosition {
                    file_number: FILE_NUMBER_SOURCE_FILE_BEGIN,
                    position: Position::new(0, 0, 0)
                },
                node: ExternalDeclaration::Declaration(Declaration::Var {
                    attributes: vec![],
                    declaration_specifiers: vec![DeclarationSpecifier::Type(TypeSpecifier::Int)],
                    init_declarators: vec![InitDeclarator::Plain(Declarator {
                        pointer: None,
                        direct: DirectDeclarator::Identifier("x".to_string())
                    })],
                })
            }]
        }
    );

    // variable declaration with storage class, type qualifiers, and pointer
    // `static long int * x;`

    assert_eq!(
        parse_source_and_get_translation_unit("static long int * x;"),
        TranslationUnit {
            external_declarations: vec![Spanned::<ExternalDeclaration> {
                pos: FilePosition {
                    file_number: FILE_NUMBER_SOURCE_FILE_BEGIN,
                    position: Position::new(0, 0, 0)
                },
                node: ExternalDeclaration::Declaration(Declaration::Var {
                    attributes: vec![],
                    declaration_specifiers: vec![
                        DeclarationSpecifier::StorageClass(StorageClassSpecifier::Static),
                        DeclarationSpecifier::Type(TypeSpecifier::Long),
                        DeclarationSpecifier::Type(TypeSpecifier::Int),
                    ],
                    init_declarators: vec![InitDeclarator::Plain(Declarator {
                        pointer: Some(Pointer {
                            qualifiers: vec![],
                            inner: None,
                        }),
                        direct: DirectDeclarator::Identifier("x".to_string())
                    })],
                })
            }]
        }
    );
}
