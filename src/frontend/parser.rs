// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::{collections::HashMap, path::Path};

use ancpp::{
    context::{FileProvider, HeaderFileCache, PreprocessResult},
    linter::Linter,
    location::Location,
    peekable_iter::PeekableIter,
    process_source_file,
    token::{C23_KEYWORD_STRS, C23Keyword, Punctuator, Token, TokenWithLocation},
};

use crate::{
    error::CompileError,
    frontend::cst::{
        Attribute, BinaryOp, BlockItem, Declaration, DeclarationSpecifier, Expression,
        ExternalDeclaration, FunctionDefinition, InitDeclarator, StaticAssert, TranslationUnit,
        UnaryOp,
    },
    indication::Indication,
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
        // file scope starts with an empty symbol table, which is used to store the typedef names
        // and enumeration constants in the file scope.
        let symbol_stack = vec![SymbolTable {
            symbols: HashMap::new(),
        }];

        Self {
            upstream,
            linters,
            symbol_stack,
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

    fn peek_token_with_location(&self, offset: usize) -> Option<&TokenWithLocation> {
        self.upstream.peek(offset)
    }

    fn peek_location(&self, offset: usize) -> Option<&Location> {
        match self.upstream.peek(offset) {
            Some(TokenWithLocation { location, .. }) => Some(location),
            None => None,
        }
    }

    fn is_eof(&self) -> bool {
        self.peek_token(0).is_none()
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

    fn consume_keyword(&mut self, expected_keyword: C23Keyword) -> Result<(), CompileError> {
        let keyword_str = C23_KEYWORD_STRS[expected_keyword as usize];

        match self.next_token() {
            Some(Token::Identifier(id)) => {
                if id == keyword_str {
                    Ok(())
                } else {
                    Err(CompileError::MessageWithIndication(
                        Indication::from_position(
                            self.last_location.file_number,
                            &self.last_location.range.start,
                        ),
                        format!("Expected keyword: {}.", keyword_str),
                    ))
                }
            }
            Some(_) => Err(CompileError::MessageWithIndication(
                Indication::from_position(
                    self.last_location.file_number,
                    &self.last_location.range.start,
                ),
                format!("Expected keyword: {}.", keyword_str),
            )),
            None => Err(CompileError::UnexpectedEndOfDocument(
                self.last_location.file_number,
                format!("Expected keyword: {}.", keyword_str),
            )),
        }
    }

    fn consume_string(&mut self) -> Result<String, CompileError> {
        match self.next_token() {
            Some(Token::String(s, _)) => Ok(s),
            Some(_) => Err(CompileError::MessageWithIndication(
                Indication::from_position(
                    self.last_location.file_number,
                    &self.last_location.range.start,
                ),
                "Expected string literal.".to_string(),
            )),
            None => Err(CompileError::UnexpectedEndOfDocument(
                self.last_location.file_number,
                "Expected string literal.".to_string(),
            )),
        }
    }

    /// Consume the next token and check if it equals to the expected token,
    /// return Ok(()) if it matches, otherwise return an error.
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
                    Err(CompileError::MessageWithIndication(
                        Indication::from_position(
                            self.last_location.file_number,
                            &self.last_location.range.start,
                        ),
                        format!("Expected token: {}.", token_description),
                    ))
                }
            }
            None => Err(CompileError::UnexpectedEndOfDocument(
                self.last_location.file_number,
                format!("Expected token: {}.", token_description),
            )),
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
}

impl<'a> Parser<'a> {
    fn check_identifier_type(&self, identifier: &str) -> Option<SymbolType> {
        for symbol_table in self.symbol_stack.iter().rev() {
            if let Some(symbol_type) = symbol_table.symbols.get(identifier) {
                return Some(*symbol_type);
            }
        }
        None
    }

    fn exists_typedef_name(&self, identifier: &str) -> bool {
        matches!(self.check_identifier_type(identifier), Some(SymbolType::TypeName))
    }

    fn exists_enum_constant(&self, identifier: &str) -> bool {
        matches!(self.check_identifier_type(identifier), Some(SymbolType::EnumConstant))
    }

    fn insert_typename(&mut self, identifier: String) {
        if let Some(symbol_table) = self.symbol_stack.last_mut() {
            symbol_table
                .symbols
                .insert(identifier, SymbolType::TypeName);
        }
    }

    fn insert_enum_constant(&mut self, identifier: String) {
        if let Some(symbol_table) = self.symbol_stack.last_mut() {
            symbol_table
                .symbols
                .insert(identifier, SymbolType::EnumConstant);
        }
    }

    fn push_symbol_table(&mut self) {
        self.symbol_stack.push(SymbolTable {
            symbols: HashMap::new(),
        });
    }

