// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use ancpp::{
    context::{FileProvider, HeaderFileCache, PreprocessResult},
    linter::Linter,
    location::Location,
    peekable_iter::PeekableIter,
    process_source_file,
    token::{
        C23_KEYWORD_STRS, C23Keyword, IntegerNumber, IntegerNumberWidth, Number, Punctuator, Token,
        TokenWithLocation,
    },
};

use crate::{
    cst::{
        AbstractDeclarator, AlignmentSpecifier, ArraySize, AssignOp, Attribute, AttributeToken,
        BinaryOp, BlockItem, Constant, Declaration, DeclarationSpecifier, Declarator, Designator,
        DirectAbstractDeclarator, DirectDeclarator, EnumSpecifier, Enumerator, Expr, Expression,
        ExtDecl, ExternalDeclaration, ForInit, FunctionDefinition, FunctionParams,
        FunctionSpecifier, GenericAssociation, GenericControlling, GenericSelection,
        InitDeclarator, Initializer, InitializerItem, IterationStatement, JumpStatement, Label,
        LabelKind, LabeledStatement, ParameterDeclaration, ParameterTypeList, Pointer,
        SelectionStatement, SizeofOperand, Spanned, SpecifierQualifier, Statement, StaticAssert,
        Stmt, StorageClassSpecifier, StringLiteral, StructDeclaration, StructDeclarator,
        StructOrUnion, StructOrUnionSpecifier, TranslationUnit, TypeName, TypeQualifier,
        TypeSpecifier, TypeofSpecifier, UnaryOp,
    },
    error::CompileError,
    file_position::FilePosition,
};

#[derive(Debug, PartialEq, Clone, Copy)]
enum SymbolType {
    TypeName,     // typedef name
    EnumConstant, // enumeration constant
}

#[derive(Debug, PartialEq, Clone)]
struct SymbolTable {
    symbols: HashMap<String, SymbolType>,
}

pub struct Parser<'a> {
    upstream: &'a mut PeekableIter<'a, TokenWithLocation>,

    /// Message that report to users
    linters: Vec<Linter>,

    /// The symbol table stack for the current parsing context, used for checking if an identifier is a typedef name or an enumeration constant.
    symbol_stack: Vec<SymbolTable>,

    /// The location of the last consumed token by `next_token` or `next_token_with_location`.
    pub last_location: Location,
}

impl<'a> Parser<'a> {
    pub fn new(
        upstream: &'a mut PeekableIter<'a, TokenWithLocation>,
        linters: Vec<Linter>,
    ) -> Self {
        Self {
            upstream,
            linters,
            symbol_stack: vec![],
            last_location: Location::default(),
        }
    }

    fn next_token(&mut self) -> Option<Token> {
        match self.next_token_with_location() {
            Some(TokenWithLocation { token, location }) => {
                self.last_location = location;
                Some(token)
            }
            None => None,
        }
    }

    fn next_token_with_location(&mut self) -> Option<TokenWithLocation> {
        match self.upstream.next() {
            Some(token_with_location) => {
                self.last_location = token_with_location.location;
                Some(token_with_location)
            }
            None => None,
        }
    }

    fn peek_token(&self, offset: usize) -> Option<&Token> {
        match self.upstream.peek(offset) {
            Some(TokenWithLocation { token, .. }) => Some(token),
            None => None,
        }
    }

    fn peek_location(&self, offset: usize) -> Option<&Location> {
        match self.upstream.peek(offset) {
            Some(TokenWithLocation { location, .. }) => Some(location),
            None => None,
        }
    }

    /// Returns the [`FilePosition`] of the token at `offset` in the peek buffer.
    /// Falls back to `last_location` when the stream is exhausted (e.g. at EOF).
    fn peek_file_position(&self, offset: usize) -> FilePosition {
        match self.peek_location(offset) {
            Some(loc) => FilePosition::new(loc.file_number, &loc.range.start),
            None => FilePosition::new(
                self.last_location.file_number,
                &self.last_location.range.start,
            ),
        }
    }

    // Peek the next token and check if it equals to the expected token,
    // return false if not equals or no more token,
    // error if lexing error occurs during peeking.
    fn peek_and_equals(&self, offset: usize, expected_token: &Token) -> bool {
        matches!(
            self.peek_token(offset),
            Some(token) if token == expected_token)
    }

    fn peek_and_equals_identifier(&self, offset: usize, expected_identifier: &str) -> bool {
        match self.peek_token(offset) {
            Some(Token::Identifier(id)) => id == expected_identifier,
            _ => false,
        }
    }

    fn peek_and_equals_keyword(&self, offset: usize, expected_keyword: C23Keyword) -> bool {
        self.peek_and_equals_identifier(offset, C23_KEYWORD_STRS[expected_keyword as usize])
    }

    fn peek_and_equals_punctuator(&self, offset: usize, expected_punctuator: Punctuator) -> bool {
        self.peek_and_equals(offset, &Token::Punctuator(expected_punctuator))
    }

    fn consume_string(&mut self) -> Result<String, CompileError> {
        match self.next_token() {
            Some(Token::String(s, _)) => Ok(s),
            Some(_) => {
                Err(self.build_error_with_last_location("Expected string literal.".to_string()))
            }
            None => Err(self.build_error_unexpected_eof("Expected string literal.".to_string())),
        }
    }

    fn consume_identifier(&mut self) -> Result<String, CompileError> {
        match self.next_token() {
            Some(Token::Identifier(id)) => Ok(id),
            Some(_) => Err(self.build_error_with_last_location("Expected identifier.".to_string())),
            None => Err(self.build_error_unexpected_eof("Expected identifier.".to_string())),
        }
    }

    fn consume_nonkeyword_identifier(&mut self) -> Result<String, CompileError> {
        match self.next_token() {
            Some(Token::Identifier(id)) if !C23_KEYWORD_STRS.contains(&id.as_str()) => Ok(id),
            Some(_) => {
                Err(self
                    .build_error_with_last_location("Expected non-keyword identifier.".to_string()))
            }
            None => {
                Err(self.build_error_unexpected_eof("Expected non-keyword identifier.".to_string()))
            }
        }
    }

    /// Consume the next token and check if it is the expected token,
    /// return Ok(()) if it matches, otherwise return an Error.
    fn consume_and_assert(
        &mut self,
        expected_token: &Token,
        token_description: &str,
    ) -> Result<(), CompileError> {
        match self.next_token() {
            Some(token) => {
                if &token == expected_token {
                    Ok(())
                } else {
                    Err(self.build_error_with_last_location(format!(
                        "Expected token: {}.",
                        token_description
                    )))
                }
            }
            None => {
                Err(self
                    .build_error_unexpected_eof(format!("Expected token: {}.", token_description)))
            }
        }
    }

    fn consume_and_assert_keyword(
        &mut self,
        expected_keyword: C23Keyword,
    ) -> Result<(), CompileError> {
        let keyword_str = C23_KEYWORD_STRS[expected_keyword as usize];

        match self.next_token() {
            Some(Token::Identifier(id)) => {
                if id == keyword_str {
                    Ok(())
                } else {
                    Err(self.build_error_with_last_location(format!(
                        "Expected keyword: {}.",
                        keyword_str
                    )))
                }
            }
            Some(_) => {
                Err(self
                    .build_error_with_last_location(format!("Expected keyword: {}.", keyword_str)))
            }
            None => {
                Err(self.build_error_unexpected_eof(format!("Expected keyword: {}.", keyword_str)))
            }
        }
    }

    fn consume_opening_parenthesis(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(
            &Token::Punctuator(Punctuator::ParenthesisOpen),
            "opening parenthesis",
        )
    }

    fn consume_closing_parenthesis(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(
            &Token::Punctuator(Punctuator::ParenthesisClose),
            "closing parenthesis",
        )
    }

    fn consume_opening_brace(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(&Token::Punctuator(Punctuator::BraceOpen), "opening brace")
    }

    fn consume_closing_brace(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(&Token::Punctuator(Punctuator::BraceClose), "closing brace")
    }

    fn consume_opening_bracket(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(
            &Token::Punctuator(Punctuator::BracketOpen),
            "opening bracket",
        )
    }

    fn consume_closing_bracket(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(
            &Token::Punctuator(Punctuator::BracketClose),
            "closing bracket",
        )
    }

    fn consume_semicolon(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(&Token::Punctuator(Punctuator::Semicolon), "semicolon")
    }

    fn consume_comma(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(&Token::Punctuator(Punctuator::Comma), "comma")
    }

    fn consume_question_mark(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(
            &Token::Punctuator(Punctuator::QuestionMark),
            "question mark",
        )
    }

    fn consume_colon(&mut self) -> Result<(), CompileError> {
        self.consume_and_assert(&Token::Punctuator(Punctuator::Colon), "colon")
    }

    /// Returns `true` if the token at `offset` is a type-qualifier keyword.
    fn peek_and_is_type_qualifier(&self, offset: usize) -> bool {
        matches!(
            self.peek_token(offset),
            Some(Token::Identifier(id)) if matches!(
                id.as_str(), "const" | "restrict" | "volatile" | "_Atomic"
            )
        )
    }

    /// Returns `true` if the token at `offset` can begin a `specifier_qualifier_list`:
    /// - `type_specifier`
    /// - `type_qualifier`
    /// - `alignment_specifier`
    /// - `attribute_specifier`
    fn peek_and_is_specifier_qualifier(&self, offset: usize) -> bool {
        match self.peek_token(offset) {
            Some(Token::Identifier(id)) => {
                matches!(
                    id.as_str(),
                    /* `type_specifier` */
                    "void"
                        | "char"
                        | "short"
                        | "int"
                        | "long"
                        | "float"
                        | "double"
                        | "signed"
                        | "unsigned"
                        | "bool"
                        | "_Complex"
                        | "_Imaginary"
                        | "_Decimal32"
                        | "_Decimal64"
                        | "_Decimal128"
                        | "_BitInt"
                        | "typeof"          // `typeof_specifier`
                        | "typeof_unqual"   // `typeof_specifier`
                        | "_Atomic"         // `atomic_type_specifier`
                        | "struct"          // `struct_or_union_specifier`
                        | "union"           // `struct_or_union_specifier`
                        | "enum"            // `enum_specifier`
                        /* `type_qualifier` */
                        | "const"
                        | "restrict"
                        | "volatile"
                        /* `alignment_specifier` */
                        | "alignas"
                ) || self.exists_typedef_name(id) // `typedef_name`
            }
            Some(Token::Punctuator(Punctuator::AttributeOpen)) => true, // `attribute_specifier`
            _ => false,
        }
    }

    /// Returns `true` if the token at `offset` can begin a `declaration_specifier`:
    /// - `storage_class_specifier`
    /// - `type_specifier`
    /// - `type_qualifier`
    /// - `function_specifier`
    /// - `alignment_specifier`
    ///
    /// C23 deprecated keywords are not included.
    fn peek_and_is_declaration_specifier(&self, offset: usize) -> bool {
        match self.peek_token(offset) {
            Some(Token::Identifier(id)) => {
                matches!(
                    id.as_str(),
                    /* `storage_class_specifier` */
                    "typedef"
                        | "extern"
                        | "static"
                        | "thread_local"
                        | "auto"
                        | "register"
                        | "constexpr"
                        /* `type_specifier` */
                        | "void"
                        | "char"
                        | "short"
                        | "int"
                        | "long"
                        | "float"
                        | "double"
                        | "signed"
                        | "unsigned"
                        | "bool"
                        | "_Complex"
                        | "_Imaginary"
                        | "_Decimal32"
                        | "_Decimal64"
                        | "_Decimal128"
                        | "_BitInt"
                        | "typeof"
                        | "typeof_unqual"
                        | "_Atomic"
                        | "struct"
                        | "union"
                        | "enum"
                        /* `type_qualifier` */
                        | "const"
                        | "restrict"
                        | "volatile"
                        /* `function_specifier` */
                        | "inline"
                        /* `alignment_specifier` */
                        | "alignas"
                ) || self.exists_typedef_name(id) // `typedef_name`
            }
            _ => false,
        }
    }

    /// Returns `true` when the token at `offset` is a non-keyword identifier.
    fn peek_and_is_nonkeyword_identifier(&self, offset: usize) -> bool {
        match self.peek_token(offset) {
            Some(Token::Identifier(id)) => !C23_KEYWORD_STRS.contains(&id.as_str()),
            _ => false,
        }
    }

    /// Returns `true` when the tokens starting at `offset` look like the head of a named
    /// declarator (i.e., after skipping any pointer `*`s and qualifiers the next meaningful
    /// piece is a bare non-keyword, non-typedef-name identifier).
    ///
    /// Used to split `parameter_declaration` into its named vs. abstract variants.
    fn peek_and_is_named_declarator(&self, offset: usize) -> bool {
        let mut off = offset;
        // Skip leading pointer * and qualifiers

        while let Some(Token::Punctuator(Punctuator::Multiply)) = self.peek_token(off) {
            off += 1;
            while self.peek_and_is_type_qualifier(off) {
                off += 1;
            }
        }

        match self.peek_token(off) {
            // Non-keyword, non-typedef identifier -> directly named
            Some(Token::Identifier(id))
                if !C23_KEYWORD_STRS.contains(&id.as_str()) && !self.exists_typedef_name(id) =>
            {
                true
            }
            // Opening paren -> could be (declarator): look inside after any pointer stars
            Some(Token::Punctuator(Punctuator::ParenthesisOpen)) => {
                let mut inner = off + 1;

                while let Some(Token::Punctuator(Punctuator::Multiply)) = self.peek_token(inner) {
                    inner += 1;
                    while self.peek_and_is_type_qualifier(inner) {
                        inner += 1;
                    }
                }

                matches!(
                    self.peek_token(inner),
                    Some(Token::Identifier(id))
                        if !C23_KEYWORD_STRS.contains(&id.as_str())
                            && !self.exists_typedef_name(id)
                )
            }
            _ => false,
        }
    }

