// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! CST (Concrete Syntax Tree) definitions for the ISO C23 standard (ISO/IEC 9899:2024).
//!
//! The top-level entry point for the entire translation unit is [`TranslationUnit`].
//! Binary expression precedence levels are collapsed into a single [`BinaryOp`] enum
//! to avoid deeply nested indirection; the parser is responsible for enforcing precedence.
//!
//! C23 additions over C11: attributes (`[[…]]`), `constexpr` storage class, `nullptr`,
//! `true`/`false` keywords, `typeof`/`typeof_unqual`, `_BitInt(N)`,
//! `_Decimal32/64/128`, fixed-underlying-type enums, empty initializer `= {}`,
//! optional `_Static_assert` message, labels as standalone block items,
//! and removal of K&R-style definitions.

// ─────────────────────────────────────────────────────────────────────────────
// § 1  Translation Unit (grammar root)
// ─────────────────────────────────────────────────────────────────────────────

/// Root of every compiled C file.
///
/// Corresponds to `translation_unit`.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslationUnit {
    pub declarations: Vec<ExternalDeclaration>,
}

/// A top-level item: either a function definition or a declaration.
///
/// Corresponds to `external_declaration`.
#[derive(Debug, Clone, PartialEq)]
pub enum ExternalDeclaration {
    Function(FunctionDefinition),
    Declaration(Declaration),
}

/// A complete function including its body.
///
/// Corresponds to `function_definition`.
/// C23 removes K&R-style definitions; a leading attribute-specifier-sequence is now allowed.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDefinition {
    /// C23: optional attribute-specifier-sequence before the declaration specifiers.
    pub attributes: Vec<Attribute>,
    pub specifiers: Vec<DeclarationSpecifier>,
    pub declarator: Declarator,
    pub body: Vec<BlockItem>,
}

// ─────────────────────────────────────────────────────────────────────────────
// § 2  Declarations
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to `declaration`.
#[derive(Debug, Clone, PartialEq)]
pub enum Declaration {
    /// `(attribute_specifier_sequence)? declaration_specifiers init_declarator_list? ';'`
    Var {
        /// C23: optional leading attributes on the declaration.
        attributes: Vec<Attribute>,
        specifiers: Vec<DeclarationSpecifier>,
        declarators: Vec<InitDeclarator>,
    },
    StaticAssert(StaticAssert),
    /// C23: standalone attribute declaration — `attribute_specifier_sequence ';'`.
    Attribute(Vec<Attribute>),
}

/// `STATIC_ASSERT '(' constant_expression (',' STRING_LITERAL)? ')' ';'`
///
/// C23: the message string is now optional (single-argument form).
#[derive(Debug, Clone, PartialEq)]
pub struct StaticAssert {
    pub expr: Box<Expression>,
    /// `None` for the C23 single-argument form `_Static_assert(expr);`.
    pub message: Option<String>,
}

/// One entry in an init-declarator list.
///
/// Corresponds to `init_declarator`.
#[derive(Debug, Clone, PartialEq)]
pub enum InitDeclarator {
    /// `declarator`
    Plain(Declarator),
    /// `declarator '=' initializer`
    Init(Declarator, Initializer),
}

// ─────────────────────────────────────────────────────────────────────────────
// § 3  Declaration specifiers
// ─────────────────────────────────────────────────────────────────────────────

/// One specifier in a `declaration_specifiers` list.
#[derive(Debug, Clone, PartialEq)]
pub enum DeclarationSpecifier {
    StorageClass(StorageClassSpecifier),
    Type(TypeSpecifier),
    Qualifier(TypeQualifier),
    Function(FunctionSpecifier),
    Alignment(AlignmentSpecifier),
}

/// Corresponds to `storage_class_specifier`.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageClassSpecifier {
    Typedef,
    Extern,
    Static,
    ThreadLocal,
    Auto,
    /// Deprecated in C23; behavior is implementation-defined.
    Register,
    /// C23: `constexpr` — object-level constant (may not be applied to functions).
    Constexpr,
}