    fn pop_symbol_table(&mut self) {
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
    predefinitions: &HashMap<String, String>,
    resolve_relative_path_within_current_file: bool,
    enable_single_argument_multiple_tokens: bool,
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
        predefinitions,
        resolve_relative_path_within_current_file,
        enable_single_argument_multiple_tokens,
        source_file_number,
        source_file_path_name,
        source_file_canonical_full_path,
    )
    .map_err(|e| CompileError::PreprocessError(e))?;

    let PreprocessResult {
        token_with_locations,
        linters,
    } = preprocess_result;
    let mut token_iter = token_with_locations.into_iter();
    let mut peekable_token_iter = PeekableIter::new(&mut token_iter);
    let mut parser = Parser::new(&mut peekable_token_iter, linters);

    let translation_unit = parser.parse_translation_unit()?;
    let parse_result = ParseResult {
        cst: translation_unit,
        linters: parser.linters,
    };

    Ok(parse_result)
}

impl<'a> Parser<'a> {
    fn parse_translation_unit(&mut self) -> Result<TranslationUnit, CompileError> {
        let mut external_declarations = Vec::new();
        while !self.is_eof() {
            external_declarations.push(self.parse_external_declaration()?);
        }

        Ok(TranslationUnit {
            external_declarations,
        })
    }