    fn build_error_with_last_location(&self, msg: String) -> CompileError {
        CompileError::MessageWithPosition(
            self.last_location.file_number,
            self.last_location.range.start,
            msg,
        )
    }

    fn build_error_with_peek_location(&self, offset: usize, msg: String) -> CompileError {
        let location = self.peek_location(offset).unwrap();
        CompileError::MessageWithPosition(location.file_number, location.range.start, msg)
    }

    fn build_error_unexpected_eof(&self, msg: String) -> CompileError {
        CompileError::UnexpectedEndOfDocument(self.last_location.file_number, msg)
    }

    fn is_eof(&self) -> bool {
        self.peek_token(0).is_none()
    }
}

/// Symbol table manipulation and identifier type checking utilities.
impl<'a> Parser<'a> {
    /// Looks up the identifier in the symbol stack and returns its SymbolType if found,
    /// or None if not found (i.e., it's a regular identifier).
    fn get_symbol_type(&self, identifier: &str) -> Option<SymbolType> {
        for symbol_table in self.symbol_stack.iter().rev() {
            if let Some(symbol_type) = symbol_table.symbols.get(identifier) {
                return Some(*symbol_type);
            }
        }
        None
    }

    fn exists_typedef_name(&self, identifier: &str) -> bool {
        matches!(self.get_symbol_type(identifier), Some(SymbolType::TypeName))
    }

    fn exists_enum_constant(&self, identifier: &str) -> bool {
        matches!(
            self.get_symbol_type(identifier),
            Some(SymbolType::EnumConstant)
        )
    }

    fn add_typename(&mut self, identifier: String) {
        if let Some(symbol_table) = self.symbol_stack.last_mut() {
            symbol_table
                .symbols
                .insert(identifier, SymbolType::TypeName);
        }
    }

    fn add_enum_constant(&mut self, identifier: String) {
        if let Some(symbol_table) = self.symbol_stack.last_mut() {
            symbol_table
                .symbols
                .insert(identifier, SymbolType::EnumConstant);
        }
    }

    fn enter_scope(&mut self) {
        self.symbol_stack.push(SymbolTable {
            symbols: HashMap::new(),
        });
    }

    fn leave_scope(&mut self) {
        self.symbol_stack.pop();
    }
}

pub struct ParseResult {
    pub cst: TranslationUnit,
    pub linters: Vec<Linter>,
}

#[allow(clippy::too_many_arguments)]
pub fn parse<T>(
    file_provider: &T,
    file_cache: &mut HeaderFileCache,
    reserved_identifiers: &[&str],
    compile_features: &HashMap<String, bool>,
    suppress_linters: &HashSet<String>,
    predefinitions: &HashMap<String, String>,
    source_file_number: usize,
    source_file_path_name: &Path,
    source_file_canonical_full_path: &Path,
) -> Result<ParseResult, CompileError>
where
    T: FileProvider,
{
    let preprocess_result = process_source_file(
        file_provider,
        file_cache,
        reserved_identifiers,
        compile_features,
        suppress_linters,
        predefinitions,
        source_file_number,
        source_file_path_name,
        source_file_canonical_full_path,
    )
    .map_err(CompileError::PreprocessError)?;

    let PreprocessResult {
        token_with_locations,
        linters,
    } = preprocess_result;
    let mut token_iter = token_with_locations.into_iter();
    let mut peekable_token_iter = PeekableIter::new(&mut token_iter);
    let mut parser = Parser::new(&mut peekable_token_iter, linters);

    parser.enter_scope(); // build the root/global scope

    let cst = parser.parse_translation_unit()?;
    let linters = parser.linters;
    let parse_result = ParseResult { cst, linters };
    Ok(parse_result)
}

impl<'a> Parser<'a> {
    fn parse_translation_unit(&mut self) -> Result<TranslationUnit, CompileError> {
        // translation_unit
        //     : external_declaration
        //     | translation_unit external_declaration
        //     ;

        let mut external_declarations = Vec::new();
        while !self.is_eof() {
            external_declarations.push(self.parse_external_declaration()?);
        }

        Ok(TranslationUnit {
            external_declarations,
        })
    }

    fn parse_external_declaration(&mut self) -> Result<ExtDecl, CompileError> {
        // external_declaration
        //     : function_definition
        //     | declaration
        //     ;

        let pos = self.peek_file_position(0);
        let declaration = self.parse_declaration()?;

        let external_declaration = if let Declaration::Var {
            attributes,
            declaration_specifiers,
            init_declarators,
        } = declaration
        {
            // The difference between a `declaration` and a `function_definition` is that
            // the latter has a compound statement (`{...}`) after the declarator list.
            if self.peek_and_equals(0, &Token::Punctuator(Punctuator::BraceOpen)) {
                // it is `function_definition`

                // `'{' '}'` or `'{' block_item_list '}'`
                let body = self.parse_compound_statement()?;

                // Convert `init_declarators` to `declarator` for function definition,
                // and check if it's valid (e.g. no initializer allowed, and only one declarator allowed)
                let mut declarator = vec![];
                for init_declarator in init_declarators {
                    if let InitDeclarator::Init(_, _) = init_declarator {
                        return Err(CompileError::MessageWithPosition(
                            pos.file_number,
                            pos.position,
                            "Initializer is not allowed in function definition.".to_string(),
                        ));
                    }
                    if let InitDeclarator::Plain(decl) = init_declarator {
                        declarator.push(decl);
                    }
                }

                if declarator.len() != 1 {
                    return Err(CompileError::MessageWithPosition(
                        pos.file_number,
                        pos.position,
                        "Only one declarator is allowed in function definition.".to_string(),
                    ));
                }

                Spanned::new(
                    ExternalDeclaration::Function(FunctionDefinition {
                        attributes,
                        declaration_specifiers,
                        declarator: declarator.into_iter().next().unwrap(),
                        body,
                    }),
                    pos,
                )
            } else {
                // it is `declaration`
                self.consume_semicolon()?; // consume the ';' after the declaration

                Spanned::new(
                    ExternalDeclaration::Declaration(Declaration::Var {
                        attributes,
                        declaration_specifiers,
                        init_declarators,
                    }),
                    pos,
                )
            }
        } else {
            // it is attribute declaration or static assert declaration.
            Spanned::new(ExternalDeclaration::Declaration(declaration), pos)
        };

        Ok(external_declaration)
    }

    fn parse_declaration(&mut self) -> Result<Declaration, CompileError> {
        // declaration
        //     : declaration_specifiers ';'
        //     | declaration_specifiers init_declarator_list ';'
        //     | attribute_specifier_sequence declaration_specifiers ';'
        //                 /* C23: leading attribute(s), no declarator */
        //     | attribute_specifier_sequence declaration_specifiers init_declarator_list ';'
        //                 /* C23: leading attribute(s) on a declaration */
        //     | attribute_specifier_sequence ';'
        //                 /* C23: standalone attribute declaration (attribute-declaration) */
        //     | static_assert_declaration
        //     ;

        if self.peek_and_equals_keyword(0, C23Keyword::StaticAssert) {
            // `static_assert_declaration`
            let static_assert = self.parse_static_assert_declaration()?;
            return Ok(Declaration::StaticAssert(static_assert));
        }

        // Optional leading attribute_specifier_sequence. The ';' is NOT consumed here because
        // the attributes may be part of a declaration (e.g. `[[nodiscard]] int foo;`).
        let attributes = if self.peek_and_equals_punctuator(0, Punctuator::AttributeOpen) {
            self.parse_attribute_specifier_sequence()?
        } else {
            vec![]
        };

        if self.peek_and_equals_punctuator(0, Punctuator::Semicolon) {
            // `attribute_specifier_sequence ';'`  -> standalone attribute declaration
            self.consume_semicolon()?;
            return Ok(Declaration::Attribute(attributes));
        }

        // `declaration_specifiers init_declarator_list ';'`
        //
        // Do not consume the ';' here, because the `function_definition`
        // has the similar prefix, and we need to check if it's a `function_definition` or
        // a `declaration` in further step.

        let declaration_specifiers = self.parse_declaration_specifiers()?;
        let init_declarators = self.parse_init_declarator_list()?;

        // Register typedef names into the current scope symbol table so that subsequent
        // parsing can correctly identify typedef names vs. regular identifiers.
        let is_typedef = declaration_specifiers.iter().any(|s| {
            matches!(
                s,
                DeclarationSpecifier::StorageClass(StorageClassSpecifier::Typedef)
            )
        });

        if is_typedef {
            for init_declarator in &init_declarators {
                let declarator = match init_declarator {
                    InitDeclarator::Plain(d) => d,
                    InitDeclarator::Init(d, _) => d,
                };
                if let Some(name) = extract_declarator_name(&declarator.direct) {
                    self.add_typename(name);
                }
            }
        }

        Ok(Declaration::Var {
            attributes,
            declaration_specifiers,
            init_declarators,
        })
    }

    fn parse_attribute_specifier_sequence(&mut self) -> Result<Vec<Attribute>, CompileError> {
        // attribute_specifier_sequence
        //     : attribute_specifier
        //     | attribute_specifier_sequence attribute_specifier
        //     ;
        //
        // attribute_specifier
        //     : '[[' attribute_list ']]'
        //     ;
        //
        // attribute_list
        //     : attribute?
        //     | attribute_list ',' attribute?
        //     ;
        //
        // attribute
        //     : attribute_token ('(' balanced_token_sequence? ')')?
        //     ;
        //
        // attribute_token
        //     : IDENTIFIER                     (simple)
        //     | IDENTIFIER '::' IDENTIFIER     (namespaced)
        //     ;

        let mut attributes = Vec::new();

        while self.peek_and_equals_punctuator(0, Punctuator::AttributeOpen) {
            self.consume_and_assert(&Token::Punctuator(Punctuator::AttributeOpen), "\"[[\"")?;

            // Parse zero or more comma-separated attribute entries
            if !self.peek_and_equals_punctuator(0, Punctuator::AttributeClose) {
                while !self.is_eof() {
                    // The attribute_token is an identifier or namespace-prefixed identifier.

                    let token = match self.peek_token(0) {
                        Some(Token::Identifier(id_ref)) => {
                            let id = id_ref.to_owned();

                            if C23_KEYWORD_STRS.contains(&id.as_str()) {
                                return Err(self.build_error_with_peek_location(
                                    0,
                                    "Expected identifier in attribute token, but found keyword."
                                        .to_string(),
                                ));
                            }

                            self.next_token(); // consume the identifier token
                            AttributeToken::Simple(id)
                        }
                        Some(Token::NamespaceIdentifier(ns_ref, id_ref)) => {
                            let ns = ns_ref.to_owned();
                            let id = id_ref.to_owned();

                            if C23_KEYWORD_STRS.contains(&ns.as_str()) {
                                return Err(self.build_error_with_peek_location(
                                    0,
                                    "Expected identifier in attribute token namespace prefix, but found keyword.".to_string(),
                                ));
                            }

                            if C23_KEYWORD_STRS.contains(&id.as_str()) {
                                return Err(self.build_error_with_peek_location(
                                    0,
                                    "Expected identifier in attribute token name, but found keyword.".to_string(),
                                ));
                            }

                            self.next_token(); // consume the namespace identifier token
                            AttributeToken::Namespaced {
                                prefix: ns,
                                name: id,
                            }
                        }
                        Some(_) => {
                            return Err(self.build_error_with_peek_location(
                                0,
                                "Expected identifier in attribute token.".to_string(),
                            ));
                        }
                        None => {
                            return Err(self.build_error_unexpected_eof(
                                "Unexpected end of file while parsing attribute token.".to_string(),
                            ));
                        }
                    };

                    let args = if self.peek_and_equals_punctuator(0, Punctuator::ParenthesisOpen) {
                        self.consume_opening_parenthesis()?;
                        let raw = self.parse_balanced_token_string()?;
                        self.consume_closing_parenthesis()?;
                        Some(raw)
                    } else {
                        None
                    };

                    attributes.push(Attribute { token, args });

                    if self.peek_and_equals_punctuator(0, Punctuator::Comma) {
                        self.consume_comma()?;
                    } else {
                        break;
                    }
                }
            }

            self.consume_and_assert(&Token::Punctuator(Punctuator::AttributeClose), "\"]]\"")?;
        }

        Ok(attributes)
    }