/// Corresponds to `type_specifier`.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeSpecifier {
    Void,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    Signed,
    Unsigned,
    Bool,
    Complex,
    /// Non-mandated extension; kept from C11.
    Imaginary,
    /// C23: `_Decimal32` (IEEE 754-2008 decimal32).
    Decimal32,
    /// C23: `_Decimal64` (IEEE 754-2008 decimal64).
    Decimal64,
    /// C23: `_Decimal128` (IEEE 754-2008 decimal128).
    Decimal128,
    /// C23: `_BitInt(N)` — bit-precise integer of exactly N bits.
    BitInt(Box<Expression>),
    /// C23: `typeof(expr-or-type)` and `typeof_unqual(expr-or-type)`.
    Typeof(TypeofSpecifier),
    /// `ATOMIC '(' type_name ')'`
    Atomic(Box<TypeName>),
    StructOrUnion(StructOrUnionSpecifier),
    Enum(EnumSpecifier),
    /// A previously `typedef`-ed name.
    TypedefName(String),
}

/// Operand of the C23 `typeof` / `typeof_unqual` type specifier.
///
/// Corresponds to `typeof_specifier` in the C23 grammar.
/// When `unqual` is `true` the specifier was written as `typeof_unqual(…)`,
/// which strips all top-level qualifiers from the resulting type.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeofSpecifier {
    /// `typeof(expression)` or `typeof_unqual(expression)`.
    Expression {
        unqual: bool,
        expression: Box<Expression>,
    },
    /// `typeof(type_name)` or `typeof_unqual(type_name)`.
    Type {
        unqual: bool,
        type_name: Box<TypeName>,
    },
}

/// Corresponds to `type_qualifier`.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeQualifier {
    Const,
    Restrict,
    Volatile,
    /// `_Atomic` used as a qualifier (not as `_Atomic(T)` specifier).
    Atomic,
}

/// Corresponds to `function_specifier`.
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionSpecifier {
    Inline,
    Noreturn,
}

/// Corresponds to `alignment_specifier`.
#[derive(Debug, Clone, PartialEq)]
pub enum AlignmentSpecifier {
    /// `ALIGNAS '(' type_name ')'`
    Type(Box<TypeName>),
    /// `ALIGNAS '(' constant_expression ')'`
    Expression(Box<Expression>),
}

// ─────────────────────────────────────────────────────────────────────────────
// § 4  Struct / union
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to `struct_or_union_specifier`.
#[derive(Debug, Clone, PartialEq)]
pub struct StructOrUnionSpecifier {
    pub kind: StructOrUnion,
    /// C23: optional attributes between the `struct`/`union` keyword and the name or body.
    pub attributes: Vec<Attribute>,
    /// Present for named structs/unions.
    pub name: Option<String>,
    /// `None` when this is a forward reference (`struct Foo`).
    pub body: Option<Vec<StructDeclaration>>,
}

/// Corresponds to `struct_or_union`.
#[derive(Debug, Clone, PartialEq)]
pub enum StructOrUnion {
    Struct,
    Union,
}

/// One declaration inside a struct or union body.
///
/// Corresponds to `struct_declaration`.
///
/// C23 Annex A 6.7.3.1: an optional `attribute_specifier_sequence` may appear
/// before the `specifier_qualifier_list`.
#[derive(Debug, Clone, PartialEq)]
pub enum StructDeclaration {
    /// `(attribute_specifier_sequence)? specifier_qualifier_list struct_declarator_list? ';'`
    Field {
        /// C23: optional leading attributes on the member declaration.
        attributes: Vec<Attribute>,
        specifiers: Vec<SpecifierQualifier>,
        declarators: Vec<StructDeclarator>,
    },
    StaticAssert(StaticAssert),
}

