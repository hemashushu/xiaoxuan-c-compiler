// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::{collections::HashMap, path::Path};

use ancpp::{
    FILE_NUMBER_SOURCE_FILE_BEGIN,
    context::{HeaderFileCache, PreprocessResult},
    error::PreprocessFileError,
    memory_file_provider::MemoryFileProvider,
    process_source_file,
    token::{C23_KEYWORD_STRS, TokenWithLocation},
};

// Help function to process a source file with additional header files and get the preprocess result.
fn preprocess_with_headers_and_result(
    main_file_relative_path: &str,
    main_file_content: &str,
    user_header_files: &[(/* relative_path */ &str, /* content */ &str)],
    user_binary_files: &[(/* relative_path */ &str, /* content */ &[u8])],
    system_header_files: &[(/* relative_path */ &str, /* content */ &str)],
) -> Result<PreprocessResult, PreprocessFileError> {
    let mut file_cache = HeaderFileCache::new();
    let mut file_provider = MemoryFileProvider::new();

    let main_file_path = Path::new(main_file_relative_path);
    let project_root = Path::new("/projects/hello");

    file_provider.add_user_text_file(main_file_path, main_file_content);

    for (path, content) in user_header_files {
        file_provider.add_user_text_file(Path::new(path), content);
    }

    for (path, content) in user_binary_files {
        file_provider.add_user_binary_file(Path::new(path), content.to_vec());
    }

    for (path, content) in system_header_files {
        file_provider.add_system_file(Path::new(path), content);
    }

    let predefinitions: HashMap<String, String> = HashMap::new();

    process_source_file(
        &file_provider,
        &mut file_cache,
        &C23_KEYWORD_STRS,
        &predefinitions,
        false,
        false,
        FILE_NUMBER_SOURCE_FILE_BEGIN,
        main_file_path,
        project_root.join(main_file_path).as_path(),
    )
}

// Help function to process a source file with additional header files and get the output tokens.
fn preprocess_with_headers(
    main_file_relative_path: &str,
    main_file_content: &str,
    user_header_files: &[(&str, &str)],
    user_binary_files: &[(&str, &[u8])],
    system_header_files: &[(&str, &str)],
) -> Vec<TokenWithLocation> {
    preprocess_with_headers_and_result(
        main_file_relative_path,
        main_file_content,
        user_header_files,
        user_binary_files,
        system_header_files,
    )
    .unwrap()
    .token_with_locations
}

// Help function to process a source file without any additional header files and get the output tokens.
fn preprocess_with_file_path(
    main_file_relative_path: &str,
    main_file_content: &str,
) -> Vec<TokenWithLocation> {
    preprocess_with_headers(main_file_relative_path, main_file_content, &[], &[], &[])
}

// Help function to process a source file without any additional header files and get the output tokens.
#[allow(dead_code)]
pub fn preprocess(main_file_content: &str) -> Vec<TokenWithLocation> {
    preprocess_with_file_path("src/main.c", main_file_content)
}

#[cfg(test)]
mod tests {
    use ancpp::token::{IntegerNumber, IntegerNumberWidth, Number, Punctuator, Token};
    use pretty_assertions::assert_eq;

    use crate::utils::preprocess;

    #[test]
    fn test_preprocess() {
        let token_with_locations = preprocess(
            r#"
            int main() {
                return 0;
            }
            "#,
        );

        let tokens = token_with_locations
            .into_iter()
            .map(|token_with_location| token_with_location.token)
            .collect::<Vec<_>>();

        assert_eq!(
            tokens,
            vec![
                Token::Identifier("int".to_owned()),
                Token::Identifier("main".to_owned()),
                Token::Punctuator(Punctuator::ParenthesisOpen),
                Token::Punctuator(Punctuator::ParenthesisClose),
                Token::Punctuator(Punctuator::BraceOpen),
                Token::Identifier("return".to_owned()),
                Token::Number(Number::Integer(IntegerNumber {
                    value: "0".to_owned(),
                    unsigned: false,
                    length: IntegerNumberWidth::Default
                })),
                Token::Punctuator(Punctuator::Semicolon),
                Token::Punctuator(Punctuator::BraceClose),
            ]
        );
    }
}