    /// Collects tokens between an already-consumed `(` and the matching `)`, returning their
    /// Display-formatted text joined by spaces.  The outer `)` is left in the stream.
    fn parse_balanced_token_string(&mut self) -> Result<String, CompileError> {
        let mut result = String::new();
        let mut depth: usize = 0;

        loop {
            match self.peek_token(0) {
                Some(Token::Punctuator(Punctuator::ParenthesisClose)) if depth == 0 => {
                    break;
                }
                Some(Token::Punctuator(Punctuator::ParenthesisOpen)) => {
                    depth += 1;
                    self.next_token();
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push('(');
                }
                Some(Token::Punctuator(Punctuator::ParenthesisClose)) => {
                    depth -= 1;
                    self.next_token();
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push(')');
                }
                Some(_) => {
                    let tok = self.next_token().unwrap();
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push_str(&tok.to_string());
                }
                None => {
                    return Err(self.build_error_unexpected_eof(
                        "Unexpected end of file while parsing attribute arguments.".to_string(),
                    ));
                }
            }
        }

        Ok(result)
    }

    fn parse_static_assert_declaration(&mut self) -> Result<StaticAssert, CompileError> {
        // static_assert_declaration
        //     : STATIC_ASSERT '(' constant_expression ',' STRING_LITERAL ')' ';'
        //     | STATIC_ASSERT '(' constant_expression ')' ';'
        //     /* C23: message string is now optional */
        //     ;

        self.consume_and_assert_keyword(C23Keyword::StaticAssert)?;
        self.consume_opening_parenthesis()?;
        let constant_expression = self.parse_constant_expression()?;
        let message = if self.peek_and_equals_punctuator(0, Punctuator::Comma) {
            self.consume_comma()?;
            let message = self.consume_string()?;
            Some(message)
        } else {
            None
        };

        self.consume_closing_parenthesis()?;
        self.consume_semicolon()?;

        Ok(StaticAssert {
            expression: Box::new(constant_expression),
            message,
        })
    }

    fn parse_constant_expression(&mut self) -> Result<Expr, CompileError> {
        // constant_expression
        //     : conditional_expression    /* with constraints */
        //     ;
        self.parse_conditional_expression()
    }