/// One item in a `specifier_qualifier_list`.
///
/// C23 Annex A 6.7.3.1 adds `alignment_specifier` and `attribute_specifier_sequence`
/// as valid items alongside `type_specifier` and `type_qualifier`.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecifierQualifier {
    Type(TypeSpecifier),
    Qualifier(TypeQualifier),
    /// C23: `alignment_specifier` inside a `specifier_qualifier_list`
    /// (e.g., `alignas(8) int x;` inside a struct body).
    Alignment(AlignmentSpecifier),
    /// C23: `attribute_specifier_sequence` inside a `specifier_qualifier_list`.
    Attribute(Vec<Attribute>),
}

/// One declarator inside a struct declaration.
///
/// Corresponds to `struct_declarator`.
#[derive(Debug, Clone, PartialEq)]
pub enum StructDeclarator {
    /// `declarator`
    Plain(Declarator),
    /// `declarator? ':' constant_expression`  (bit-field)
    BitField {
        declarator: Option<Declarator>,
        width: Box<Expression>,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// § 5  Enum
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to `enum_specifier`.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumSpecifier {
    /// C23: optional attributes between `enum` and the name or body.
    pub attributes: Vec<Attribute>,
    pub name: Option<String>,
    /// C23: fixed underlying type — `enum E : int { … }`.
    /// `None` for classical (untyped) enumerations.
    pub underlying_type: Option<Vec<SpecifierQualifier>>,
    /// `None` for forward references (`enum Color`).
    pub variants: Option<Vec<Enumerator>>,
}

/// Corresponds to `enumerator`.
#[derive(Debug, Clone, PartialEq)]
pub struct Enumerator {
    pub name: String,
    /// C23: optional attributes on the enumerator name.
    pub attributes: Vec<Attribute>,
    /// Present when `enumeration_constant '=' constant_expression`.
    pub value: Option<Box<Expression>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// § 6  Declarators
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to `declarator`.
#[derive(Debug, Clone, PartialEq)]
pub struct Declarator {
    pub pointer: Option<Pointer>,
    pub direct: DirectDeclarator,
}

/// Corresponds to `pointer` (one or more `*` levels with optional qualifiers).
#[derive(Debug, Clone, PartialEq)]
pub struct Pointer {
    pub qualifiers: Vec<TypeQualifier>,
    /// Points to the next `*` level, if any.
    pub inner: Option<Box<Pointer>>,
}

/// Corresponds to `direct_declarator`.
#[derive(Debug, Clone, PartialEq)]
pub enum DirectDeclarator {
    /// A bare identifier.
    Identifier(String),
    /// `'(' declarator ')'`
    Parenthesized(Box<Declarator>),
    /// `direct_declarator '[' array_size ']'`
    Array(Box<DirectDeclarator>, ArraySize),
    /// `direct_declarator '(' function_params ')'`
    Function(Box<DirectDeclarator>, FunctionParams),
}

/// Encodes the many forms of array size expressions in `direct_declarator`.
#[derive(Debug, Clone, PartialEq)]
pub enum ArraySize {
    /// `[]`
    Unknown,
    /// `[*]`
    Vla,
    /// `[STATIC type_qualifier_list? assignment_expression]`
    /// The qualifiers come before or after STATIC depending on the production,
    /// but semantically they are the same.
    Static {
        qualifiers: Vec<TypeQualifier>,
        size: Box<Expression>,
    },
    /// `[type_qualifier_list]`  or  `[type_qualifier_list assignment_expression]`
    Qualified {
        qualifiers: Vec<TypeQualifier>,
        /// `None` when only qualifiers are present (no size expression).
        size: Option<Box<Expression>>,
    },
    /// `[assignment_expression]`
    Expression(Box<Expression>),
}

/// The parameter/identifier portion of a function declarator.
///
/// C23 removes K&R-style `identifier_list` forms.
/// The C23 lone-ellipsis form `void f(...)` is represented as
/// `TypeList(ParameterTypeList { params: vec![], variadic: true })`.
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionParams {
    /// `'(' parameter_type_list ')'`
    TypeList(ParameterTypeList),
    /// `'(' ')'`
    Empty,
}

/// Corresponds to `parameter_type_list`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterTypeList {
    pub params: Vec<ParameterDeclaration>,
    /// `true` when the list ends with `', ...'`.
    pub variadic: bool,
}

/// One parameter in a parameter list.
///
/// Corresponds to `parameter_declaration`.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterDeclaration {
    /// `declaration_specifiers declarator`
    Named {
        specifiers: Vec<DeclarationSpecifier>,
        declarator: Declarator,
    },
    /// `declaration_specifiers abstract_declarator?`
    Abstract {
        specifiers: Vec<DeclarationSpecifier>,
        declarator: Option<AbstractDeclarator>,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// § 7  Abstract declarators and type names
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to `type_name`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeName {
    pub specifiers: Vec<SpecifierQualifier>,
    pub declarator: Option<AbstractDeclarator>,
}

/// Corresponds to `abstract_declarator`.
#[derive(Debug, Clone, PartialEq)]
pub struct AbstractDeclarator {
    pub pointer: Option<Pointer>,
    pub direct: Option<DirectAbstractDeclarator>,
}

/// Corresponds to `direct_abstract_declarator`.
#[derive(Debug, Clone, PartialEq)]
pub enum DirectAbstractDeclarator {
    /// `'(' abstract_declarator ')'`
    Parenthesized(Box<AbstractDeclarator>),
    /// `direct_abstract_declarator? '[' array_size ']'`
    Array(Option<Box<DirectAbstractDeclarator>>, ArraySize),
    /// `direct_abstract_declarator? '(' parameter_type_list? ')'`
    Function(
        Option<Box<DirectAbstractDeclarator>>,
        Option<ParameterTypeList>,
    ),
}

// ─────────────────────────────────────────────────────────────────────────────
// § 8  Initializers
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to `initializer`.
#[derive(Debug, Clone, PartialEq)]
pub enum Initializer {
    /// A single expression: `assignment_expression`.
    Expression(Box<Expression>),
    /// A brace-enclosed list: `'{' initializer_list ','? '}'`.
    List(Vec<InitializerItem>),
    /// C23: empty initializer `= {}` — zero-initializes the entire object.
    Empty,
}

/// One entry in an `initializer_list`.
#[derive(Debug, Clone, PartialEq)]
pub struct InitializerItem {
    /// Optional `designation` (designator list followed by `'='`).
    pub designation: Vec<Designator>,
    pub initializer: Box<Initializer>,
}

/// One designator in a designation.
///
/// Corresponds to `designator`.
#[derive(Debug, Clone, PartialEq)]
pub enum Designator {
    /// `'[' constant_expression ']'`
    Index(Box<Expression>),
    /// `'.' IDENTIFIER`
    Member(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// § 9  Expressions
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to the entire expression hierarchy from `primary_expression` up through `expression`:
///
/// - `primary_expression` (identifiers, constants, string literals, generic selections)
/// - `postfix_expression` (calls, member access, array subscripting, etc.)
/// - `unary_expression` (prefix operators, sizeof, alignof, `++`/`--` before the operand, etc.)
/// - `cast_expression` (type casts `(T)expression`)
/// - `binary_expression` (all binary operators, with precedence levels folded in)
///   - `multiplicative_expression` (`*`, `/`, `%`)
///   - `additive_expression` (`+`, `-`)
///   - `shift_expression` (`<<`, `>>`)
///   - `relational_expression` (`<`, `>`, `<=`, `>=`)
///   - `equality_expression` (`==`, `!=`)
///   - `and_expression` (`&`)
///   - `exclusive_or_expression` (`^`)
///   - `inclusive_or_expression` (`|`)
///   - `logical_and_expression`
///   - `logical_or_expression`
/// - `conditional_expression` (ternary `?:`)
/// - `assignment_expression` (`conditional_expression` | `unary_expression assignment_operator assignment_expression`)
/// - `expression` (comma operator)
///
/// Binary expression precedence levels (multiplicative … logical-or)
/// are folded into `Binary { op, lhs, rhs }` to reduce indirection;
/// the parser is responsible for building the tree in the correct shape.
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    // ── primary ────────────────────────────────────────────────────────────
    /// `IDENTIFIER`
    Identifier(String),
    /// `I_CONSTANT | F_CONSTANT | character_constant | ENUMERATION_CONSTANT | true | false`
    Constant(Constant),
    /// `STRING_LITERAL | FUNC_NAME`
    String(StringLiteral),
    /// `GENERIC '(' assignment_expression ',' generic_assoc_list ')'`
    Generic(Box<GenericSelection>),
    /// C23: `nullptr` — null-pointer constant of type `nullptr_t`.
    Nullptr,

    // ── postfix ────────────────────────────────────────────────────────────
    /// `postfix_expression '[' expression ']'`
    Index(Box<Expression>, Box<Expression>),
    /// `postfix_expression '(' argument_expression_list? ')'`
    Call(Box<Expression>, Vec<Expression>),
    /// `postfix_expression '.' IDENTIFIER`
    Member(Box<Expression>, String),
    /// `postfix_expression PTR_OP IDENTIFIER`
    ArrowMember(Box<Expression>, String),
    /// `postfix_expression INC_OP`
    PostIncrement(Box<Expression>),
    /// `postfix_expression DEC_OP`
    PostDecrement(Box<Expression>),
    /// C23 N3038: `'(' (storage_class_specifier)? type_name ')' '{' items? '}'`
    ///
    /// A storage-class specifier may appear before the type name inside the
    /// parentheses.  Allowed specifiers: `static`, `register`, `thread_local`,
    /// `constexpr`, `auto`.  The standard validates this combination
    /// semantically; the grammar accepts any `storage_class_specifier`.
    CompoundLiteral {
        /// C23 N3038: optional storage class inside `( )`. `None` for the
        /// classic `(T){ … }` form that was valid since C99.
        storage: Option<StorageClassSpecifier>,
        type_name: TypeName,
        /// Items from the initializer list; empty when using `= {}`.
        items: Vec<InitializerItem>,
    },

    // ── unary ──────────────────────────────────────────────────────────────
    /// `INC_OP unary_expression`
    PreIncrement(Box<Expression>),
    /// `DEC_OP unary_expression`
    PreDecrement(Box<Expression>),
    /// `unary_operator cast_expression`
    Unary(UnaryOp, Box<Expression>),
    /// `SIZEOF unary_expression` or `SIZEOF '(' type_name ')'`
    Sizeof(SizeofOperand),
    /// `ALIGNOF '(' type_name ')'`
    Alignof(TypeName),

    // ── cast ───────────────────────────────────────────────────────────────
    /// `'(' type_name ')' cast_expression`
    Cast(TypeName, Box<Expression>),

    // ── binary (all precedence levels) ─────────────────────────────────────
    Binary(BinaryOp, Box<Expression>, Box<Expression>),

    // ── conditional ────────────────────────────────────────────────────────
    /// `logical_or_expression '?' expression ':' conditional_expression`
    Conditional(Box<Expression>, Box<Expression>, Box<Expression>),

    // ── assignment ─────────────────────────────────────────────────────────
    /// `unary_expression assignment_operator assignment_expression`
    Assign(AssignOp, Box<Expression>, Box<Expression>),

    // ── comma ──────────────────────────────────────────────────────────────
    /// `expression ',' assignment_expression`
    /// It is recommended that only use comma operators in the `for` loop initializer and step expressions,
    /// but the grammar allows them anywhere.
    Comma(Box<Expression>, Box<Expression>),
}

// ─────────────────────────────────────────────────────────────────────────────
// § 10  Leaf / operator enums used by Expression
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to `constant`.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    /// `I_CONSTANT` (integer literal, including character constants).
    Integer(String),
    /// `F_CONSTANT` (floating-point literal).
    Float(String),
    /// `ENUMERATION_CONSTANT`
    Enumeration(String),
    /// C23: `true` or `false` keywords (boolean constants).
    Bool(bool),
}

/// Corresponds to `string`.
#[derive(Debug, Clone, PartialEq)]
pub enum StringLiteral {
    /// `STRING_LITERAL` (may be the result of concatenation).
    Literal(String),
    /// `FUNC_NAME` (`__func__`).
    FuncName,
}

/// Corresponds to `generic_selection`.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericSelection {
    /// C23: the controlling operand may be an expression or a type name.
    pub controlling: GenericControlling,
    pub associations: Vec<GenericAssociation>,
}

/// The controlling operand of a `_Generic` selection expression.
///
/// Corresponds to `generic_controlling_operand` in the C23 grammar (6.5.1.1).
/// In C11 only an `assignment_expression` was valid; C23 also allows a bare
/// type name, enabling patterns like `_Generic(int, int: "int", ...)`.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericControlling {
    /// `assignment_expression` (valid since C11)
    Expression(Box<Expression>),
    /// C23: type-name as the controlling operand.
    Type(TypeName),
}

/// Corresponds to `generic_association`.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericAssociation {
    /// `type_name ':' assignment_expression`
    Type(TypeName, Box<Expression>),
    /// `DEFAULT ':' assignment_expression`
    Default(Box<Expression>),
}