    fn parse_external_declaration(&mut self) -> Result<ExternalDeclaration, CompileError> {
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
                        return Err(CompileError::MessageWithIndication(
                            Indication::default(), // TODO:: position need to be improved
                            "Initializer is not allowed in function definition.".to_string(),
                        ));
                    }
                    if let InitDeclarator::Plain(decl) = init_declarator {
                        declarator.push(decl);
                    }
                }

                if declarator.len() != 1 {
                    return Err(CompileError::MessageWithIndication(
                        Indication::default(), // TODO:: position need to be improved
                        "Only one declarator is allowed in function definition.".to_string(),
                    ));
                }

                ExternalDeclaration::Function(FunctionDefinition {
                    attributes,
                    declaration_specifiers,
                    declarator: declarator.into_iter().next().unwrap(),
                    body,
                })
            } else {
                // it is `declaration`
                self.consume_semicolon()?; // consume the ';' after the declaration
                ExternalDeclaration::Declaration(Declaration::Var {
                    attributes,
                    declaration_specifiers,
                    init_declarators,
                })
            }
        } else {
            // it is attribute declaration or static assert declaration.
            ExternalDeclaration::Declaration(declaration)
        };

        Ok(external_declaration)
    }

    fn parse_declaration(&mut self) -> Result<Declaration, CompileError> {
        if self.peek_and_equals_keyword(0, C23Keyword::StaticAssert) {
            // `static_assert_declaration`
            let static_assert = self.parse_static_assert_declaration()?;
            return Ok(Declaration::StaticAssert(static_assert));
        }

        let attributes = if self.peek_and_equals_punctuator(0, Punctuator::AttributeOpen) {
            // `attribute_specifier_sequence ';'`
            let attributes = self.parse_attribute_specifier_sequence()?;
            self.consume_semicolon()?; // consume the ';' after the attribute specifier sequence
            attributes
        } else {
            vec![]
        };

        if self.peek_and_equals_punctuator(0, Punctuator::Semicolon) {
            // `attribute_specifier_sequence ';'`
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

        todo!()
    }

    fn parse_static_assert_declaration(&mut self) -> Result<StaticAssert, CompileError> {
        // static_assert_declaration
        //     : STATIC_ASSERT '(' constant_expression ',' STRING_LITERAL ')' ';'
        //     | STATIC_ASSERT '(' constant_expression ')' ';'
        //     /* C23: message string is now optional */
        //     ;

        self.consume_keyword(C23Keyword::StaticAssert)?;
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

    fn parse_constant_expression(&mut self) -> Result<Expression, CompileError> {
        // constant_expression
        //     : conditional_expression    /* with constraints */
        //     ;
        self.parse_conditional_expression()
    }

    fn parse_declaration_specifiers(&mut self) -> Result<Vec<DeclarationSpecifier>, CompileError> {
        // declaration_specifiers
        //     : storage_class_specifier declaration_specifiers
        //     | storage_class_specifier
        //     | type_specifier declaration_specifiers
        //     | type_specifier
        //     | type_qualifier declaration_specifiers
        //     | type_qualifier
        //     | function_specifier declaration_specifiers
        //     | function_specifier
        //     | alignment_specifier declaration_specifiers
        //     | alignment_specifier
        //     ;

        todo!()
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

        todo!()
    }

    fn parse_compound_statement(&mut self) -> Result<Vec<BlockItem>, CompileError> {
        // compound_statement
        //     : '{' '}'
        //     | '{' block_item_list '}'
        //     ;
        let mut block_items = vec![];

        self.push_symbol_table(); // new block scope

        self.consume_opening_brace()?;
        while !self.peek_and_equals_punctuator(0, Punctuator::BraceClose) {
            block_items.push(self.parse_block_item()?);
        }
        self.consume_closing_brace()?;

        self.pop_symbol_table(); // exit block scope
        Ok(block_items)
    }

    fn parse_block_item(&mut self) -> Result<BlockItem, CompileError> {
        // block_item
        //     : declaration
        //     | statement
        //     | label        /* C23: standalone label before a declaration or end of block */
        //     ;

        todo!()
    }

    fn parse_statement(&mut self) -> Result<Expression, CompileError> {
        // statement
        //     : labeled_statement
        //     | compound_statement
        //     | expression_statement
        //     | selection_statement
        //     | iteration_statement
        //     | jump_statement
        //     ;

        todo!()
    }

    fn parse_labeled_statement(&mut self) -> Result<Expression, CompileError> {
        // labeled_statement: a label immediately followed by a statement.
        // Note: when a label appears at the end of a block or before a declaration,
        // it is reduced as a standalone block_item (see block_item below).
        // The shift/reduce conflict between  label _ statement  is resolved by
        // preferring shift, which produces the expected labeled_statement.
        //
        // labeled_statement
        //     : label statement
        //     ;

        // C23: label is separated into its own non-terminal so that it can also appear
        // as a standalone block_item (preceding a declaration or the closing '}').
        //
        // label
        //     : attribute_specifier_seq_opt IDENTIFIER ':'
        //     | attribute_specifier_seq_opt CASE constant_expression ':'
        //     | attribute_specifier_seq_opt DEFAULT ':'
        //     ;

        todo!()
    }

    fn parse_jump_statement(&mut self) -> Result<Expression, CompileError> {
        // jump_statement
        //     : GOTO IDENTIFIER ';'
        //     | CONTINUE ';'
        //     | BREAK ';'
        //     | RETURN ';'
        //     | RETURN expression ';'
        //     ;

        todo!()
    }

    fn parse_iteration_statement(&mut self) -> Result<Expression, CompileError> {
        // iteration_statement
        //     : WHILE '(' expression ')' statement
        //     | DO statement WHILE '(' expression ')' ';'
        //     | FOR '(' expression_statement expression_statement ')' statement
        //     | FOR '(' expression_statement expression_statement expression ')' statement
        //     | FOR '(' declaration expression_statement ')' statement
        //     | FOR '(' declaration expression_statement expression ')' statement
        //     ;

        todo!()
    }

    fn parse_selection_statement(&mut self) -> Result<Expression, CompileError> {
        // selection_statement
        //     : IF '(' expression ')' statement ELSE statement
        //     | IF '(' expression ')' statement
        //     | SWITCH '(' expression ')' statement
        //     ;

        todo!()
    }

    fn parse_expression_statement(&mut self) -> Result<Option<Expression>, CompileError> {
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

    fn parse_expression(&mut self) -> Result<Expression, CompileError> {
        // expression
        //     : assignment_expression
        //     | expression ',' assignment_expression
        //     ;

        todo!()
    }

    fn parse_assignment_expression(&mut self) -> Result<Expression, CompileError> {
        // assignment_expression
        //     : conditional_expression
        //     | unary_expression assignment_operator assignment_expression
        //     ;

        // assignment_operator
        //     : '='
        //     | MUL_ASSIGN
        //     | DIV_ASSIGN
        //     | MOD_ASSIGN
        //     | ADD_ASSIGN
        //     | SUB_ASSIGN
        //     | LEFT_ASSIGN
        //     | RIGHT_ASSIGN
        //     | AND_ASSIGN
        //     | XOR_ASSIGN
        //     | OR_ASSIGN
        //     ;

        todo!()
    }

    fn parse_conditional_expression(&mut self) -> Result<Expression, CompileError> {
        // conditional_expression
        //     : logical_or_expression
        //     | logical_or_expression '?' expression ':' conditional_expression
        //     ;

        let expression = self.parse_logical_or_expression()?;
        if self.peek_and_equals_punctuator(0, Punctuator::QuestionMark) {
            self.consume_question_mark()?;
            let true_expression = self.parse_expression()?;
            self.consume_colon()?;
            let false_expression = self.parse_conditional_expression()?;

            Ok(Expression::Conditional(
                Box::new(expression),
                Box::new(true_expression),
                Box::new(false_expression),
            ))
        } else {
            Ok(expression)
        }
    }

    fn parse_logical_or_expression(&mut self) -> Result<Expression, CompileError> {
        // logical_or_expression
        //     : logical_and_expression
        //     | logical_or_expression OR_OP logical_and_expression
        //     ;
        self.parse_binary_expression(&[Punctuator::Or], Self::parse_logical_and_expression)
    }

    fn parse_logical_and_expression(&mut self) -> Result<Expression, CompileError> {
        // logical_and_expression
        //     : inclusive_or_expression
        //     | logical_and_expression AND_OP inclusive_or_expression
        //     ;
        self.parse_binary_expression(&[Punctuator::And], Self::parse_inclusive_or_expression)
    }

    fn parse_inclusive_or_expression(&mut self) -> Result<Expression, CompileError> {
        // inclusive_or_expression
        //     : exclusive_or_expression
        //     | inclusive_or_expression '|' exclusive_or_expression
        //     ;
        self.parse_binary_expression(
            &[Punctuator::BitwiseOr],
            Self::parse_exclusive_or_expression,
        )
    }

    fn parse_exclusive_or_expression(&mut self) -> Result<Expression, CompileError> {
        // exclusive_or_expression
        //     : and_expression
        //     | exclusive_or_expression '^' and_expression
        //     ;
        self.parse_binary_expression(&[Punctuator::BitwiseXor], Self::parse_and_expression)
    }

    fn parse_and_expression(&mut self) -> Result<Expression, CompileError> {
        // and_expression
        //     : equality_expression
        //     | and_expression '&' equality_expression
        //     ;

        self.parse_binary_expression(&[Punctuator::BitwiseAnd], Self::parse_equality_expression)
    }

    fn parse_equality_expression(&mut self) -> Result<Expression, CompileError> {
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

    fn parse_relational_expression(&mut self) -> Result<Expression, CompileError> {
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

    fn parse_shift_expression(&mut self) -> Result<Expression, CompileError> {
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

    fn parse_additive_expression(&mut self) -> Result<Expression, CompileError> {
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

    fn parse_multiplicative_expression(&mut self) -> Result<Expression, CompileError> {
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
        parse_rhs: fn(&mut Self) -> Result<Expression, CompileError>,
    ) -> Result<Expression, CompileError> {
        let mut expression = parse_rhs(self)?;

        while let Some(token) = self.peek_token(0) {
            if let Token::Punctuator(punctuator) = token {
                if operator_punctuators.contains(punctuator) {
                    let operator = BinaryOp::from(token);
                    self.next_token(); // consume the operator
                    let rhs = parse_rhs(self)?;
                    expression = Expression::Binary(operator, Box::new(expression), Box::new(rhs));
                    continue;
                }
            }
            break;
        }

        Ok(expression)
    }

    fn parse_cast_expression(&mut self) -> Result<Expression, CompileError> {
        // cast_expression
        //     : unary_expression
        //     | '(' type_name ')' cast_expression
        //     ;

        todo!()
    }

    fn parse_unary_expression(&mut self) -> Result<Expression, CompileError> {
        // unary_expression
        //     : postfix_expression
        //     | INC_OP unary_expression
        //     | DEC_OP unary_expression
        //     | unary_operator cast_expression
        //     | SIZEOF unary_expression
        //     | SIZEOF '(' type_name ')'
        //     | ALIGNOF '(' type_name ')'
        //     ;

        // unary_operator
        //     : '&'
        //     | '*'
        //     | '+'
        //     | '-'
        //     | '~'
        //     | '!'
        //     ;

        todo!()
    }

    fn parse_postfix_expression(&mut self) -> Result<Expression, CompileError> {
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

        // argument_expression_list
        //     : assignment_expression
        //     | argument_expression_list ',' assignment_expression
        //     ;

        todo!()
    }

    fn parse_primary_expression(&mut self) -> Result<Expression, CompileError> {
        // primary_expression
        //     : IDENTIFIER
        //     | constant
        //     | string
        //     | '(' expression ')'
        //     | generic_selection
        //     | NULLPTR            /* C23: null-pointer constant */
        //     ;

        // constant
        //     : I_CONSTANT        /* includes character_constant */
        //     | F_CONSTANT
        //     | ENUMERATION_CONSTANT    /* after it has been defined as such */
        //     | TRUE            /* C23: boolean keyword */
        //     | FALSE            /* C23: boolean keyword */
        //     ;

        // enumeration_constant        /* before it has been defined as such */
        //     : IDENTIFIER
        //     ;

        // string
        //     : STRING_LITERAL
        //     | FUNC_NAME
        //     ;

        todo!()
    }

    fn parse_generic_selection(&mut self) -> Result<Expression, CompileError> {
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

        todo!()
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