    fn parse_declaration_specifiers(&mut self) -> Result<Vec<DeclarationSpecifier>, CompileError> {
        // declaration_specifiers: one or more of the following in any order:
        // - `storage_class_specifier`
        // - `type_specifier`
        // - `type_qualifier`
        // - `function_specifier `
        // - `alignment_specifier`

        let mut specifiers = Vec::new();

        while let Some(Token::Identifier(id)) = self.peek_token(0) {
            let id = id.clone();
            match id.as_str() {
                // ---- storage_class_specifier ----
                "typedef" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::StorageClass(
                        StorageClassSpecifier::Typedef,
                    ));
                }
                "extern" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::StorageClass(
                        StorageClassSpecifier::Extern,
                    ));
                }
                "static" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::StorageClass(
                        StorageClassSpecifier::Static,
                    ));
                }
                "thread_local" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::StorageClass(
                        StorageClassSpecifier::ThreadLocal,
                    ));
                }
                "auto" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::StorageClass(
                        StorageClassSpecifier::Auto,
                    ));
                }
                "register" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::StorageClass(
                        StorageClassSpecifier::Register,
                    ));
                }
                "constexpr" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::StorageClass(
                        StorageClassSpecifier::Constexpr,
                    ));
                }
                // ---- type_specifier ----
                "void" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Void));
                }
                "char" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Char));
                }
                "short" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Short));
                }
                "int" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Int));
                }
                "long" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Long));
                }
                "float" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Float));
                }
                "double" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Double));
                }
                "signed" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Signed));
                }
                "unsigned" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Unsigned));
                }
                "bool" | "_Bool" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Bool));
                }
                "_Complex" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Complex));
                }
                "_Imaginary" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Imaginary));
                }
                "_Decimal32" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Decimal32));
                }
                "_Decimal64" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Decimal64));
                }
                "_Decimal128" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Decimal128));
                }
                "_BitInt" => {
                    self.next_token();
                    self.consume_opening_parenthesis()?;
                    let n = self.parse_constant_expression()?;
                    self.consume_closing_parenthesis()?;
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::BitInt(Box::new(
                        n,
                    ))));
                }
                "typeof" => {
                    self.next_token();
                    let ts = self.parse_typeof_specifier(false)?;
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Typeof(ts)));
                }
                "typeof_unqual" => {
                    self.next_token();
                    let ts = self.parse_typeof_specifier(true)?;
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Typeof(ts)));
                }
                "_Atomic" => {
                    // `_Atomic '(' type_name ')'`  -> TypeSpecifier
                    // `_Atomic`                    -> TypeQualifier
                    if self.peek_and_equals_punctuator(1, Punctuator::ParenthesisOpen) {
                        self.next_token(); // consume '_Atomic'
                        self.consume_opening_parenthesis()?;
                        let tn = self.parse_type_name()?;
                        self.consume_closing_parenthesis()?;
                        specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Atomic(
                            Box::new(tn),
                        )));
                    } else {
                        self.next_token();
                        specifiers.push(DeclarationSpecifier::Qualifier(TypeQualifier::Atomic));
                    }
                }
                "struct" | "union" => {
                    let su = self.parse_struct_or_union_specifier()?;
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::StructOrUnion(su)));
                }
                "enum" => {
                    let es = self.parse_enum_specifier()?;
                    specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::Enum(es)));
                }
                // ---- type_qualifier ----
                "const" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Qualifier(TypeQualifier::Const));
                }
                "restrict" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Qualifier(TypeQualifier::Restrict));
                }
                "volatile" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Qualifier(TypeQualifier::Volatile));
                }
                // "_Atomic" is handled above because it can be both type specifier and qualifier
                // ---- function_specifier ----
                "inline" => {
                    self.next_token();
                    specifiers.push(DeclarationSpecifier::Function(FunctionSpecifier::Inline));
                }
                "_Noreturn" => {
                    // C23 deprecated function specifier
                    // todo::
                    // report error
                    unimplemented!()
                }
                // ---- alignment_specifier ----
                "alignas" => {
                    let align = self.parse_alignment_specifier()?;
                    specifiers.push(DeclarationSpecifier::Alignment(align));
                }
                other => {
                    // Could be a typedef-name – only accept it if it really is one
                    if self.exists_typedef_name(other) {
                        self.next_token();
                        specifiers.push(DeclarationSpecifier::Type(TypeSpecifier::TypedefName(id)));
                    } else {
                        // Not a declaration specifier; stop the loop
                        break;
                    }
                }
            }
        }

        if specifiers.is_empty() {
            return Err(
                self.build_error_with_last_location("Expected declaration specifier.".to_string())
            );
        }

        Ok(specifiers)
    }

    /// Parses `alignas '(' type_name ')'` or `alignas '(' constant_expression ')'`.
    fn parse_alignment_specifier(&mut self) -> Result<AlignmentSpecifier, CompileError> {
        // alignment_specifier
        //     : ALIGNAS '(' type_name ')'
        //     | ALIGNAS '(' constant_expression ')'
        //     ;

        self.consume_and_assert_keyword(C23Keyword::Alignas)?;
        self.consume_opening_parenthesis()?;

        // Disambiguate: type_name vs. expression
        let result = if self.peek_and_is_specifier_qualifier(0) {
            let type_name = self.parse_type_name()?;
            AlignmentSpecifier::Type(Box::new(type_name))
        } else {
            let expr = self.parse_constant_expression()?;
            AlignmentSpecifier::Expression(Box::new(expr))
        };

        self.consume_closing_parenthesis()?;
        Ok(result)
    }

    /// Parses the body of `typeof(...)` / `typeof_unqual(...)` after the keyword has been consumed.
    fn parse_typeof_specifier(&mut self, unqual: bool) -> Result<TypeofSpecifier, CompileError> {
        // C23: typeof(expression) / typeof(type_name)
        // typeof_unqual(expression) / typeof_unqual(type_name)
        //
        // typeof_specifier
        //     : TYPEOF '(' typeof_specifier_argument ')'
        //     | TYPEOF_UNQUAL '(' typeof_specifier_argument ')'
        //     ;

        // typeof_specifier_argument
        //     : expression
        //     | type_name
        //     ;

        self.consume_opening_parenthesis()?;

        let result = if self.peek_and_is_specifier_qualifier(0) {
            let type_name = self.parse_type_name()?;
            TypeofSpecifier::Type {
                unqual,
                type_name: Box::new(type_name),
            }
        } else {
            // todo::
            // expression or assignment_expression ?
            let expression = self.parse_assignment_expression()?;
            TypeofSpecifier::Expression {
                unqual,
                expression: Box::new(expression),
            }
        };

        self.consume_closing_parenthesis()?;
        Ok(result)
    }

    /// Parses `specifier_qualifier_list` — used inside struct/union bodies and type_name.
    fn parse_specifier_qualifier_list(&mut self) -> Result<Vec<SpecifierQualifier>, CompileError> {
        // specifier_qualifier_list
        //     : type_specifier specifier_qualifier_list
        //     | type_specifier
        //     | type_qualifier specifier_qualifier_list
        //     | type_qualifier
        //     | alignment_specifier specifier_qualifier_list    /* C23 */
        //     | alignment_specifier                /* C23 */
        //     | attribute_specifier_sequence specifier_qualifier_list    /* C23 */
        //     | attribute_specifier_sequence            /* C23 */
        //     ;

        let mut items = Vec::new();

        while let Some(token) = self.peek_token(0) {
            match token {
                Token::Identifier(id) => {
                    let id = id.clone();
                    match id.as_str() {
                        // ---- type_specifier ----
                        "void" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Void));
                        }
                        "char" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Char));
                        }
                        "short" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Short));
                        }
                        "int" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Int));
                        }
                        "long" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Long));
                        }
                        "float" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Float));
                        }
                        "double" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Double));
                        }
                        "signed" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Signed));
                        }
                        "unsigned" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Unsigned));
                        }
                        "bool" | "_Bool" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Bool));
                        }
                        "_Complex" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Complex));
                        }
                        "_Imaginary" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Imaginary));
                        }
                        "_Decimal32" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Decimal32));
                        }
                        "_Decimal64" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Decimal64));
                        }
                        "_Decimal128" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Decimal128));
                        }
                        "_BitInt" => {
                            self.next_token();
                            self.consume_opening_parenthesis()?;
                            let n = self.parse_constant_expression()?;
                            self.consume_closing_parenthesis()?;
                            items
                                .push(SpecifierQualifier::Type(TypeSpecifier::BitInt(Box::new(n))));
                        }
                        "typeof" => {
                            self.next_token();
                            let ts = self.parse_typeof_specifier(false)?;
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Typeof(ts)));
                        }
                        "typeof_unqual" => {
                            self.next_token();
                            let ts = self.parse_typeof_specifier(true)?;
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Typeof(ts)));
                        }
                        "_Atomic" => {
                            if self.peek_and_equals_punctuator(1, Punctuator::ParenthesisOpen) {
                                self.next_token();
                                self.consume_opening_parenthesis()?;
                                let tn = self.parse_type_name()?;
                                self.consume_closing_parenthesis()?;
                                items.push(SpecifierQualifier::Type(TypeSpecifier::Atomic(
                                    Box::new(tn),
                                )));
                            } else {
                                self.next_token();
                                items.push(SpecifierQualifier::Qualifier(TypeQualifier::Atomic));
                            }
                        }
                        "struct" | "union" => {
                            let su = self.parse_struct_or_union_specifier()?;
                            items.push(SpecifierQualifier::Type(TypeSpecifier::StructOrUnion(su)));
                        }
                        "enum" => {
                            let es = self.parse_enum_specifier()?;
                            items.push(SpecifierQualifier::Type(TypeSpecifier::Enum(es)));
                        }
                        // ---- type_qualifier ----
                        "const" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Qualifier(TypeQualifier::Const));
                        }
                        "restrict" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Qualifier(TypeQualifier::Restrict));
                        }
                        "volatile" => {
                            self.next_token();
                            items.push(SpecifierQualifier::Qualifier(TypeQualifier::Volatile));
                        }
                        // "_Atomic" is handled above because it can be both type specifier and qualifier
                        // ---- alignment_specifier ----
                        "alignas" => {
                            let align = self.parse_alignment_specifier()?;
                            items.push(SpecifierQualifier::Alignment(align));
                        }
                        other => {
                            if self.exists_typedef_name(other) {
                                self.next_token();
                                items
                                    .push(SpecifierQualifier::Type(TypeSpecifier::TypedefName(id)));
                            } else {
                                break;
                            }
                        }
                    }
                }
                Token::Punctuator(Punctuator::AttributeOpen) => {
                    let attrs = self.parse_attribute_specifier_sequence()?;
                    items.push(SpecifierQualifier::Attribute(attrs));
                }
                _ => {
                    break;
                }
            }
        }

        if items.is_empty() {
            return Err(self.build_error_with_last_location(
                "Expected type specifier or qualifier.".to_string(),
            ));
        }

        Ok(items)
    }

    fn parse_struct_or_union_specifier(&mut self) -> Result<StructOrUnionSpecifier, CompileError> {
        // struct_or_union_specifier
        //     : struct_or_union attribute_specifier_seq_opt '{' struct_declaration_list '}'
        //     | struct_or_union attribute_specifier_seq_opt IDENTIFIER '{' struct_declaration_list '}'
        //     | struct_or_union attribute_specifier_seq_opt IDENTIFIER
        //     ;

        // struct_or_union
        //     : STRUCT
        //     | UNION
        //     ;

        // struct_declaration_list
        //     : struct_declaration
        //     | struct_declaration_list struct_declaration
        //     ;

        let kind = match self.next_token() {
            Some(Token::Identifier(kw)) if kw == "struct" => StructOrUnion::Struct,
            Some(Token::Identifier(kw)) if kw == "union" => StructOrUnion::Union,
            Some(_) => {
                return Err(self.build_error_with_last_location(
                    "Expected \"struct\" or \"union\".".to_string(),
                ));
            }
            None => {
                return Err(self
                    .build_error_unexpected_eof("Expected \"struct\" or \"union\".".to_string()));
            }
        };

        // Optional C23 attribute_specifier_sequence
        let attributes = if self.peek_and_equals_punctuator(0, Punctuator::AttributeOpen) {
            self.parse_attribute_specifier_sequence()?
        } else {
            vec![]
        };

        // Optional tag name
        let name = if self.peek_and_is_nonkeyword_identifier(0) {
            let id = self.consume_identifier()?;
            Some(id)
        } else {
            None
        };

        // Optional body
        let body = if self.peek_and_equals_punctuator(0, Punctuator::BraceOpen) {
            self.consume_opening_brace()?;
            let mut decls = Vec::new();
            while !self.is_eof() {
                if self.peek_and_equals_punctuator(0, Punctuator::BraceClose) {
                    break;
                }
                decls.push(self.parse_struct_declaration()?);
            }
            self.consume_closing_brace()?;
            Some(decls)
        } else {
            None
        };

        if name.is_none() && body.is_none() {
            return Err(self.build_error_with_last_location(
                "struct/union must have either a tag name or a body.".to_string(),
            ));
        }

        Ok(StructOrUnionSpecifier {
            kind,
            attributes,
            name,
            body,
        })
    }

    fn parse_struct_declaration(&mut self) -> Result<StructDeclaration, CompileError> {
        // /* C23 Annex A 6.7.3.1: optional leading attribute-specifier-sequence. */
        //
        // struct_declaration
        //     : attribute_specifier_seq_opt specifier_qualifier_list ';'
        //                 /* anonymous struct/union or plain field; C23: optional attr */
        //     | attribute_specifier_seq_opt specifier_qualifier_list struct_declarator_list ';'
        //     | static_assert_declaration
        //     ;

        // struct_declarator_list
        //     : struct_declarator
        //     | struct_declarator_list ',' struct_declarator
        //     ;

        // struct_declarator
        //     : ':' constant_expression
        //     | declarator ':' constant_expression
        //     | declarator
        //     ;

        if self.peek_and_equals_keyword(0, C23Keyword::StaticAssert) {
            let sa = self.parse_static_assert_declaration()?;
            return Ok(StructDeclaration::StaticAssert(sa));
        }

        // Optional leading attributes
        let attributes = if self.peek_and_equals_punctuator(0, Punctuator::AttributeOpen) {
            self.parse_attribute_specifier_sequence()?
        } else {
            vec![]
        };

        let specifiers = self.parse_specifier_qualifier_list()?;

        // Optional struct_declarator_list
        let declarators = if self.peek_and_equals_punctuator(0, Punctuator::Semicolon) {
            vec![]
        } else {
            let mut list = vec![self.parse_struct_declarator()?];
            while self.peek_and_equals_punctuator(0, Punctuator::Comma) {
                self.consume_comma()?;
                list.push(self.parse_struct_declarator()?);
            }
            list
        };

        self.consume_semicolon()?;

        Ok(StructDeclaration::Field {
            attributes,
            specifiers,
            declarators,
        })
    }

    fn parse_struct_declarator(&mut self) -> Result<StructDeclarator, CompileError> {
        // struct_declarator
        //     : ':' constant_expression
        //     | declarator ':' constant_expression /* bit-field with declarator (e.g. int x:3) */
        //     | declarator
        //     ;

        // If first token is ':' -> bit-field with no declarator
        if self.peek_and_equals_punctuator(0, Punctuator::Colon) {
            self.consume_colon()?;
            let width = self.parse_constant_expression()?;
            return Ok(StructDeclarator::BitField {
                declarator: None,
                width: Box::new(width),
            });
        }

        let declarator = self.parse_declarator()?;

        if self.peek_and_equals_punctuator(0, Punctuator::Colon) {
            self.consume_colon()?;
            let width = self.parse_constant_expression()?;
            Ok(StructDeclarator::BitField {
                declarator: Some(declarator),
                width: Box::new(width),
            })
        } else {
            Ok(StructDeclarator::Plain(declarator))
        }
    }

    fn parse_enum_specifier(&mut self) -> Result<EnumSpecifier, CompileError> {
        // enum_specifier
        //     /* C11 forms (no fixed underlying type) */
        //     : ENUM attribute_specifier_seq_opt '{' enumerator_list '}'
        //     | ENUM attribute_specifier_seq_opt '{' enumerator_list ',' '}'
        //     | ENUM attribute_specifier_seq_opt IDENTIFIER '{' enumerator_list '}'
        //     | ENUM attribute_specifier_seq_opt IDENTIFIER '{' enumerator_list ',' '}'
        //     | ENUM attribute_specifier_seq_opt IDENTIFIER
        //     /* C23: fixed underlying type — enum E : int { ... } */
        //     | ENUM attribute_specifier_seq_opt ':' specifier_qualifier_list '{' enumerator_list '}'
        //     | ENUM attribute_specifier_seq_opt ':' specifier_qualifier_list '{' enumerator_list ',' '}'
        //     | ENUM attribute_specifier_seq_opt IDENTIFIER ':' specifier_qualifier_list '{' enumerator_list '}'
        //     | ENUM attribute_specifier_seq_opt IDENTIFIER ':' specifier_qualifier_list '{' enumerator_list ',' '}'
        //     | ENUM attribute_specifier_seq_opt IDENTIFIER ':' specifier_qualifier_list
        //     ;
        //
        // enumerator_list
        //     : enumerator
        //     | enumerator_list ',' enumerator
        //     ;

        self.consume_and_assert_keyword(C23Keyword::Enum)?;

        let attributes = if self.peek_and_equals_punctuator(0, Punctuator::AttributeOpen) {
            self.parse_attribute_specifier_sequence()?
        } else {
            vec![]
        };

        let name = if self.peek_and_is_nonkeyword_identifier(0) {
            Some(self.consume_identifier()?)
        } else {
            None
        };

        // C23: fixed underlying type  `enum E : int { ... }`
        let underlying_type = if self.peek_and_equals_punctuator(0, Punctuator::Colon) {
            self.consume_colon()?;
            Some(self.parse_specifier_qualifier_list()?)
        } else {
            None
        };

        // Optional enumerator list
        let variants = if self.peek_and_equals_punctuator(0, Punctuator::BraceOpen) {
            self.consume_opening_brace()?;
            let mut list = Vec::new();
            if !self.peek_and_equals_punctuator(0, Punctuator::BraceClose) {
                while !self.is_eof() {
                    list.push(self.parse_enumerator()?);
                    if self.peek_and_equals_punctuator(0, Punctuator::Comma) {
                        self.consume_comma()?;
                    } else {
                        break;
                    }
                }
            }

            self.consume_closing_brace()?;
            Some(list)
        } else {
            None
        };

        Ok(EnumSpecifier {
            attributes,
            name,
            underlying_type,
            variants,
        })
    }

    fn parse_enumerator(&mut self) -> Result<Enumerator, CompileError> {
        // enumerator    /* identifiers must be flagged as ENUMERATION_CONSTANT */
        //     : enumeration_constant attribute_specifier_seq_opt '=' constant_expression
        //                 /* C23: attribute on enumerator */
        //     | enumeration_constant attribute_specifier_seq_opt
        //     ;

        let name = self.consume_nonkeyword_identifier()?;

        let attributes = if self.peek_and_equals_punctuator(0, Punctuator::AttributeOpen) {
            self.parse_attribute_specifier_sequence()?
        } else {
            vec![]
        };

        let value = if self.peek_and_equals_punctuator(0, Punctuator::Assign) {
            self.next_token(); // consume '='
            Some(Box::new(self.parse_constant_expression()?))
        } else {
            None
        };

        // Register the enumerator name so the parser can recognise it later
        self.add_enum_constant(name.clone());

        Ok(Enumerator {
            name,
            attributes,
            value,
        })
    }

    fn parse_init_declarator_list(&mut self) -> Result<Vec<InitDeclarator>, CompileError> {
        // init_declarator_list
        //     : init_declarator
        //     | init_declarator_list ',' init_declarator
        //     ;
        //
        // init_declarator
        //     : declarator
        //     | declarator '=' initializer
        //     ;

        let mut list = Vec::new();

        // Parse the first init_declarator
        let declarator = self.parse_declarator()?;
        if self.peek_and_equals_punctuator(0, Punctuator::Assign) {
            self.next_token(); // consume '='
            let initializer = self.parse_initializer()?;
            list.push(InitDeclarator::Init(declarator, initializer));
        } else {
            list.push(InitDeclarator::Plain(declarator));
        }

        // Parse subsequent entries
        while self.peek_and_equals_punctuator(0, Punctuator::Comma) {
            self.consume_comma()?;
            let declarator = self.parse_declarator()?;
            if self.peek_and_equals_punctuator(0, Punctuator::Assign) {
                self.next_token();
                let initializer = self.parse_initializer()?;
                list.push(InitDeclarator::Init(declarator, initializer));
            } else {
                list.push(InitDeclarator::Plain(declarator));
            }
        }

        Ok(list)
    }

    fn parse_declarator(&mut self) -> Result<Declarator, CompileError> {
        // declarator
        //     : pointer? direct_declarator
        //     ;

        let pointer = self.parse_pointer()?;
        let direct = self.parse_direct_declarator()?;
        Ok(Declarator { pointer, direct })
    }

    fn parse_pointer(&mut self) -> Result<Option<Pointer>, CompileError> {
        // pointer
        //     : '*' type_qualifier_list?
        //     | '*' type_qualifier_list? pointer
        //     ;

        if !self.peek_and_equals_punctuator(0, Punctuator::Multiply) {
            return Ok(None);
        }

        self.next_token(); // consume '*'

        let mut qualifiers = Vec::new();

        while let Some(Token::Identifier(id)) = self.peek_token(0) {
            match id.as_str() {
                "const" => {
                    self.next_token();
                    qualifiers.push(TypeQualifier::Const);
                }
                "restrict" => {
                    self.next_token();
                    qualifiers.push(TypeQualifier::Restrict);
                }
                "volatile" => {
                    self.next_token();
                    qualifiers.push(TypeQualifier::Volatile);
                }
                "_Atomic" => {
                    self.next_token();
                    qualifiers.push(TypeQualifier::Atomic);
                }
                _ => break,
            }
        }

        let inner = self.parse_pointer()?;

        Ok(Some(Pointer {
            qualifiers,
            inner: inner.map(Box::new),
        }))
    }

    fn parse_direct_declarator(&mut self) -> Result<DirectDeclarator, CompileError> {
        // direct_declarator
        //     : IDENTIFIER
        //     | '(' declarator ')'
        //     | direct_declarator '[' ']'
        //     | direct_declarator '[' '*' ']'
        //     | direct_declarator '[' STATIC type_qualifier_list assignment_expression ']'
        //     | direct_declarator '[' STATIC assignment_expression ']'
        //     | direct_declarator '[' type_qualifier_list '*' ']'
        //     | direct_declarator '[' type_qualifier_list STATIC assignment_expression ']'
        //     | direct_declarator '[' type_qualifier_list assignment_expression ']'
        //     | direct_declarator '[' type_qualifier_list ']'
        //     | direct_declarator '[' assignment_expression ']'
        //     | direct_declarator '(' parameter_type_list ')'
        //     | direct_declarator '(' ')'
        //     /* C23: K&R-style identifier_list removed — old-style declarations are no longer valid */
        //     ;

        // Base: identifier or parenthesized declarator
        let mut base = if self.peek_and_equals_punctuator(0, Punctuator::ParenthesisOpen) {
            self.consume_opening_parenthesis()?;
            let inner = self.parse_declarator()?;
            self.consume_closing_parenthesis()?;
            DirectDeclarator::Parenthesized(Box::new(inner))
        } else {
            let id = self.consume_nonkeyword_identifier()?;
            DirectDeclarator::Identifier(id)
        };

        // Postfix: [] or ()

        while let Some(Token::Punctuator(p)) = self.peek_token(0) {
            match p {
                Punctuator::BracketOpen => {
                    self.consume_opening_bracket()?;
                    let size = self.parse_array_size()?;
                    self.consume_closing_bracket()?;
                    base = DirectDeclarator::Array(Box::new(base), size);
                }
                Punctuator::ParenthesisOpen => {
                    self.consume_opening_parenthesis()?;
                    let params = if self.peek_and_equals_punctuator(0, Punctuator::ParenthesisClose)
                    {
                        FunctionParams::Empty
                    } else {
                        FunctionParams::TypeList(self.parse_parameter_type_list()?)
                    };
                    self.consume_closing_parenthesis()?;
                    base = DirectDeclarator::Function(Box::new(base), params);
                }
                _ => {
                    break;
                }
            }
        }

        Ok(base)
    }

    fn parse_array_size(&mut self) -> Result<ArraySize, CompileError> {
        // Handles all forms inside `[...]` — the `[` has already been consumed.
        //
        // - `[]`
        // - `[*]`
        // - `[static qualifier_list? expr]`
        // - `[qualifier_list static expr]`
        // - `[qualifier_list expr?]`
        // - `[expr]`

        // `[]` -> Unknown
        if self.peek_and_equals_punctuator(0, Punctuator::BracketClose) {
            return Ok(ArraySize::Unknown);
        }

        // `[*]` -> Vla (variable-length array, prototype only)
        if self.peek_and_equals_punctuator(0, Punctuator::Multiply) {
            self.next_token();
            return Ok(ArraySize::Vla);
        }

        // `[static qualifier_list? expr]`
        if self.peek_and_equals_keyword(0, C23Keyword::Static) {
            self.next_token(); // consume 'static'
            let mut qualifiers = Vec::new();
            while self.peek_and_is_type_qualifier(0) {
                qualifiers.push(self.parse_single_type_qualifier()?);
            }
            let size = self.parse_assignment_expression()?;
            return Ok(ArraySize::Static {
                qualifiers,
                size: Box::new(size),
            });
        }

        // Collect leading qualifiers
        let mut qualifiers = Vec::new();
        while self.peek_and_is_type_qualifier(0) {
            qualifiers.push(self.parse_single_type_qualifier()?);
        }

        if !qualifiers.is_empty() {
            // `[qualifier_list static expr]`
            if self.peek_and_equals_keyword(0, C23Keyword::Static) {
                self.next_token(); // consume 'static'
                let size = self.parse_assignment_expression()?;
                return Ok(ArraySize::Static {
                    qualifiers,
                    size: Box::new(size),
                });
            }
            // `[qualifier_list expr?]`
            let size = if self.peek_and_equals_punctuator(0, Punctuator::BracketClose) {
                None
            } else {
                Some(Box::new(self.parse_assignment_expression()?))
            };
            return Ok(ArraySize::Qualified { qualifiers, size });
        }

        // Plain expression
        let size = self.parse_assignment_expression()?;
        Ok(ArraySize::Expression(Box::new(size)))
    }

    fn parse_single_type_qualifier(&mut self) -> Result<TypeQualifier, CompileError> {
        match self.next_token() {
            Some(Token::Identifier(id)) => match id.as_str() {
                "const" => Ok(TypeQualifier::Const),
                "restrict" => Ok(TypeQualifier::Restrict),
                "volatile" => Ok(TypeQualifier::Volatile),
                "_Atomic" => Ok(TypeQualifier::Atomic),
                _ => Err(self.build_error_with_last_location(format!(
                    "Expected type qualifier, got '{}'.",
                    id
                ))),
            },
            Some(_) => {
                Err(self.build_error_with_last_location("Expected type qualifier.".to_string()))
            }
            None => Err(self.build_error_unexpected_eof("Expected type qualifier.".to_string())),
        }
    }

    fn parse_parameter_type_list(&mut self) -> Result<ParameterTypeList, CompileError> {
        // parameter_type_list
        //     : parameter_list ',' ELLIPSIS
        //     | parameter_list
        //     | ELLIPSIS    /* C23: variadic function with no named parameter: void f(...) */
        //     ;

        // C23 lone-ellipsis `f(...)`
        if self.peek_and_equals_punctuator(0, Punctuator::Ellipsis) {
            self.next_token();
            return Ok(ParameterTypeList {
                params: vec![],
                variadic: true,
            });
        }

        let mut params = vec![self.parse_parameter_declaration()?];

        while self.peek_and_equals_punctuator(0, Punctuator::Comma) {
            self.consume_comma()?;
            if self.peek_and_equals_punctuator(0, Punctuator::Ellipsis) {
                self.next_token();
                return Ok(ParameterTypeList {
                    params,
                    variadic: true,
                });
            }
            params.push(self.parse_parameter_declaration()?);
        }

        Ok(ParameterTypeList {
            params,
            variadic: false,
        })
    }

    fn parse_parameter_declaration(&mut self) -> Result<ParameterDeclaration, CompileError> {
        // parameter_declaration
        //     : declaration_specifiers declarator
        //     | declaration_specifiers abstract_declarator
        //     | declaration_specifiers
        //     ;

        let specifiers = self.parse_declaration_specifiers()?;

        // Use lookahead to decide named vs. abstract
        if self.peek_and_is_named_declarator(0) {
            let declarator = self.parse_declarator()?;
            Ok(ParameterDeclaration::Named {
                specifiers,
                declarator,
            })
        } else {
            let declarator = self.parse_abstract_declarator_opt()?;
            Ok(ParameterDeclaration::Abstract {
                specifiers,
                declarator,
            })
        }
    }

    fn parse_type_name(&mut self) -> Result<TypeName, CompileError> {
        // type_name
        //     : specifier_qualifier_list abstract_declarator
        //     | specifier_qualifier_list
        //     ;

        let specifiers = self.parse_specifier_qualifier_list()?;
        let declarator = self.parse_abstract_declarator_opt()?;
        Ok(TypeName {
            specifiers,
            declarator,
        })
    }

    fn parse_abstract_declarator_opt(
        &mut self,
    ) -> Result<Option<AbstractDeclarator>, CompileError> {
        // abstract_declarator
        //     : pointer direct_abstract_declarator
        //     | pointer
        //     | direct_abstract_declarator
        //     ;

        let has_pointer = self.peek_and_equals_punctuator(0, Punctuator::Multiply);
        let has_direct = self.peek_and_equals_punctuator(0, Punctuator::ParenthesisOpen)
            || self.peek_and_equals_punctuator(0, Punctuator::BracketOpen);

        if !has_pointer && !has_direct {
            return Ok(None);
        }

        let pointer = self.parse_pointer()?;
        let direct = self.parse_direct_abstract_declarator_opt()?;

        Ok(Some(AbstractDeclarator { pointer, direct }))
    }

    fn parse_direct_abstract_declarator_opt(
        &mut self,
    ) -> Result<Option<DirectAbstractDeclarator>, CompileError> {
        // direct_abstract_declarator
        //     : '(' abstract_declarator ')'
        //     | '[' ']'
        //     | '[' '*' ']'
        //     | '[' STATIC type_qualifier_list assignment_expression ']'
        //     | '[' STATIC assignment_expression ']'
        //     | '[' type_qualifier_list STATIC assignment_expression ']'
        //     | '[' type_qualifier_list assignment_expression ']'
        //     | '[' type_qualifier_list ']'
        //     | '[' assignment_expression ']'
        //     | direct_abstract_declarator '[' ']'
        //     | direct_abstract_declarator '[' '*' ']'
        //     | direct_abstract_declarator '[' STATIC type_qualifier_list assignment_expression ']'
        //     | direct_abstract_declarator '[' STATIC assignment_expression ']'
        //     | direct_abstract_declarator '[' type_qualifier_list assignment_expression ']'
        //     | direct_abstract_declarator '[' type_qualifier_list STATIC assignment_expression ']'
        //     | direct_abstract_declarator '[' type_qualifier_list ']'
        //     | direct_abstract_declarator '[' assignment_expression ']'
        //     | '(' ')'
        //     | '(' parameter_type_list ')'
        //     | direct_abstract_declarator '(' ')'
        //     | direct_abstract_declarator '(' parameter_type_list ')'
        //     ;

        // Base: optional '(' abstract_declarator ')'
        // Only enter this branch if the content is NOT a type-name (which would indicate
        // a function-parameter type list instead of a parenthesized abstract declarator).
        let mut base: Option<DirectAbstractDeclarator> = if self
            .peek_and_equals_punctuator(0, Punctuator::ParenthesisOpen)
            && !self.peek_and_is_specifier_qualifier(1)
            && !self.peek_and_equals_punctuator(1, Punctuator::ParenthesisClose)
        {
            self.consume_opening_parenthesis()?;
            let inner = self.parse_abstract_declarator_opt()?;
            self.consume_closing_parenthesis()?;
            inner.map(|a| DirectAbstractDeclarator::Parenthesized(Box::new(a)))
        } else {
            None
        };

        // Postfix: [] or ()
        while let Some(Token::Punctuator(p)) = self.peek_token(0) {
            match p {
                Punctuator::BracketOpen => {
                    self.consume_opening_bracket()?;
                    let size = self.parse_array_size()?;
                    self.consume_closing_bracket()?;
                    base = Some(DirectAbstractDeclarator::Array(base.map(Box::new), size));
                }
                Punctuator::ParenthesisOpen => {
                    self.consume_opening_parenthesis()?;
                    let params = if self.peek_and_equals_punctuator(0, Punctuator::ParenthesisClose)
                    {
                        None
                    } else {
                        Some(self.parse_parameter_type_list()?)
                    };
                    self.consume_closing_parenthesis()?;
                    base = Some(DirectAbstractDeclarator::Function(
                        base.map(Box::new),
                        params,
                    ));
                }
                _ => {
                    break;
                }
            }
        }

        Ok(base)
    }

    fn parse_initializer(&mut self) -> Result<Initializer, CompileError> {
        // initializer
        //     : '{' initializer_list '}'
        //     | '{' initializer_list ',' '}'
        //     | '{' '}'        /* C23: empty initializer (zero-initializes the object) */
        //     | assignment_expression
        //     ;

        let initializer = if self.peek_and_equals_punctuator(0, Punctuator::BraceOpen) {
            self.consume_opening_brace()?;

            if self.peek_and_equals_punctuator(0, Punctuator::BraceClose) {
                // C23 empty initializer `= {}`
                self.consume_closing_brace()?;
                return Ok(Initializer::Empty);
            }

            let items = self.parse_initializer_list()?;

            // Trailing comma is allowed
            if self.peek_and_equals_punctuator(0, Punctuator::Comma) {
                self.consume_comma()?;
            }

            self.consume_closing_brace()?;
            Initializer::List(items)
        } else {
            let expr = self.parse_assignment_expression()?;
            Initializer::Expression(Box::new(expr))
        };

        Ok(initializer)
    }

    fn parse_initializer_list(&mut self) -> Result<Vec<InitializerItem>, CompileError> {
        // initializer_list
        //     : designation initializer
        //     | initializer
        //     | initializer_list ',' designation initializer
        //     | initializer_list ',' initializer
        //     ;

        let mut items = Vec::new();

        while !self.is_eof() {
            // Check for designation: `designator_list '='`
            let designation = self.parse_designation()?;
            let initializer = self.parse_initializer()?;
            items.push(InitializerItem {
                designation,
                initializer: Box::new(initializer),
            });

            if self.peek_and_equals_punctuator(0, Punctuator::Comma) {
                if self.peek_and_equals_punctuator(1, Punctuator::BraceClose) {
                    // Reach closing brace, trailing comma is allowed
                    break;
                } else {
                    // More items to come
                    self.consume_comma()?;
                }
            } else {
                break;
            }
        }

        Ok(items)
    }

    fn parse_designation(&mut self) -> Result<Vec<Designator>, CompileError> {
        // designation
        //     : designator_list '='
        //     ;

        // designator_list
        //     : designator
        //     | designator_list designator
        //     ;

        // designator
        //     : '[' constant_expression ']'
        //     | '.' IDENTIFIER
        //     ;

        let mut designators = Vec::new();

        while let Some(Token::Punctuator(p)) = self.peek_token(0) {
            match p {
                Punctuator::BracketOpen => {
                    self.consume_opening_bracket()?;
                    let expr = self.parse_constant_expression()?;
                    self.consume_closing_bracket()?;
                    designators.push(Designator::Index(Box::new(expr)));
                }
                Punctuator::Dot => {
                    self.next_token(); // consume '.'
                    let id = self.consume_nonkeyword_identifier()?;
                    designators.push(Designator::Member(id));
                }
                _ => {
                    break;
                }
            }
        }

        // If there were designators, consume the '='
        if !designators.is_empty() {
            self.consume_and_assert(&Token::Punctuator(Punctuator::Assign), "'='")?;
        }

        Ok(designators)
    }

    fn parse_compound_statement(&mut self) -> Result<Vec<BlockItem>, CompileError> {
        // compound_statement
        //     : '{' '}'
        //     | '{' block_item_list '}'
        //     ;
        let mut block_items = vec![];

        self.enter_scope(); // new block scope

        self.consume_opening_brace()?;
        while !self.is_eof() {
            if self.peek_and_equals_punctuator(0, Punctuator::BraceClose) {
                break;
            }
            block_items.push(self.parse_block_item()?);
        }
        self.consume_closing_brace()?;

        self.leave_scope(); // exit block scope
        Ok(block_items)
    }

    fn parse_block_item(&mut self) -> Result<BlockItem, CompileError> {
        // block_item
        //     : declaration
        //     | statement
        //     | label        /* C23: standalone label before a declaration or end of block */
        //     ;
        //
        // Disambiguation heuristic:
        //   1. `static_assert` keyword -> declaration
        //   2. `[[...]]` -> parse attributes, then dispatch on what follows
        //   3. `case` / `default` keyword -> label (possibly labeled_statement)
        //   4. non-keyword identifier followed by `:` -> label (possibly labeled_statement)
        //   5. declaration specifier / typedef name -> declaration
        //   6. everything else -> statement

        // 1. Static assert
        if self.peek_and_equals_keyword(0, C23Keyword::StaticAssert) {
            let pos = self.peek_file_position(0);
            let sa = self.parse_static_assert_declaration()?;
            return Ok(BlockItem::Declaration(Spanned::new(
                Declaration::StaticAssert(sa),
                pos,
            )));
        }

        // 2. Optional attributes
        let attributes = if self.peek_and_equals_punctuator(0, Punctuator::AttributeOpen) {
            self.parse_attribute_specifier_sequence()?
        } else {
            vec![]
        };

        // After optional attributes, dispatch
        if !attributes.is_empty() {
            // `[[...]] ;` -> standalone attribute declaration
            if self.peek_and_equals_punctuator(0, Punctuator::Semicolon) {
                let pos = self.peek_file_position(0);
                self.consume_semicolon()?;
                return Ok(BlockItem::Declaration(Spanned::new(
                    Declaration::Attribute(attributes),
                    pos,
                )));
            }

            // `[[...]] case ...` / `[[...]] default ...` / `[[...]] ident ':'`  -> label
            if self.peek_and_equals_keyword(0, C23Keyword::Case)
                || self.peek_and_equals_keyword(0, C23Keyword::Default)
                || (self.peek_and_is_nonkeyword_identifier(0)
                    && self.peek_and_equals_punctuator(1, Punctuator::Colon))
            {
                let label = self.parse_label(attributes)?;
                return self.dispatch_label_block_item(label);
            }

            // Otherwise: attributes on a declaration
            if self.peek_and_is_declaration_specifier(0) {
                let pos = self.peek_file_position(0);
                let decl = self.parse_declaration_with_attributes(attributes)?;
                return Ok(BlockItem::Declaration(Spanned::new(decl, pos)));
            }

            return Err(self.build_error_with_last_location(
                "Unexpected token after attribute specifier in block item.".to_string(),
            ));
        }

        // 3 and 4. Labels: case / default / identifier ':'
        if self.peek_and_equals_keyword(0, C23Keyword::Case)
            || self.peek_and_equals_keyword(0, C23Keyword::Default)
            || (self.peek_and_is_nonkeyword_identifier(0)
                && self.peek_and_equals_punctuator(1, Punctuator::Colon))
        {
            let label = self.parse_label(vec![])?;
            return self.dispatch_label_block_item(label);
        }

        // 5. Declaration
        if self.peek_and_is_declaration_specifier(0) {
            let pos = self.peek_file_position(0);
            let decl = self.parse_declaration()?;
            // Declaration::Var does not consume ';'; handle here
            if let Declaration::Var { .. } = &decl {
                self.consume_semicolon()?;
            }
            return Ok(BlockItem::Declaration(Spanned::new(decl, pos)));
        }

        // 6. Statement
        let statement = self.parse_statement()?;
        Ok(BlockItem::Statement(statement))
    }

    /// Decides whether a parsed `Label` becomes a standalone `BlockItem::Label` or a
    /// `BlockItem::Statement(Statement::Labeled(...))` by checking the token that follows.
    fn dispatch_label_block_item(&mut self, label: Label) -> Result<BlockItem, CompileError> {
        // C23: a label can be the last item in a block (before `}`) or appear immediately
        // before a declaration. In both cases it is a standalone label, not a labeled statement.
        let next_is_statement_start = !self.is_eof()
            && !self.peek_and_equals_punctuator(0, Punctuator::BraceClose)
            && !self.peek_and_is_declaration_specifier(0)
            && !self.peek_and_equals_keyword(0, C23Keyword::StaticAssert);

        if next_is_statement_start {
            let statement = self.parse_statement()?;
            let pos = statement.pos;
            Ok(BlockItem::Statement(Spanned::new(
                Statement::Labeled(LabeledStatement {
                    label,
                    statement: Box::new(statement),
                }),
                pos,
            )))
        } else {
            Ok(BlockItem::Label(label))
        }
    }

    /// Called when we know we have a leading attribute_specifier_sequence and a declaration
    /// follows. Re-uses parse_declaration machinery but injects the already-parsed attributes.
    fn parse_declaration_with_attributes(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> Result<Declaration, CompileError> {
        let declaration_specifiers = self.parse_declaration_specifiers()?;
        let init_declarators = self.parse_init_declarator_list()?;

        let is_typedef = declaration_specifiers.iter().any(|s| {
            matches!(
                s,
                DeclarationSpecifier::StorageClass(StorageClassSpecifier::Typedef)
            )
        });

        if is_typedef {
            for id in &init_declarators {
                let d = match id {
                    InitDeclarator::Plain(d) => d,
                    InitDeclarator::Init(d, _) => d,
                };
                if let Some(name) = extract_declarator_name(&d.direct) {
                    self.add_typename(name);
                }
            }
        }

        self.consume_semicolon()?;

        Ok(Declaration::Var {
            attributes,
            declaration_specifiers,
            init_declarators,
        })
    }

    fn parse_statement(&mut self) -> Result<Stmt, CompileError> {
        // statement
        //     : labeled_statement
        //     | compound_statement
        //     | expression_statement
        //     | selection_statement
        //     | iteration_statement
        //     | jump_statement
        //     ;

        let pos = self.peek_file_position(0);

        let statement = match self.peek_token(0) {
            Some(Token::Punctuator(Punctuator::BraceOpen)) => {
                // Compound statement `{...}`
                let items = self.parse_compound_statement()?;
                Statement::Compound(items)
            }
            Some(Token::Identifier(id)) if id == "if" || id == "switch" => {
                // Selection: if / switch
                let sel = self.parse_selection_statement()?;
                Statement::Selection(Box::new(sel))
            }
            Some(Token::Identifier(id)) if id == "while" || id == "do" || id == "for" => {
                // Iteration: while / do / for
                let iter = self.parse_iteration_statement()?;
                Statement::Iteration(Box::new(iter))
            }
            Some(Token::Identifier(id))
                if id == "goto" || id == "continue" || id == "break" || id == "return" =>
            {
                // Jump: goto / continue / break / return
                let jump = self.parse_jump_statement()?;
                Statement::Jump(jump)
            }
            Some(Token::Identifier(id)) if id == "case" || id == "default" => {
                // Labeled statement  (case / default / identifier ':')
                let label = self.parse_label(vec![])?;
                let statement = self.parse_statement()?;
                Statement::Labeled(LabeledStatement {
                    label,
                    statement: Box::new(statement),
                })
            }
            Some(_)
                if (self.peek_and_is_nonkeyword_identifier(0)
                    && self.peek_and_equals_punctuator(1, Punctuator::Colon)) =>
            {
                // Labeled statement  (case / default / identifier ':')
                let label = self.parse_label(vec![])?;
                let statement = self.parse_statement()?;
                Statement::Labeled(LabeledStatement {
                    label,
                    statement: Box::new(statement),
                })
            }
            _ => {
                // Expression statement (including the empty `;`)
                let expr_opt = self.parse_expression_statement()?;
                Statement::Expression(expr_opt)
            }
        };

        Ok(Spanned::new(statement, pos))
    }

    fn parse_label(&mut self, attributes: Vec<Attribute>) -> Result<Label, CompileError> {
        // label
        //     : attribute_specifier_seq_opt IDENTIFIER ':'
        //     | attribute_specifier_seq_opt CASE constant_expression ':'
        //     | attribute_specifier_seq_opt DEFAULT ':'
        //     ;

        let kind = if self.peek_and_equals_keyword(0, C23Keyword::Case) {
            self.next_token(); // consume 'case'
            let expr = self.parse_constant_expression()?;
            self.consume_colon()?;
            LabelKind::Case(Box::new(expr))
        } else if self.peek_and_equals_keyword(0, C23Keyword::Default) {
            self.next_token(); // consume 'default'
            self.consume_colon()?;
            LabelKind::Default
        } else {
            let name = self.consume_nonkeyword_identifier()?;
            self.consume_colon()?;
            LabelKind::Named(name)
        };

        Ok(Label { attributes, kind })
    }

    fn parse_jump_statement(&mut self) -> Result<JumpStatement, CompileError> {
        // jump_statement
        //     : GOTO IDENTIFIER ';'
        //     | CONTINUE ';'
        //     | BREAK ';'
        //     | RETURN ';'
        //     | RETURN expression ';'
        //     ;

        match self.peek_token(0) {
            Some(Token::Identifier(id)) => {
                match id.as_str() {
                    "goto" => {
                        self.next_token();
                        let target = self.consume_nonkeyword_identifier()?;
                        self.consume_semicolon()?;
                        Ok(JumpStatement::Goto(target))
                    }
                    "continue" => {
                        self.next_token();
                        self.consume_semicolon()?;
                        Ok(JumpStatement::Continue)
                    }
                    "break" => {
                        self.next_token();
                        self.consume_semicolon()?;
                        Ok(JumpStatement::Break)
                    }
                    "return" => {
                        self.next_token();
                        if self.peek_and_equals_punctuator(0, Punctuator::Semicolon) {
                            self.consume_semicolon()?;
                            Ok(JumpStatement::Return(None))
                        } else {
                            let expr = self.parse_expression()?;
                            self.consume_semicolon()?;
                            Ok(JumpStatement::Return(Some(Box::new(expr))))
                        }
                    }
                    _ => Err(self
                        .build_error_with_peek_location(0, "Expected jump statement.".to_string())),
                }
            }
            Some(_) => {
                Err(self.build_error_with_peek_location(0, "Expected jump statement.".to_string()))
            }
            None => Err(self.build_error_unexpected_eof("Expected jump statement.".to_string())),
        }
    }

    fn parse_iteration_statement(&mut self) -> Result<IterationStatement, CompileError> {
        // iteration_statement
        //     : WHILE '(' expression ')' statement
        //     | DO statement WHILE '(' expression ')' ';'
        //     | FOR '(' expression_statement expression_statement ')' statement
        //     | FOR '(' expression_statement expression_statement expression ')' statement
        //     | FOR '(' declaration expression_statement ')' statement
        //     | FOR '(' declaration expression_statement expression ')' statement
        //     ;

        match self.peek_token(0) {
            Some(Token::Identifier(id)) => match id.as_str() {
                "while" => {
                    self.next_token();
                    self.consume_opening_parenthesis()?;
                    let cond = self.parse_expression()?;
                    self.consume_closing_parenthesis()?;
                    let body = self.parse_statement()?;

                    Ok(IterationStatement::While {
                        cond: Box::new(cond),
                        body: Box::new(body),
                    })
                }
                "do" => {
                    self.next_token();
                    let body = self.parse_statement()?;
                    self.consume_and_assert_keyword(C23Keyword::While)?;
                    self.consume_opening_parenthesis()?;
                    let cond = self.parse_expression()?;
                    self.consume_closing_parenthesis()?;
                    self.consume_semicolon()?;

                    Ok(IterationStatement::DoWhile {
                        body: Box::new(body),
                        cond: Box::new(cond),
                    })
                }
                "for" => {
                    self.next_token();
                    self.consume_opening_parenthesis()?;

                    // init clause: declaration OR expression_statement
                    let init = if self.peek_and_equals_keyword(0, C23Keyword::StaticAssert)
                        || self.peek_and_is_declaration_specifier(0)
                    {
                        let pos = self.peek_file_position(0);
                        let decl = self.parse_declaration()?;
                        // Declaration::Var does not consume ';'; the expression_statement does
                        if let Declaration::Var { .. } = &decl {
                            self.consume_semicolon()?;
                        }
                        ForInit::Declaration(Spanned::new(decl, pos))
                    } else {
                        let expr_opt = self.parse_expression_statement()?;
                        ForInit::Expression(expr_opt)
                    };

                    // condition (expression_statement — may be empty)
                    let cond = if self.peek_and_equals_punctuator(0, Punctuator::Semicolon) {
                        self.consume_semicolon()?;
                        None
                    } else {
                        let e = self.parse_expression()?;
                        self.consume_semicolon()?;
                        Some(Box::new(e))
                    };

                    // step expression (optional)
                    let step = if self.peek_and_equals_punctuator(0, Punctuator::ParenthesisClose) {
                        None
                    } else {
                        Some(Box::new(self.parse_expression()?))
                    };

                    self.consume_closing_parenthesis()?;
                    let body = self.parse_statement()?;
                    Ok(IterationStatement::For {
                        init,
                        cond,
                        step,
                        body: Box::new(body),
                    })
                }
                _ => Err(self.build_error_with_peek_location(
                    0,
                    "Expected iteration statement.".to_string(),
                )),
            },
            Some(_) => {
                Err(self
                    .build_error_with_peek_location(0, "Expected iteration statement.".to_string()))
            }
            None => {
                Err(self.build_error_unexpected_eof("Expected iteration statement.".to_string()))
            }
        }
    }

    fn parse_selection_statement(&mut self) -> Result<SelectionStatement, CompileError> {
        // selection_statement
        //     : IF '(' expression ')' statement ELSE statement
        //     | IF '(' expression ')' statement
        //     | SWITCH '(' expression ')' statement
        //     ;

        match self.peek_token(0) {
            Some(Token::Identifier(id)) => match id.as_str() {
                "if" => {
                    self.next_token();
                    self.consume_opening_parenthesis()?;
                    let cond = self.parse_expression()?;
                    self.consume_closing_parenthesis()?;
                    let then = self.parse_statement()?;

                    let else_ = if self.peek_and_equals_keyword(0, C23Keyword::Else) {
                        self.next_token();
                        Some(Box::new(self.parse_statement()?))
                    } else {
                        None
                    };

                    Ok(SelectionStatement::If {
                        cond: Box::new(cond),
                        then: Box::new(then),
                        else_,
                    })
                }
                "switch" => {
                    self.next_token();
                    self.consume_opening_parenthesis()?;
                    let cond = self.parse_expression()?;
                    self.consume_closing_parenthesis()?;
                    let body = self.parse_statement()?;
                    Ok(SelectionStatement::Switch {
                        cond: Box::new(cond),
                        body: Box::new(body),
                    })
                }
                _ => Err(self.build_error_with_peek_location(
                    0,
                    "Expected selection statement.".to_string(),
                )),
            },
            Some(_) => {
                Err(self
                    .build_error_with_peek_location(0, "Expected selection statement.".to_string()))
            }
            None => {
                Err(self.build_error_unexpected_eof("Expected selection statement.".to_string()))
            }
        }
    }

    fn parse_expression_statement(&mut self) -> Result<Option<Expr>, CompileError> {
        // expression_statement
        //     : ';'
        //     | expression ';'
        //     ;

        if self.peek_and_equals_punctuator(0, Punctuator::Semicolon) {
            self.consume_semicolon()?;
            Ok(None)
        } else {
            let expression = self.parse_expression()?;
            self.consume_semicolon()?;
            Ok(Some(expression))
        }
    }

    fn parse_expression(&mut self) -> Result<Expr, CompileError> {
        // expression
        //     : assignment_expression
        //     | expression ',' assignment_expression
        //     ;

        let mut lhs = self.parse_assignment_expression()?;
        while self.peek_and_equals_punctuator(0, Punctuator::Comma) {
            let pos = lhs.pos;
            self.consume_comma()?;
            let rhs = self.parse_assignment_expression()?;
            lhs = Spanned::new(Expression::Comma(Box::new(lhs), Box::new(rhs)), pos);
        }
        Ok(lhs)
    }

    fn parse_assignment_expression(&mut self) -> Result<Expr, CompileError> {
        // assignment_expression
        //     : conditional_expression
        //     | unary_expression assignment_operator assignment_expression
        //     ;

        let lhs = self.parse_conditional_expression()?;

        match self.peek_token(0) {
            Some(
                token @ Token::Punctuator(
                    Punctuator::Assign
                    | Punctuator::MultiplyAssign
                    | Punctuator::DivideAssign
                    | Punctuator::ModulusAssign
                    | Punctuator::AddAssign
                    | Punctuator::SubtractAssign
                    | Punctuator::ShiftLeftAssign
                    | Punctuator::ShiftRightAssign
                    | Punctuator::BitwiseAndAssign
                    | Punctuator::BitwiseXorAssign
                    | Punctuator::BitwiseOrAssign,
                ),
            ) => {
                let assign_op = AssignOp::from(token);
                let pos = lhs.pos;

                // todo::
                // We parse the LHS as a conditional_expression first.  If the next token is an
                // assignment operator we wrap it in Expression::Assign; otherwise we return the
                // conditional expression as-is.  Semantic analysis is responsible for checking that
                // the LHS is a valid lvalue (the `unary_expression`).

                self.next_token(); // consume the assignment operator
                let rhs = self.parse_assignment_expression()?;
                Ok(Spanned::new(
                    Expression::Assign(assign_op, Box::new(lhs), Box::new(rhs)),
                    pos,
                ))
            }
            _ => Ok(lhs),
        }
    }

    fn parse_conditional_expression(&mut self) -> Result<Expr, CompileError> {
        // conditional_expression
        //     : logical_or_expression
        //     | logical_or_expression '?' expression ':' conditional_expression
        //     ;

        let expression = self.parse_logical_or_expression()?;
        if self.peek_and_equals_punctuator(0, Punctuator::QuestionMark) {
            let pos = expression.pos;
            self.consume_question_mark()?;
            let true_expression = self.parse_expression()?;
            self.consume_colon()?;
            let false_expression = self.parse_conditional_expression()?;

            Ok(Spanned::new(
                Expression::Conditional(
                    Box::new(expression),
                    Box::new(true_expression),
                    Box::new(false_expression),
                ),
                pos,
            ))
        } else {
            Ok(expression)
        }
    }

    fn parse_logical_or_expression(&mut self) -> Result<Expr, CompileError> {
        // logical_or_expression
        //     : logical_and_expression
        //     | logical_or_expression OR_OP logical_and_expression
        //     ;
        self.parse_binary_expression(&[Punctuator::Or], Self::parse_logical_and_expression)
    }

    fn parse_logical_and_expression(&mut self) -> Result<Expr, CompileError> {
        // logical_and_expression
        //     : inclusive_or_expression
        //     | logical_and_expression AND_OP inclusive_or_expression
        //     ;
        self.parse_binary_expression(&[Punctuator::And], Self::parse_inclusive_or_expression)
    }

    fn parse_inclusive_or_expression(&mut self) -> Result<Expr, CompileError> {
        // inclusive_or_expression
        //     : exclusive_or_expression
        //     | inclusive_or_expression '|' exclusive_or_expression
        //     ;
        self.parse_binary_expression(
            &[Punctuator::BitwiseOr],
            Self::parse_exclusive_or_expression,
        )
    }

    fn parse_exclusive_or_expression(&mut self) -> Result<Expr, CompileError> {
        // exclusive_or_expression
        //     : and_expression
        //     | exclusive_or_expression '^' and_expression
        //     ;
        self.parse_binary_expression(&[Punctuator::BitwiseXor], Self::parse_and_expression)
    }

    fn parse_and_expression(&mut self) -> Result<Expr, CompileError> {
        // and_expression
        //     : equality_expression
        //     | and_expression '&' equality_expression
        //     ;

        self.parse_binary_expression(&[Punctuator::BitwiseAnd], Self::parse_equality_expression)
    }

    fn parse_equality_expression(&mut self) -> Result<Expr, CompileError> {
        // equality_expression
        //     : relational_expression
        //     | equality_expression EQ_OP relational_expression
        //     | equality_expression NE_OP relational_expression
        //     ;
        self.parse_binary_expression(
            &[Punctuator::Equal, Punctuator::NotEqual],
            Self::parse_relational_expression,
        )
    }

    fn parse_relational_expression(&mut self) -> Result<Expr, CompileError> {
        // relational_expression
        //     : shift_expression
        //     | relational_expression '<' shift_expression
        //     | relational_expression '>' shift_expression
        //     | relational_expression LE_OP shift_expression
        //     | relational_expression GE_OP shift_expression
        //     ;
        self.parse_binary_expression(
            &[
                Punctuator::LessThan,
                Punctuator::GreaterThan,
                Punctuator::LessThanOrEqual,
                Punctuator::GreaterThanOrEqual,
            ],
            Self::parse_shift_expression,
        )
    }

    fn parse_shift_expression(&mut self) -> Result<Expr, CompileError> {
        // shift_expression
        //     : additive_expression
        //     | shift_expression LEFT_OP additive_expression
        //     | shift_expression RIGHT_OP additive_expression
        //     ;

        self.parse_binary_expression(
            &[Punctuator::ShiftLeft, Punctuator::ShiftRight],
            Self::parse_additive_expression,
        )
    }

    fn parse_additive_expression(&mut self) -> Result<Expr, CompileError> {
        // additive_expression
        //     : multiplicative_expression
        //     | additive_expression '+' multiplicative_expression
        //     | additive_expression '-' multiplicative_expression
        //     ;

        self.parse_binary_expression(
            &[Punctuator::Add, Punctuator::Subtract],
            Self::parse_multiplicative_expression,
        )
    }

    fn parse_multiplicative_expression(&mut self) -> Result<Expr, CompileError> {
        // multiplicative_expression
        //     : cast_expression
        //     | multiplicative_expression '*' cast_expression
        //     | multiplicative_expression '/' cast_expression
        //     | multiplicative_expression '%' cast_expression
        //     ;

        self.parse_binary_expression(
            &[Punctuator::Multiply, Punctuator::Divide, Punctuator::Modulo],
            Self::parse_cast_expression,
        )
    }

    fn parse_binary_expression(
        &mut self,
        operator_punctuators: &[Punctuator],
        parse_rhs: fn(&mut Self) -> Result<Expr, CompileError>,
    ) -> Result<Expr, CompileError> {
        let mut expression = parse_rhs(self)?;

        while let Some(token) = self.peek_token(0) {
            if let Token::Punctuator(punctuator) = token
                && operator_punctuators.contains(punctuator)
            {
                let operator = BinaryOp::from(token);
                let pos = expression.pos;
                self.next_token(); // consume the operator
                let rhs = parse_rhs(self)?;
                expression = Spanned::new(
                    Expression::Binary(operator, Box::new(expression), Box::new(rhs)),
                    pos,
                );
            } else {
                break;
            }
        }

        Ok(expression)
    }

    fn parse_cast_expression(&mut self) -> Result<Expr, CompileError> {
        // cast_expression
        //     : unary_expression
        //     | '(' type_name ')' cast_expression
        //     ;
        //
        // Compound literals also start with '(' type_name ')' but are followed by '{'.
        // They are handled here: when '{' follows the type name we build a
        // CompoundLiteral node (playing the role of postfix_expression) and then
        // thread it through the postfix operator loop.

        if self.peek_and_equals_punctuator(0, Punctuator::ParenthesisOpen)
            && self.peek_and_is_specifier_qualifier(1)
        {
            let pos = self.peek_file_position(0);
            self.consume_opening_parenthesis()?;

            // todo::
            // if we want to support the C23 compound-literal form with storage-class specifier,
            // we need to parse an optional storage-class specifier here and pass it to the CompoundLiteral node.
            let storage: Option<StorageClassSpecifier> = None;

            let type_name = self.parse_type_name()?;
            self.consume_closing_parenthesis()?;

            if self.peek_and_equals_punctuator(0, Punctuator::BraceOpen) {
                // Compound literal: '(' type_name ')' '{' ... '}'
                self.consume_opening_brace()?;
                let items = if self.peek_and_equals_punctuator(0, Punctuator::BraceClose) {
                    vec![]
                } else {
                    let list = self.parse_initializer_list()?;

                    // trailing comma is allowed before the closing brace
                    if self.peek_and_equals_punctuator(0, Punctuator::Comma) {
                        self.consume_comma()?;
                    }
                    list
                };
                self.consume_closing_brace()?;

                // Build the compound-literal base expression and apply postfix operators.
                let base = Spanned::new(
                    Expression::CompoundLiteral {
                        storage,
                        type_name,
                        items,
                    },
                    pos,
                );
                self.parse_postfix_expression_suffix(base)
            } else {
                // Plain cast: '(' type_name ')' cast_expression
                let inner = self.parse_cast_expression()?;
                Ok(Spanned::new(
                    Expression::Cast(type_name, Box::new(inner)),
                    pos,
                ))
            }
        } else {
            self.parse_unary_expression()
        }
    }

    fn parse_unary_expression(&mut self) -> Result<Expr, CompileError> {
        // unary_expression
        //     : postfix_expression
        //     | INC_OP unary_expression
        //     | DEC_OP unary_expression
        //     | unary_operator cast_expression
        //     | SIZEOF unary_expression
        //     | SIZEOF '(' type_name ')'
        //     | ALIGNOF '(' type_name ')'
        //     ;
        //
        // unary_operator: '&' | '*' | '+' | '-' | '~' | '!'

        match self.peek_token(0) {
            Some(Token::Punctuator(Punctuator::Increase)) => {
                let pos = self.peek_file_position(0);
                self.next_token();
                let operand = self.parse_unary_expression()?;
                Ok(Spanned::new(
                    Expression::PreIncrement(Box::new(operand)),
                    pos,
                ))
            }
            Some(Token::Punctuator(Punctuator::Decrease)) => {
                let pos = self.peek_file_position(0);
                self.next_token();
                let operand = self.parse_unary_expression()?;
                Ok(Spanned::new(
                    Expression::PreDecrement(Box::new(operand)),
                    pos,
                ))
            }
            Some(
                token @ Token::Punctuator(
                    Punctuator::BitwiseAnd
                    | Punctuator::Multiply
                    | Punctuator::Add
                    | Punctuator::Subtract
                    | Punctuator::BitwiseNot
                    | Punctuator::Not,
                ),
            ) => {
                let op = UnaryOp::from(token);
                let pos = self.peek_file_position(0);
                self.next_token();
                let operand = self.parse_cast_expression()?;
                Ok(Spanned::new(Expression::Unary(op, Box::new(operand)), pos))
            }
            Some(Token::Identifier(id)) if id == "sizeof" => {
                let pos = self.peek_file_position(0);
                self.next_token();
                if self.peek_and_equals_punctuator(0, Punctuator::ParenthesisOpen)
                    && self.peek_and_is_specifier_qualifier(1)
                {
                    // it is `sizeof (type_name)`
                    self.consume_opening_parenthesis()?;
                    let type_name = self.parse_type_name()?;
                    self.consume_closing_parenthesis()?;
                    Ok(Spanned::new(
                        Expression::Sizeof(SizeofOperand::Type(type_name)),
                        pos,
                    ))
                } else {
                    // it is `sizeof unary_expression`
                    let operand = self.parse_unary_expression()?;
                    Ok(Spanned::new(
                        Expression::Sizeof(SizeofOperand::Expression(Box::new(operand))),
                        pos,
                    ))
                }
            }
            Some(Token::Identifier(id)) if id == "alignof" => {
                let pos = self.peek_file_position(0);
                self.next_token();
                self.consume_opening_parenthesis()?;
                let tn = self.parse_type_name()?;
                self.consume_closing_parenthesis()?;
                Ok(Spanned::new(Expression::Alignof(tn), pos))
            }
            _ => self.parse_postfix_expression(),
        }
    }

    fn parse_postfix_expression(&mut self) -> Result<Expr, CompileError> {
        // postfix_expression
        //     : primary_expression
        //     | postfix_expression '[' expression ']'
        //     | postfix_expression '(' ')'
        //     | postfix_expression '(' argument_expression_list ')'
        //     | postfix_expression '.' IDENTIFIER
        //     | postfix_expression PTR_OP IDENTIFIER
        //     | postfix_expression INC_OP
        //     | postfix_expression DEC_OP
        //     | '(' type_name ')' '{' initializer_list '}'
        //     | '(' type_name ')' '{' initializer_list ',' '}'
        //     | '(' type_name ')' '{' '}'        /* C23: compound literal with empty initializer */
        //     /* C23 N3038: compound literal with a storage-class specifier. */
        //     /* Allowed specifiers: static, register, thread_local, constexpr, auto. */
        //     | '(' storage_class_specifier type_name ')' '{' initializer_list '}'
        //     | '(' storage_class_specifier type_name ')' '{' initializer_list ',' '}'
        //     | '(' storage_class_specifier type_name ')' '{' '}'
        //     ;

        // The compound literal cases are currently handled in `parse_cast_expression`,
        // Note that compound literals are not cast, the following are examples of compound literals:
        //
        // - `(int){42}`
        // - `(struct Point){1, 2}`
        // - `(struct Point){ .x = 3, .y = 4 }`
        // - `(int[]){1,2,3}`

        let base = self.parse_primary_expression()?;
        self.parse_postfix_expression_suffix(base)
    }

    /// Applies any postfix operators ([...], (...), .id, ->id, ++, --) to `base`.
    fn parse_postfix_expression_suffix(&mut self, mut expr: Expr) -> Result<Expr, CompileError> {
        while let Some(Token::Punctuator(p)) = self.peek_token(0) {
            match p {
                Punctuator::BracketOpen => {
                    let pos = expr.pos;
                    self.consume_opening_bracket()?;
                    let index = self.parse_expression()?;
                    self.consume_closing_bracket()?;
                    expr = Spanned::new(Expression::Index(Box::new(expr), Box::new(index)), pos);
                }
                Punctuator::ParenthesisOpen => {
                    let pos = expr.pos;
                    self.consume_opening_parenthesis()?;
                    let mut args = Vec::new();
                    if !self.peek_and_equals_punctuator(0, Punctuator::ParenthesisClose) {
                        args.push(self.parse_assignment_expression()?);
                        while self.peek_and_equals_punctuator(0, Punctuator::Comma) {
                            self.consume_comma()?;
                            args.push(self.parse_assignment_expression()?);
                        }
                    }
                    self.consume_closing_parenthesis()?;
                    expr = Spanned::new(Expression::Call(Box::new(expr), args), pos);
                }
                Punctuator::Dot => {
                    let pos = expr.pos;
                    self.next_token();
                    let member = self.consume_identifier()?;
                    expr = Spanned::new(Expression::Member(Box::new(expr), member), pos);
                }
                Punctuator::Arrow => {
                    let pos = expr.pos;
                    self.next_token();
                    let member = self.consume_identifier()?;
                    expr = Spanned::new(Expression::ArrowMember(Box::new(expr), member), pos);
                }
                Punctuator::Increase => {
                    let pos = expr.pos;
                    self.next_token();
                    expr = Spanned::new(Expression::PostIncrement(Box::new(expr)), pos);
                }
                Punctuator::Decrease => {
                    let pos = expr.pos;
                    self.next_token();
                    expr = Spanned::new(Expression::PostDecrement(Box::new(expr)), pos);
                }
                _ => {
                    break;
                }
            }
        }

        Ok(expr)
    }

    fn parse_primary_expression(&mut self) -> Result<Expr, CompileError> {
        // primary_expression
        //     : IDENTIFIER
        //     | constant
        //     | string
        //     | '(' expression ')'
        //     | generic_selection
        //     | NULLPTR            /* C23: null-pointer constant */
        //     ;

        let pos = self.peek_file_position(0);

        match self.peek_token(0) {
            // `_Generic(...)`
            Some(Token::Identifier(id)) if id == "_Generic" => self.parse_generic_selection(),
            // `nullptr`
            Some(Token::Identifier(id)) if id == "nullptr" => {
                self.next_token();
                Ok(Spanned::new(Expression::Nullptr, pos))
            }
            // `true`
            Some(Token::Identifier(id)) if id == "true" => {
                self.next_token();
                Ok(Spanned::new(
                    Expression::Constant(Constant::Bool(true)),
                    pos,
                ))
            }
            // `false`
            Some(Token::Identifier(id)) if id == "false" => {
                self.next_token();
                Ok(Spanned::new(
                    Expression::Constant(Constant::Bool(false)),
                    pos,
                ))
            }
            // `__func__`
            Some(Token::Identifier(id)) if id == "__func__" => {
                self.next_token();
                Ok(Spanned::new(
                    Expression::String(StringLiteral::FuncName),
                    pos,
                ))
            }
            // Enumeration constant or regular identifier
            Some(Token::Identifier(id)) => {
                let id = id.clone();
                self.next_token();
                if self.exists_enum_constant(&id) {
                    Ok(Spanned::new(
                        Expression::Constant(Constant::Enumeration(id)),
                        pos,
                    ))
                } else {
                    Ok(Spanned::new(Expression::Identifier(id), pos))
                }
            }
            // Integer literal
            Some(Token::Number(Number::Integer(n))) => {
                let num = n.clone();
                self.next_token();
                Ok(Spanned::new(
                    Expression::Constant(Constant::Integer(num)),
                    pos,
                ))
            }
            // Float literal
            Some(Token::Number(Number::FloatingPoint(n))) => {
                let num = n.clone();
                self.next_token();
                Ok(Spanned::new(
                    Expression::Constant(Constant::Float(num)),
                    pos,
                ))
            }
            // Character constant (represented as integer in C)
            Some(Token::Char(c, _)) => {
                let codepoint = *c as u32;
                let num =
                    IntegerNumber::new(codepoint.to_string(), true, IntegerNumberWidth::Default);
                self.next_token();
                Ok(Spanned::new(
                    Expression::Constant(Constant::Integer(num)),
                    pos,
                ))
            }
            // String literal (possibly concatenated — preprocessing should have merged them)
            Some(Token::String(s, _)) => {
                let s = s.clone();
                self.next_token();
                Ok(Spanned::new(
                    Expression::String(StringLiteral::Literal(s)),
                    pos,
                ))
            }
            // `( expression )` — grouped expression
            Some(Token::Punctuator(Punctuator::ParenthesisOpen)) => {
                self.consume_opening_parenthesis()?;
                let expr = self.parse_expression()?;
                self.consume_closing_parenthesis()?;
                Ok(expr)
            }
            Some(_) => {
                Err(self
                    .build_error_with_peek_location(0, "Expected primary expression.".to_string()))
            }
            None => {
                Err(self.build_error_unexpected_eof("Expected primary expression.".to_string()))
            }
        }
    }

    fn parse_generic_selection(&mut self) -> Result<Expr, CompileError> {
        // C23 6.5.1.1: The controlling operand may be an assignment_expression
        // OR a type_name.  Both forms are now grouped under the new
        // generic_controlling_operand non-terminal.
        //
        // generic_selection
        //     : GENERIC '(' generic_controlling_operand ',' generic_assoc_list ')'
        //     ;

        // generic_controlling_operand
        //     : assignment_expression
        //     | type_name
        //     ;

        // generic_assoc_list
        //     : generic_association
        //     | generic_assoc_list ',' generic_association
        //     ;

        // generic_association
        //     : type_name ':' assignment_expression
        //     | DEFAULT ':' assignment_expression
        //     ;

        let pos = self.peek_file_position(0);
        self.consume_and_assert_keyword(C23Keyword::Generic)?;
        self.consume_opening_parenthesis()?;

        // Parse controlling operand: type_name or assignment_expression
        let controlling = if self.peek_and_is_specifier_qualifier(0) {
            GenericControlling::Type(self.parse_type_name()?)
        } else {
            GenericControlling::Expression(Box::new(self.parse_assignment_expression()?))
        };

        self.consume_comma()?;

        // Parse generic_assoc_list
        let mut associations = Vec::new();

        while !self.is_eof() {
            if self.peek_and_equals_keyword(0, C23Keyword::Default) {
                self.next_token(); // consume 'default'
                self.consume_colon()?; // consume ':'
                let expression = self.parse_assignment_expression()?;
                associations.push(GenericAssociation::Default(Box::new(expression)));
            } else {
                let type_name = self.parse_type_name()?;
                self.consume_colon()?; // consume ':'
                let expression = self.parse_assignment_expression()?;
                associations.push(GenericAssociation::Type(type_name, Box::new(expression)));
            }

            if self.peek_and_equals_punctuator(0, Punctuator::Comma) {
                self.consume_comma()?; // consume ','
            } else {
                break;
            }
        }

        self.consume_closing_parenthesis()?;

        Ok(Spanned::new(
            Expression::Generic(Box::new(GenericSelection {
                controlling,
                associations,
            })),
            pos,
        ))
    }
}