/// The operand of a `sizeof` expression.
#[derive(Debug, Clone, PartialEq)]
pub enum SizeofOperand {
    /// `sizeof unary_expression`
    Expression(Box<Expression>),
    /// `sizeof '(' type_name ')'`
    Type(TypeName),
}

/// Corresponds to `unary_operator`.
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    /// `'&'` — address-of
    AddressOf,
    /// `'*'` — dereference
    Deref,
    /// `'+'` — unary plus
    Plus,
    /// `'-'` — unary minus
    Minus,
    /// `'~'` — bitwise NOT
    BitwiseNot,
    /// `'!'` — logical NOT
    LogicalNot,
}

/// All binary operators, spanning every precedence level from
/// multiplicative through logical-or.
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    // multiplicative_expression
    Mul,
    Div,
    Mod,
    // additive_expression
    Add,
    Sub,
    // shift_expression
    Shl,
    Shr,
    // relational_expression
    Lt,
    Gt,
    Le,
    Ge,
    // equality_expression
    Eq,
    Ne,
    // and_expression
    BitAnd,
    // exclusive_or_expression
    BitXor,
    // inclusive_or_expression
    BitOr,
    // logical_and_expression
    LogAnd,
    // logical_or_expression
    LogOr,
}

/// Corresponds to `assignment_operator`.
#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    /// `'='`
    Assign,
    /// `'*='`
    MulAssign,
    /// `'/='`
    DivAssign,
    /// `'%='`
    ModAssign,
    /// `'+='`
    AddAssign,
    /// `'-='`
    SubAssign,
    /// `'<<='`
    ShlAssign,
    /// `'>>='`
    ShrAssign,
    /// `'&='`
    AndAssign,
    /// `'^='`
    XorAssign,
    /// `'|='`
    OrAssign,
}

// ─────────────────────────────────────────────────────────────────────────────
// § 11  Statements
// ─────────────────────────────────────────────────────────────────────────────

/// Corresponds to `statement`.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Labeled(LabeledStatement),

    // `'{' block_item_list '}'` or `'{' '}'`
    Compound(Vec<BlockItem>),

    // `;` (empty statement) or `expression ';'`
    Expression(Option<Expression>),

    // `if` / `switch`
    Selection(Box<SelectionStatement>),

    // `while` / `do-while` / `for`
    Iteration(Box<IterationStatement>),

    // `goto` / `continue` / `break` / `return`
    Jump(JumpStatement),
}

/// A C23 label, separated from the statement that follows it.
///
/// Corresponds to the `label` non-terminal introduced in C23 so that a label
/// can appear as a standalone `block_item` before a declaration or before `}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    /// C23: optional attributes on the label (e.g. `[[maybe_unused]]`).
    pub attributes: Vec<Attribute>,
    pub kind: LabelKind,
}

/// The kind of a label.
#[derive(Debug, Clone, PartialEq)]
pub enum LabelKind {
    /// `IDENTIFIER ':'`
    Named(String),
    /// `CASE constant_expression ':'`
    Case(Box<Expression>),
    /// `DEFAULT ':'`
    Default,
}