impl From<&Token> for BinaryOp {
    fn from(token: &Token) -> Self {
        match token {
            Token::Punctuator(punctuator) => match punctuator {
                Punctuator::Or => BinaryOp::LogicalOr,
                Punctuator::And => BinaryOp::LogicalAnd,
                Punctuator::BitwiseOr => BinaryOp::BitwiseOr,
                Punctuator::BitwiseXor => BinaryOp::BitwiseXor, // `^`
                Punctuator::BitwiseAnd => BinaryOp::BitwiseAnd,
                Punctuator::Equal => BinaryOp::Equal,
                Punctuator::NotEqual => BinaryOp::NotEqual,
                Punctuator::LessThan => BinaryOp::LessThan,
                Punctuator::GreaterThan => BinaryOp::GreaterThan,
                Punctuator::LessThanOrEqual => BinaryOp::LessThanOrEqual,
                Punctuator::GreaterThanOrEqual => BinaryOp::GreaterThanOrEqual,
                Punctuator::ShiftLeft => BinaryOp::ShiftLeft,
                Punctuator::ShiftRight => BinaryOp::ShiftRight,
                Punctuator::Add => BinaryOp::Add,
                Punctuator::Subtract => BinaryOp::Subtract,
                Punctuator::Multiply => BinaryOp::Multiply,
                Punctuator::Divide => BinaryOp::Divide,
                Punctuator::Modulo => BinaryOp::Modulo,
                _ => panic!("Invalid binary operator token: {:?}", token),
            },
            _ => panic!("Invalid binary operator token: {:?}", token),
        }
    }
}