/// A label immediately followed by a statement.
///
/// Corresponds to `labeled_statement`.
#[derive(Debug, Clone, PartialEq)]
pub struct LabeledStatement {
    pub label: Label,
    pub statement: Box<Statement>,
}

/// One item in a `block_item_list`.
///
/// C23 extends this to allow standalone labels before declarations or at the
/// end of a compound statement.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockItem {
    Declaration(Declaration),
    Statement(Statement),
    /// C23: a label not immediately followed by a statement in the same block item
    /// (e.g. a label before a declaration, or immediately before `}`).
    Label(Label),
}

/// Corresponds to `selection_statement`.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionStatement {
    /// `IF '(' expression ')' statement (ELSE statement)?`
    If {
        cond: Box<Expression>,
        then: Box<Statement>,
        else_: Option<Box<Statement>>,
    },
    /// `SWITCH '(' expression ')' statement`
    Switch {
        cond: Box<Expression>,
        body: Box<Statement>,
    },
}

/// Corresponds to `iteration_statement`.
#[derive(Debug, Clone, PartialEq)]
pub enum IterationStatement {
    /// `WHILE '(' expression ')' statement`
    While {
        cond: Box<Expression>,
        body: Box<Statement>,
    },
    /// `DO statement WHILE '(' expression ')' ';'`
    DoWhile {
        body: Box<Statement>,
        cond: Box<Expression>,
    },
    /// `FOR '(' for_init expression_statement expression? ')' statement`
    For {
        init: ForInit,
        cond: Option<Box<Expression>>,
        step: Option<Box<Expression>>,
        body: Box<Statement>,
    },
}

/// The initializer clause of a `for` statement.
#[derive(Debug, Clone, PartialEq)]
pub enum ForInit {
    /// `expression_statement` (may be empty)
    Expression(Option<Expression>),
    /// `declaration`
    Declaration(Declaration),
}

/// Corresponds to `jump_statement`.
#[derive(Debug, Clone, PartialEq)]
pub enum JumpStatement {
    /// `GOTO IDENTIFIER ';'`
    Goto(String),
    /// `CONTINUE ';'`
    Continue,
    /// `BREAK ';'`
    Break,
    /// `RETURN expression? ';'`
    Return(Option<Box<Expression>>),
}

// ─────────────────────────────────────────────────────────────────────────────
// § 0  Attributes  (C23)
// ─────────────────────────────────────────────────────────────────────────────

/// A single attribute entry inside an `[[...]]` specifier.
///
/// Corresponds to one item in `attribute_list`.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub token: AttributeToken,
    /// Raw balanced-token argument string; `None` when no `(...)` clause appears.
    pub args: Option<String>,
}

/// The name portion of an attribute, with or without a namespace prefix.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeToken {
    /// `identifier` — e.g. `nodiscard`, `deprecated`, `fallthrough`.
    Simple(String),
    /// `identifier '::' identifier` — e.g. `gnu::noinline`, `clang::no_sanitize`.
    Namespaced { prefix: String, name: String },
}