impl From<&Token> for UnaryOp {
    fn from(token: &Token) -> Self {
        match token {
            Token::Punctuator(punctuator) => match punctuator {
                Punctuator::BitwiseAnd => UnaryOp::AddressOf,
                Punctuator::Multiply => UnaryOp::Dereference,
                Punctuator::Add => UnaryOp::Plus,
                Punctuator::Subtract => UnaryOp::Minus,
                Punctuator::BitwiseNot => UnaryOp::BitwiseNot,
                Punctuator::Not => UnaryOp::LogicalNot,
                _ => panic!("Invalid unary operator token: {:?}", token),
            },
            _ => panic!("Invalid unary operator token: {:?}", token),
        }
    }
}

impl From<&Token> for AssignOp {
    fn from(token: &Token) -> Self {
        match token {
            Token::Punctuator(punctuator) => match punctuator {
                Punctuator::Assign => AssignOp::Assign,
                Punctuator::MultiplyAssign => AssignOp::MultiplyAssign,
                Punctuator::DivideAssign => AssignOp::DivideAssign,
                Punctuator::ModulusAssign => AssignOp::ModuloAssign,
                Punctuator::AddAssign => AssignOp::AddAssign,
                Punctuator::SubtractAssign => AssignOp::SubtractAssign,
                Punctuator::ShiftLeftAssign => AssignOp::ShiftLeftAssign,
                Punctuator::ShiftRightAssign => AssignOp::ShiftRightAssign,
                Punctuator::BitwiseAndAssign => AssignOp::AndAssign,
                Punctuator::BitwiseXorAssign => AssignOp::XorAssign,
                Punctuator::BitwiseOrAssign => AssignOp::OrAssign,
                _ => panic!("Invalid assignment operator token: {:?}", token),
            },
            _ => panic!("Invalid assignment operator token: {:?}", token),
        }
    }
}

/// Extracts the innermost declared identifier name from a `DirectDeclarator`.
fn extract_declarator_name(direct: &DirectDeclarator) -> Option<String> {
    match direct {
        DirectDeclarator::Identifier(name) => Some(name.clone()),
        DirectDeclarator::Parenthesized(decl) => extract_declarator_name(&decl.direct),
        DirectDeclarator::Array(inner, _) => extract_declarator_name(inner),
        DirectDeclarator::Function(inner, _) => extract_declarator_name(inner),
    }
}
