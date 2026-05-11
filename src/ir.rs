// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! This is Cranelift IR AST, It can be used for code generation through Cranelift's frontend API.
//!
//! Frontend example:
//! https://docs.rs/cranelift-frontend/latest/cranelift_frontend/
//!
//! There are several main APIs for constructing Cranelift IR:
//!
//! - `cranelift_module::Module`: https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html
//! - `cranelift_frontend::FunctionBuilder`: https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html
//! - `cranelift_codegen::ir::InstBuilder`: https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/trait.InstBuilder.html

// ------------------------------------
// Abstract Syntax Tree for Cranelift IR
// ------------------------------------

// Node Sections:
//
// - Top-level file structure
// - Entity references — all 13 newtype-u32 indices (`Value`, `Block`, `StackSlot`, ...)
// - Type system — scalars, floats, SIMD vectors, dynamic vectors
// - Immediates — `Imm64`, `Uimm64`, `Uimm8`, `Offset32`, `Ieee16/32/64/128`, `ConstantData`
// - Enumerations — `IntCC`, `FloatCC`, `TrapCode`, `AtomicRmwOp`, `MemFlags`, `CallConv`, `ArgumentExtension`, `ArgumentPurpose`
// - Function/external names — `UserFuncName`, `ExternalName`, `UserExternalName`
// - Call signatures — `Signature`, `AbiParam`
// - Composite operands — `BlockCall`, `BlockArg`, `JumpTableData`, `ExceptionTableData`
// - Global values — `GlobalValueData` (5 variants)
// - Stack slots — `StackSlotData`, `DynamicStackSlotData`, `DynamicTypeData`
// - Preamble declarations — all 8 declaration kinds
// - Function/basic-block/instruction nodes
// - `InstructionData` — 26 variants, one per `InstructionFormat`, with opcodes listed per variant
// - `Opcode` enum — complete alphabetical listing (~200 opcodes, grouped by category)
// - Annotated example function

// ------------------------------------
// TOP-LEVEL FILE STRUCTURE
// ------------------------------------

pub struct Module {
    // imported function declarations
    // imported data declarations
    // read-only data definitions (.rodata)
    // read-write data definitions (.data)
    // uninitialized data definitions (.bss)
    pub functions: Vec<FunctionDef>,
}

// ------------------------------------
// ENTITY REFERENCES  (newtype u32 indices)
// ------------------------------------
//
// Each entity reference is a distinct newtype over u32, printed as a sigilled
// decimal: v42 (Value), block7 (Block), ss3 (StackSlot), etc.

type Value = u32; // v0, v1, ...            -> SSA values produced by instructions
type Block = u32; // block0, ...            -> Basic-block labels
type Inst = u32; // inst5, ...              -> Instruction identifiers (used rarely in text)
type StackSlot = u32; // ss0, ss1, ...      -> Static stack slot references
type DynamicStackSlot = u32; // dss0, ...   -> Dynamic-sized stack slot references
type DynamicType = u32; // dt0, ...         -> Dynamic SIMD type references
type GlobalValue = u32; // gv0, gv1, ...    -> Global-value references
type Constant = u32; // const0, ...         -> Constant-pool references
type SigRef = u32; // sig0, sig1, ...       -> Call-signature references
type FuncRef = u32; // fn0, fn1, ...        -> Function references
type JumpTable = u32; // jt0, jt1, ...      -> Jump-table references
type ExceptionTable = u32; // et0, ...      -> Exception-table references
type ExceptionTag = u32; // etag0, ...      -> Exception-tag references
type UserExternalNameRef = u32; // (internal index into external-name table)
type Immediate = u32; // (index into immediate pool, used by Shuffle)

// Entity reference:
// https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/entities/index.html
//
// There are some entities are not quite straightforward, here are the links to their documentation:
//
// - `SigRef`
//   https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/entities/struct.SigRef.html
// - `Signature`
//   https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/struct.Signature.html
// - `FuncRef`
//   https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/entities/struct.FuncRef.html
// - `GlobalValue`
//   https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/entities/struct.GlobalValue.html
// - `Immediate`
//   https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/entities/struct.Immediate.html
// - `UserExternalNameRef`
//   https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/entities/struct.UserExternalNameRef.html

// ------------------------------------
// TYPES
// ------------------------------------

pub enum Type {
    // ---- scalar integer types
    I8,   // 8-bit integer
    I16,  // 16-bit integer
    I32,  // 32-bit integer
    I64,  // 64-bit integer
    I128, // 128-bit integer

    // ---- scalar floating-point types
    F16,  // 16-bit float (IEEE 754-2008 half)
    F32,  // 32-bit float (IEEE 754 single)
    F64,  // 64-bit float (IEEE 754 double)
    F128, // 128-bit float (IEEE 754 quad)

    // ---- reference types
    R32, // 32-bit GC reference
    R64, // 64-bit GC reference

    // ---- SIMD vector types: NxT  (N lanes of type T)
    // Valid combinations: 2×I8, 4×I8, 8×I8, 16×I8,
    //                     2×I16, 4×I16, 8×I16,
    //                     2×I32, 4×I32, 2×I64, 4×I64,
    //                     2×F16, 4×F16, 8×F16,
    //                     2×F32, 4×F32, 2×F64, 4×F64,
    //                     4×F128, ...  (all up to 128-bit total width)
    Vector {
        lanes: u16,
        lane_type: ScalarType,
    },

    // ---- dynamic vector types (SIMD with runtime-determined lane count) ─────
    DynamicVector {
        dt: DynamicType,
        lane_type: ScalarType,
    },

    // ---- no-value placeholder (used internally, never appears in CLIF text) ─
    Void,
}

// Scalar component of a vector lane (same variants as above minus Vector).
pub enum ScalarType {
    I8,
    I16,
    I32,
    I64,
    F16,
    F32,
    F64,
    F128,
}

// ------------------------------------
// IMMEDIATES
// ------------------------------------

// Signed 64-bit integer immediate.  Printed as decimal or 0x... hex.
pub struct Imm64 {
    pub value: i64,
}

// Unsigned 64-bit integer immediate.
struct Uimm64 {
    value: u64,
}

// Unsigned 8-bit integer immediate (used for lane indices and shift amounts).
type Uimm8 = u8;

// Unsigned 32-bit integer immediate (e.g. StructArgument ABI purpose size).
type Uimm32 = u32;

// Signed byte-offset into a stack slot or memory operand (fits in i32).
pub struct Offset32 {
    pub value: i32, // printed "+N" / "-N" / empty if zero
}

// IEEE 754 half-precision floating-point constant.
pub struct Ieee16 {
    pub bits: u16, // printed as hex: 0xABCD
}

// IEEE 754 single-precision floating-point constant.
pub struct Ieee32 {
    pub bits: u32, // printed as 0x... or decimal
}

// IEEE 754 double-precision floating-point constant.
pub struct Ieee64 {
    pub bits: u64, // printed as 0x... or decimal
}

// IEEE 754 quad-precision floating-point constant.
struct Ieee128 {
    bits: u128,
}

// Raw constant-pool data (byte array, up to 128 bytes / 1024 bits).
// Stored in the function's constant pool, referenced by Constant index.
pub struct ConstantData {
    pub bytes: Vec<u8>,
}

// ------------------------------------
// ENUMERATIONS
// ------------------------------------

// ---- Integer condition codes
pub enum IntCC {
    Equal,                      // "eq"
    NotEqual,                   // "ne"
    SignedLessThan,             // "slt"
    SignedLessThanOrEqual,      // "sle"
    SignedGreaterThan,          // "sgt"
    SignedGreaterThanOrEqual,   // "sge"
    UnsignedLessThan,           // "ult"
    UnsignedLessThanOrEqual,    // "ule"
    UnsignedGreaterThan,        // "ugt"
    UnsignedGreaterThanOrEqual, // "uge"
}

// ---- Float condition codes (IEEE 754 ordered/unordered comparisons)
pub enum FloatCC {
    Ordered,                       // "ord"  — neither operand is NaN
    Unordered,                     // "uno"  — at least one operand is NaN
    Equal,                         // "eq"   — ordered equal
    NotEqual,                      // "ne"   — unordered not-equal
    OrderedNotEqual,               // "one"
    UnorderedOrEqual,              // "ueq"
    LessThan,                      // "lt"
    LessThanOrEqual,               // "le"
    GreaterThan,                   // "gt"
    GreaterThanOrEqual,            // "ge"
    UnorderedOrLessThan,           // "ult"
    UnorderedOrLessThanOrEqual,    // "ule"
    UnorderedOrGreaterThan,        // "ugt"
    UnorderedOrGreaterThanOrEqual, // "uge"
}

// ---- Trap reason codes
pub enum TrapCode {
    StackOverflow,          // "stk_ovf"
    HeapOutOfBounds,        // "heap_oob"
    IntegerOverflow,        // "int_ovf"
    IntegerDivisionByZero,  // "int_divz"
    BadConversionToInteger, // "bad_toint"
    NullReference,          // "null_ref"
    UnreachableCodeReached, // "unreachable"
    User(u32),              // "user<N>"   — embedder-defined code
}

// ---- Atomic read-modify-write operations
pub enum AtomicRmwOp {
    Add,  // "add"
    Sub,  // "sub"
    And,  // "and"
    Nand, // "nand"
    Or,   // "or"
    Xor,  // "xor"
    Xchg, // "xchg"
    Umin, // "umin"
    Umax, // "umax"
    Smin, // "smin"
    Smax, // "smax"
}

// ---- Memory access flags
// These are packed into a u16 bitfield in the implementation.
pub struct MemFlags {
    pub aligned: bool,                     // access is naturally aligned
    pub readonly: bool,                    // pointer is read-only
    pub endian: Endianness,                // byte order (default = native)
    pub alias_region: Option<AliasRegion>, // helps alias analysis
    pub trap_code: Option<TrapCode>,       // trap on fault (if not set -> UB)
    pub can_move: bool,                    // allowed to hoist the access
}

pub enum Endianness {
    Native,
    Little,
    Big,
}
pub enum AliasRegion {
    Heap,
    Table,
    Vmctx,
}

// ---- Calling conventions
pub enum CallConv {
    Fast,            // Cranelift-internal fast convention
    Cold,            // For cold paths, callee-saves fewer registers
    Tail,            // Tail-call-optimized convention
    SystemV,         // Linux/macOS x86-64 & aarch64 System V ABI
    WindowsFastcall, // Windows x64
    AppleAarch64,    // Apple Silicon (macOS M-series)
    Probestack,      // Stack-overflow-safe probe convention
    WasmtimeSystemV,
    WasmtimeFastcall,
    WasmtimeAppleAarch64,
    // ... additional Wasmtime-specific variants
}

// ---- ABI parameter/return extension
pub enum ArgumentExtension {
    None, // no sign/zero extension
    Uext, // zero-extend to register width
    Sext, // sign-extend to register width
}

// ---- ABI parameter purpose
pub enum ArgumentPurpose {
    Normal,                 // ordinary value argument/return
    StructArgument(Uimm32), // pointer to struct; size in bytes
    StructReturn,           // pointer to return struct area
    Link,                   // link register (return address)
    FPReg,                  // frame-pointer register
    CalleeSaved,            // callee-saved register pass-through
    VMContext,              // Wasmtime VM context pointer
    SignatureId,            // polymorphic call-site signature id
    StackLimit,             // stack limit pointer
}

// ------------------------------------
// FUNCTION NAME & EXTERNAL NAMES
// ------------------------------------

// Name attached to the function definition ("%foo" or "u0:0").
pub enum UserFuncName {
    User { namespace: u32, index: u32 }, // "u<ns>:<idx>"  (UserExternalName)
    Testcase(String),                    // "%identifier"
}

// External function/symbol name (used in FuncRef and GlobalValue decls).
pub enum ExternalName {
    User(UserExternalName), // "u<ns>:<idx>"
    TestCase(String),       // "%foo"
    LibCall(String),        // known library call name
    KnownSymbol(String),    // well-known platform symbol
}

pub struct UserExternalName {
    pub namespace: u32,
    pub index: u32,
}

// ------------------------------------
// CALL SIGNATURES
// ------------------------------------

pub struct Signature {
    pub params: Vec<AbiParam>,
    pub returns: Vec<AbiParam>,
    pub call_conv: CallConv,
}

pub struct AbiParam {
    pub value_type: Type,
    pub extension: ArgumentExtension,
    pub purpose: ArgumentPurpose,
}

// ------------------------------------
// COMPOSITE INSTRUCTION OPERANDS
// ------------------------------------

// A block reference with optional arguments (used in branch/jump targets).
pub struct BlockCall {
    pub block: Block,
    pub args: Vec<BlockArg>,
}

// An argument to a block call; can be a value or a "try_call" return slot.
pub enum BlockArg {
    Value(Value),    // normal SSA value
    TryCallRet(u32), // result<N> — Nth normal-return value slot
    TryCallExn(u32), // exn<N>    — Nth exception-payload value slot
}

// Jump-table, exception-table are not used in the C programs.
//
// ```rust
// // Jump-table data stored in the function's jump-table pool.
// struct JumpTableData {
//     // table[0] is the default block (taken when index is out-of-range).
//     // table[1..] are indexed entries.
//     table: Vec<BlockCall>,
// }

// // Exception-landing-pad table attached to a try_call instruction.
// struct ExceptionTableData {
//     sig: SigRef,                    // signature of exception payload
//     normal_return: BlockCall,       // block for normal (non-exceptional) return
//     items: Vec<ExceptionTableItem>, // exception dispatch entries
// }

// enum ExceptionTableItem {
//     // if thrown tag matches, jump to target
//     Tag {
//         tag: ExceptionTag,
//         target: BlockCall,
//     },
//     // catch-all landing pad
//     Default {
//         target: BlockCall,
//     },
//     // pass exception context value
//     Context(Value),
// }
// ```

// ------------------------------------
// GLOBAL VALUES
// ------------------------------------

// GlobalValueData describes how a global value is computed at runtime.
pub enum GlobalValueData {
    VMContext,
    // Load a value from memory at (base + offset), typed as global_type.
    Load {
        base: GlobalValue,
        offset: Offset32,
        global_type: Type,
        flags: MemFlags,
    },
    // Integer-add an immediate to a base global value.
    IAddImm {
        base: GlobalValue,
        offset: Imm64,
        global_type: Type,
    },
    // Symbol whose address is resolved by the linker/loader.
    Symbol {
        name: ExternalName,
        offset: Imm64,
        colocated: bool, // defined in the same linkage unit
        tls: bool,       // thread-local storage
    },
    // Scale factor for a dynamic SIMD vector type.
    DynScaleTargetConst {
        vector_type: Type,
    },
}

// ------------------------------------
// STACK SLOTS
// ------------------------------------

pub struct StackSlotData {
    pub kind: StackSlotKind,
    pub size: u32,          // byte size
    pub align_shift: u8,    // log2 of required alignment (0 -> 1-byte)
    pub index: Option<u64>, // optional disambiguation key
}

pub struct DynamicStackSlotData {
    pub kind: StackSlotKind,
    pub dyn_ty: DynamicType,
}

pub enum StackSlotKind {
    ExplicitSlot,        // "explicit_slot"  — programmer-visible local
    ExplicitDynamicSlot, // "explicit_dynamic_slot"
}

// Dynamic SIMD type: the vector type and scale factor (from a GlobalValue).
pub struct DynamicTypeData {
    pub base_vector_ty: Type,       // known fixed-width vector element type
    pub dynamic_scale: GlobalValue, // runtime scale factor
}

// ------------------------------------
//  PREAMBLE DECLARATIONS
// ------------------------------------

// The preamble of a function body lists all entities referenced by name.
pub enum PreambleDecl {
    // "ss<N> = ..."
    StackSlot {
        id: StackSlot,
        data: StackSlotData,
    },
    // "dss<N> = ..."
    DynStackSlot {
        id: DynamicStackSlot,
        data: DynamicStackSlotData,
    },
    // "dt<N> = ..."
    DynType {
        id: DynamicType,
        data: DynamicTypeData,
    },
    // "gv<N> = ..."
    GlobalValue {
        id: GlobalValue,
        data: GlobalValueData,
    },
    // "sig<N> = ..."
    Sig {
        id: SigRef,
        sig: Signature,
    },
    // "fn<N> = [colocated] [patchable] <name> sig<M>"
    Func {
        id: FuncRef,
        colocated: bool,
        patchable: bool,
        name: ExternalName,
        sig: SigRef,
    },
    // "const<N> = 0x<hex>"
    ConstPool {
        id: Constant,
        data: ConstantData,
    },
    // "stack_limit = gv<N>"
    StackLimit {
        gv: GlobalValue,
    },
}

// ------------------------------------
// FUNCTION DEFINITION
// ------------------------------------

// A complete CLIF function (corresponds to ir::Function).
pub struct FunctionDef {
    pub name: UserFuncName,
    pub signature: Signature,
    pub preamble: Vec<PreambleDecl>,
    pub blocks: Vec<BasicBlock>,
}

// An extended-basic-block / block node.
pub struct BasicBlock {
    pub id: Block,
    pub cold: bool,              // "; cold" attribute on the block header
    pub params: Vec<BlockParam>, // "(v0: i32, v1: f64, ...)"
    pub body: Vec<Instruction>,
}

// A formal parameter declared by a block (SSA block parameter = Φ-equivalent).
pub struct BlockParam {
    pub value: Value,
    pub ty: Type,
}

// ------------------------------------
// INSTRUCTION NODE
// ------------------------------------

pub struct Instruction {
    pub srcloc: Option<SourceLoc>, // "@src_loc" annotation
    pub debug_tags: Vec<DebugTag>, // ";; debug_tag ..." annotations
    pub results: Vec<Value>,       // LHS value definitions (may be 0, 1, or 2)
    pub data: InstructionData,     // opcode + format-specific operands
}

// Source-location annotation (packed u32 offset/id assigned by the frontend).
pub struct SourceLoc {
    pub raw: u32,
}

// Debug tag attached (via sequence_point) to mark a source position or label.
pub enum DebugTag {
    Label(String),
    // ...further variants as the IR evolves
}

// ------------------------------------
// INSTRUCTION DATA  (one variant per InstructionFormat)
// ------------------------------------
//
// Each variant includes:
//   • The opcode (determines semantics and result types)
//   • Format-specific immediates and operand values
//
// The result count and types are determined by the opcode + optional
// controlling type variable (ctrl_typevar), not stored explicitly here.

pub enum InstructionData {
    // ---- NullAry
    // No value operands, no immediates.
    // Opcodes: nop, debugtrap, fence, sequence_point,
    //          get_frame_pointer, get_stack_pointer,
    //          get_return_address, get_pinned_reg
    NullAry {
        opcode: Opcode,
    },

    // ---- Unary
    // One value input, one value result (or two for isplit).
    // Opcodes: bnot, ineg, iabs, popcnt, bitrev, clz, cls, ctz, bswap,
    //          fneg, fabs, sqrt, ceil, floor, nearest, trunc,
    //          splat, vhigh_bits, vany_true, vall_true,
    //          swiden_low, swiden_high, uwiden_low, uwiden_high,
    //          isplit (-> 2 results), bmask,
    //          ireduce, uextend, sextend, fdemote, fpromote,
    //          fcvt_to_uint, fcvt_to_sint,
    //          fcvt_to_uint_sat, fcvt_to_sint_sat,
    //          fcvt_from_uint, fcvt_from_sint,
    //          scalar_to_vector, x86_cvtt2dq
    Unary {
        opcode: Opcode,
        arg: Value,
    },

    // ---- UnaryImm
    // Integer constant.  One Imm64 immediate, no value inputs.
    // Opcodes: iconst
    UnaryImm {
        opcode: Opcode,
        imm: Imm64,
    },

    // ---- UnaryIeee16
    // Opcodes: f16const
    UnaryIeee16 {
        opcode: Opcode,
        imm: Ieee16,
    },

    // ---- UnaryIeee32
    // Opcodes: f32const
    UnaryIeee32 {
        opcode: Opcode,
        imm: Ieee32,
    },

    // ---- UnaryIeee64
    // Opcodes: f64const
    UnaryIeee64 {
        opcode: Opcode,
        imm: Ieee64,
    },

    // ---- UnaryConst
    // References a constant in the function's constant pool.
    // Opcodes: vconst, f128const
    UnaryConst {
        opcode: Opcode,
        constant_handle: Constant,
    },

    // ---- UnaryGlobalValue
    // Loads or computes an address from the global-value table.
    // Opcodes: global_value, symbol_value, tls_value
    UnaryGlobalValue {
        opcode: Opcode,
        global_value: GlobalValue,
    },

    // ---- Binary
    // Two value inputs, one value result.
    // Opcodes: iadd, isub, imul, udiv, sdiv, urem, srem,
    //          band, bor, bxor, band_not, bor_not, bxor_not,
    //          rotl, rotr, ishl, ushr, sshr,
    //          fadd, fsub, fmul, fdiv, fcopysign, fmin, fmax,
    //          fmin_pseudo, fmax_pseudo,
    //          smin, umin, smax, umax, avg_round,
    //          uadd_sat, sadd_sat, usub_sat, ssub_sat,
    //          umulhi, smulhi, sqmul_round_sat,
    //          uadd_overflow (-> 2 results), sadd_overflow (-> 2),
    //          usub_overflow (-> 2), ssub_overflow (-> 2),
    //          smul_overflow (-> 2), umul_overflow (-> 2),
    //          iconcat, iadd_pairwise,
    //          unarrow, snarrow, swizzle,
    //          x86_pshufb, x86_pmulhrsw, x86_pmaddubsw
    Binary {
        opcode: Opcode,
        args: [Value; 2],
    },

    // ---- BinaryImm8
    // One value input, one 8-bit unsigned immediate.
    // Opcodes: extractlane, extract_vector
    BinaryImm8 {
        opcode: Opcode,
        imm: Uimm8,
        arg: Value,
    },

    // ---- BinaryImm64
    // One value input, one signed 64-bit immediate.
    // Opcodes: iadd_imm, imul_imm, udiv_imm, sdiv_imm, urem_imm, srem_imm,
    //          irsub_imm, band_imm, bor_imm, bxor_imm,
    //          rotl_imm, rotr_imm, ishl_imm, ushr_imm, sshr_imm
    BinaryImm64 {
        opcode: Opcode,
        imm: Imm64,
        arg: Value,
    },

    // ---- Ternary
    // Three value inputs, one value result (or more for overflow variants).
    // Opcodes: fma, select, select_spectre_guard, bitselect, blendv,
    //          sadd_overflow_cin (-> 2 results), uadd_overflow_cin (-> 2),
    //          usub_overflow_bin (-> 2),
    //          stack_switch
    Ternary {
        opcode: Opcode,
        args: [Value; 3],
    },

    // ---- TernaryImm8
    // Two value inputs, one 8-bit unsigned immediate.
    // Opcodes: insertlane
    TernaryImm8 {
        opcode: Opcode,
        imm: Uimm8,
        args: [Value; 2],
    },

    // ---- MultiAry
    // Variable number of value inputs, no immediates.
    // Opcodes: return_
    MultiAry {
        opcode: Opcode,
        args: Vec<Value>,
    },

    // ---- Jump
    // Unconditional branch to a block with optional arguments.
    // Opcodes: jump
    Jump {
        opcode: Opcode,
        destination: BlockCall,
    },

    // ---- Brif
    // Conditional branch: if arg ≠ 0 -> block_then, else -> block_else.
    // Opcodes: brif
    Brif {
        opcode: Opcode,
        arg: Value,
        block_then: BlockCall,
        block_else: BlockCall,
    },

    // ---- BranchTable
    // Indirect branch via jump table.  No block args can be passed.
    // Opcodes: br_table
    BranchTable {
        opcode: Opcode,
        arg: Value,
        table: JumpTable,
    },

    // ---- IntCompare
    // Integer comparison producing an i8 boolean.
    // Opcodes: icmp
    IntCompare {
        opcode: Opcode,
        cond: IntCC,
        args: [Value; 2],
    },

    // ---- IntCompareImm
    // Integer comparison against a sign-extended 64-bit immediate.
    // Opcodes: icmp_imm
    IntCompareImm {
        opcode: Opcode,
        cond: IntCC,
        imm: Imm64,
        arg: Value,
    },

    // ---- FloatCompare
    // Floating-point comparison producing an i8 boolean (or vector mask).
    // Opcodes: fcmp
    FloatCompare {
        opcode: Opcode,
        cond: FloatCC,
        args: [Value; 2],
    },

    // ---- Call
    // Direct function call to a declared function reference.
    // Opcodes: call, return_call
    Call {
        opcode: Opcode,
        func_ref: FuncRef,
        args: Vec<Value>,
    },

    // ---- CallIndirect
    // Indirect call through a callee address value.
    // args[0] = callee address; args[1..] = call arguments
    // Opcodes: call_indirect, return_call_indirect
    CallIndirect {
        opcode: Opcode,
        sig_ref: SigRef,
        args: Vec<Value>, // first element is the callee pointer
    },

    // ---- TryCall
    // Direct call with exception table.
    // Opcodes: try_call
    TryCall {
        opcode: Opcode,
        func_ref: FuncRef,
        exception: ExceptionTable,
        args: Vec<Value>,
    },

    // ---- TryCallIndirect
    // Indirect call with exception table.
    // args[0] = callee address; args[1..] = call arguments
    // Opcodes: try_call_indirect
    TryCallIndirect {
        opcode: Opcode,
        exception: ExceptionTable,
        args: Vec<Value>, // first element is the callee pointer
    },

    // ---- FuncAddr
    // Compute the address of a declared function (for indirect calls).
    // Opcodes: func_addr
    FuncAddr {
        opcode: Opcode,
        func_ref: FuncRef,
    },

    // ---- StackLoad
    // Load from a static stack slot at a fixed byte offset.
    // Opcodes: stack_load, stack_addr
    StackLoad {
        opcode: Opcode,
        stack_slot: StackSlot,
        offset: Offset32,
    },

    // ---- StackStore
    // Store into a static stack slot at a fixed byte offset.
    // Opcodes: stack_store
    StackStore {
        opcode: Opcode,
        stack_slot: StackSlot,
        offset: Offset32,
        arg: Value,
    },

    // ---- DynamicStackLoad
    // Load from a dynamic stack slot (variable-size SIMD slot).
    // Opcodes: dynamic_stack_load, dynamic_stack_addr
    DynamicStackLoad {
        opcode: Opcode,
        dynamic_stack_slot: DynamicStackSlot,
    },

    // ---- DynamicStackStore
    // Store into a dynamic stack slot.
    // Opcodes: dynamic_stack_store
    DynamicStackStore {
        opcode: Opcode,
        dynamic_stack_slot: DynamicStackSlot,
        arg: Value,
    },

    // ---- Load
    // Load from memory: addr = p + Offset.  Produces one value.
    // Opcodes: load,
    //          uload8, sload8, uload16, sload16, uload32, sload32
    Load {
        opcode: Opcode,
        flags: MemFlags,
        offset: Offset32,
        address: Value, // "p"
    },

    // ---- Store
    // Store to memory: addr = p + Offset.  Produces no value.
    // Opcodes: store, istore8, istore16, istore32
    Store {
        opcode: Opcode,
        flags: MemFlags,
        offset: Offset32,
        value: Value,   // "x" — value to store
        address: Value, // "p" — base pointer
    },

    // ---- LoadNoOffset
    // Load with no constant offset (typically vector / atomic loads).
    // Opcodes: uload8x8, sload8x8, uload16x4, sload16x4,
    //          uload32x2, sload32x2, atomic_load
    LoadNoOffset {
        opcode: Opcode,
        flags: MemFlags,
        address: Value,
    },

    // ---- StoreNoOffset
    // Store with no constant offset.
    // Opcodes: atomic_store
    StoreNoOffset {
        opcode: Opcode,
        flags: MemFlags,
        value: Value,
        address: Value,
    },

    // ---- Trap
    // Unconditional trap.  Terminates a block.
    // Opcodes: trap
    Trap {
        opcode: Opcode,
        code: TrapCode,
    },

    // ---- CondTrap
    // Conditional trap: trap if arg is zero (trapz) or non-zero (trapnz).
    // Opcodes: trapz, trapnz
    CondTrap {
        opcode: Opcode,
        code: TrapCode,
        arg: Value,
    },

    // ---- AtomicCas
    // Sequentially-consistent compare-and-swap.
    // Opcodes: atomic_cas
    AtomicCas {
        opcode: Opcode,
        flags: MemFlags,
        address: Value,     // "p"
        expected: Value,    // expected old value
        replacement: Value, // new value to store if equal
    },

    // ---- AtomicRmw
    // Atomic read-modify-write.  Returns old value at address.
    // Opcodes: atomic_rmw
    AtomicRmw {
        opcode: Opcode,
        flags: MemFlags,
        op: AtomicRmwOp,
        address: Value,
        value: Value,
    },

    // ---- IntAddTrap
    // Add two integers; trap on unsigned overflow.
    // Opcodes: uadd_overflow_trap
    IntAddTrap {
        opcode: Opcode,
        code: TrapCode,
        args: [Value; 2],
    },

    // ---- Shuffle
    // Variable-lane permutation of two SIMD vectors using a 16-byte mask.
    //   mask[i] ∈ [0,15] -> lane from a; [16,31] -> lane from b;
    //   ≥ 32 -> undefined.
    // Opcodes: shuffle
    Shuffle {
        opcode: Opcode,
        imm: Immediate,   // 16-byte lane-select mask (index into imm pool)
        args: [Value; 2], // [a, b]
    },

    // ---- ExceptionHandlerAddress
    // Produce the machine address of an exception-handler block, adjusted
    // by an addend.  Used to build exception tables at runtime.
    // Opcodes: get_exception_handler_address
    ExceptionHandlerAddress {
        opcode: Opcode,
        block: Block, // raw block reference (not a BlockCall)
        imm: Imm64,   // byte addend
    },
}

// ------------------------------------
// OPCODES
// (Complete alphabetical listing grouped by semantic category)
// ------------------------------------

pub enum Opcode {
    // ---- Control flow
    Jump,               // jump block(args...)
    Brif,               // brif v, block_then(...), block_else(...)
    BrTable,            // br_table v, JT
    Return,             // return vals...
    ReturnCall,         // return_call fn(args...)
    ReturnCallIndirect, // return_call_indirect sig, callee(args...)
    Call,               // call fn(args...)
    CallIndirect,       // call_indirect sig, callee(args...)
    TryCall,            // try_call fn(args...), ET
    TryCallIndirect,    // try_call_indirect sig, callee(args...), ET

    // ---- Traps
    Trap,            // trap code
    Trapz,           // trapz v, code
    Trapnz,          // trapnz v, code
    Debugtrap,       // debugtrap
    ResumableTrap,   // resumable_trap code
    ResumableTrapnz, // resumable_trapnz v, code

    // ---- Integer constants
    Iconst, // iconst.T N

    // ---- Float constants
    F16const,  // f16const 0x...
    F32const,  // f32const 0x...
    F64const,  // f64const 0x...
    F128const, // f128const const0
    Vconst,    // vconst.TxN const0

    // ---- Integer arithmetic
    Iadd,             // iadd x, y
    Isub,             // isub x, y
    Ineg,             // ineg x
    Iabs,             // iabs x
    Imul,             // imul x, y
    Umulhi,           // umulhi x, y
    Smulhi,           // smulhi x, y
    SqmulRoundSat,    // sqmul_round_sat x, y
    Udiv,             // udiv x, y
    Sdiv,             // sdiv x, y
    Urem,             // urem x, y
    Srem,             // srem x, y
    IaddImm,          // iadd_imm x, N
    ImulImm,          // imul_imm x, N
    UdivImm,          // udiv_imm x, N
    SdivImm,          // sdiv_imm x, N
    UremImm,          // urem_imm x, N
    SremImm,          // srem_imm x, N
    IrsubImm,         // irsub_imm x, N  (N - x)
    IaddCarry,        // iadd_carry x, y, c_in  -> (a, c_out)  [deprecated]
    IaddCarryFlagss,  // (hidden)
    IsubBorrow,       // isub_borrow x, y, b_in -> (a, b_out)  [deprecated]
    SaddOverflow,     // sadd_overflow x, y -> (a, of)
    UaddOverflow,     // uadd_overflow x, y -> (a, of)
    SsubOverflow,     // ssub_overflow x, y -> (a, of)
    UsubOverflow,     // usub_overflow x, y -> (a, of)
    SmulOverflow,     // smul_overflow x, y -> (a, of)
    UmulOverflow,     // umul_overflow x, y -> (a, of)
    SaddOverflowCin,  // sadd_overflow_cin x, y, c_in -> (a, c_out)
    UaddOverflowCin,  // uadd_overflow_cin x, y, c_in -> (a, c_out)
    UsubOverflowBin,  // usub_overflow_bin x, y, b_in -> (a, b_out)
    UaddOverflowTrap, // uadd_overflow_trap x, y, code -> a (traps on overflow)
    IaddPairwise,     // iadd_pairwise x, y
    Iconcat,          // iconcat lo, hi  (produces lo:hi)
    Isplit,           // isplit x        -> (lo, hi)

    // ---- Integer compare
    Icmp,    // icmp cc, x, y
    IcmpImm, // icmp_imm cc, x, N
    Bmask,   // bmask x        (broadcast scalar boolean to all bits)

    // ---- Integer bit-manipulation
    Band,    // band x, y
    Bor,     // bor x, y
    Bxor,    // bxor x, y
    Bnot,    // bnot x
    BandNot, // band_not x, y
    BorNot,  // bor_not x, y
    BxorNot, // bxor_not x, y
    BandImm, // band_imm x, N
    BorImm,  // bor_imm x, N
    BxorImm, // bxor_imm x, N
    Rotl,    // rotl x, y
    Rotr,    // rotr x, y
    RotlImm, // rotl_imm x, N
    RotrImm, // rotr_imm x, N
    Ishl,    // ishl x, y
    Ushr,    // ushr x, y
    Sshr,    // sshr x, y
    IshlImm, // ishl_imm x, N
    UshrImm, // ushr_imm x, N
    SshrImm, // sshr_imm x, N
    Bitrev,  // bitrev x
    Clz,     // clz x
    Cls,     // cls x
    Ctz,     // ctz x
    Bswap,   // bswap x
    Popcnt,  // popcnt x

    // ---- Integer extend / reduce
    Ireduce, // ireduce.T x    (truncate to narrower int)
    Uextend, // uextend.T x    (zero-extend)
    Sextend, // sextend.T x    (sign-extend)

    // ---- Float arithmetic
    Fadd,       // fadd x, y
    Fsub,       // fsub x, y
    Fmul,       // fmul x, y
    Fdiv,       // fdiv x, y
    Sqrt,       // sqrt x
    Fma,        // fma x, y, z   (fused multiply-add)
    Fneg,       // fneg x
    Fabs,       // fabs x
    Fcopysign,  // fcopysign x, y
    Fmin,       // fmin x, y
    Fmax,       // fmax x, y
    FminPseudo, // fmin_pseudo x, y
    FmaxPseudo, // fmax_pseudo x, y

    // ---- Float rounding
    Ceil,    // ceil x
    Floor,   // floor x
    Nearest, // nearest x
    Trunc,   // trunc x

    // ---- Float compare
    Fcmp, // fcmp cc, x, y

    // ---- Float conversion
    Fdemote,       // fdemote.T x  (widen -> narrower float)
    Fpromote,      // fpromote.T x (promote -> wider float)
    FcvtToUint,    // fcvt_to_uint.T x      (traps if out of range)
    FcvtToSint,    // fcvt_to_sint.T x
    FcvtToUintSat, // fcvt_to_uint_sat.T x  (saturates)
    FcvtToSintSat, // fcvt_to_sint_sat.T x
    FcvtFromUint,  // fcvt_from_uint.T x
    FcvtFromSint,  // fcvt_from_sint.T x

    // ---- Bitcast / reinterpret
    Bitcast,        // bitcast.T flags, x
    ScalarToVector, // scalar_to_vector.TxN s
    VhighBits,      // vhigh_bits x  (collect MSB of each lane -> bitmask)

    // ---- Select
    Select,             // select c, x, y
    SelectSpectreGuard, // select_spectre_guard c, x, y

    // ---- Memory
    Load,     // load.T flags, p+Offset
    Store,    // store flags, x, p+Offset
    Uload8,   // uload8.T flags, p+Offset
    Sload8,   // sload8.T flags, p+Offset
    Istore8,  // istore8 flags, x, p+Offset
    Uload16,  // uload16.T flags, p+Offset
    Sload16,  // sload16.T flags, p+Offset
    Istore16, // istore16 flags, x, p+Offset
    Uload32,  // uload32.T flags, p+Offset
    Sload32,  // sload32.T flags, p+Offset
    Istore32, // istore32 flags, x, p+Offset

    // ---- SIMD widening loads (LoadNoOffset)
    Uload8x8,  // uload8x8 flags, p   -> i16x8
    Sload8x8,  // sload8x8 flags, p   -> i16x8
    Uload16x4, // uload16x4 flags, p  -> i32x4
    Sload16x4, // sload16x4 flags, p  -> i32x4
    Uload32x2, // uload32x2 flags, p  -> i64x2
    Sload32x2, // sload32x2 flags, p  -> i64x2

    // ---- Stack-slot access
    StackLoad,         // stack_load.T SS+Offset
    StackStore,        // stack_store x, SS+Offset
    StackAddr,         // stack_addr.T SS+Offset
    DynamicStackLoad,  // dynamic_stack_load.T DSS
    DynamicStackStore, // dynamic_stack_store x, DSS
    DynamicStackAddr,  // dynamic_stack_addr.T DSS

    // ---- Global values / symbols
    GlobalValue, // global_value.T GV
    SymbolValue, // symbol_value.T GV
    TlsValue,    // tls_value.T GV
    FuncAddr,    // func_addr.T fn

    // ---- Atomic
    AtomicLoad,  // atomic_load.T flags, p
    AtomicStore, // atomic_store flags, x, p
    AtomicRmw,   // atomic_rmw.T flags, op, p, x
    AtomicCas,   // atomic_cas.T flags, p, expected, replacement
    Fence,       // fence

    // ---- SIMD vector operations
    Splat,       // splat.TxN x
    Swizzle,     // swizzle x, y
    Shuffle,     // shuffle a, b, mask
    Insertlane,  // insertlane x, y, Idx
    Extractlane, // extractlane x, Idx
    // ScalarToVector,    // (see above)
    SelectI,       // (internal)
    VanyTrue,      // vany_true x
    VallTrue,      // vall_true x
    SwidenLow,     // swiden_low x        (narrow int lanes -> wider lanes, signed)
    SwidenHigh,    // swiden_high x       (upper half of lanes)
    UwidenLow,     // uwiden_low x        (unsigned)
    UwidenHigh,    // uwiden_high x
    Unarrow,       // unarrow x, y        (saturate to narrower unsigned)
    Snarrow,       // snarrow x, y        (saturate to narrower signed)
    SaddSat,       // sadd_sat x, y
    UaddSat,       // uadd_sat x, y
    SsubSat,       // ssub_sat x, y
    UsubSat,       // usub_sat x, y
    Smin,          // smin x, y
    Umin,          // umin x, y
    Smax,          // smax x, y
    Umax,          // umax x, y
    AvgRound,      // avg_round x, y
    ExtractVector, // extract_vector x, Idx  (128-bit sub-vector)
    // IaddPairwise,      // (see above)
    Bitselect, // bitselect c, x, y      (bit-level blend using c as mask)
    Blendv,    // blendv c, x, y         (MSB of c selects lane)

    // ---- Miscellaneous
    Nop,              // nop
    Copy,             // copy x              (identity, useful for regalloc)
    Regspill,         // (internal)
    Regfill,          // (internal)
    GetPinnedReg,     // get_pinned_reg.T
    SetPinnedReg,     // set_pinned_reg x
    GetFramePointer,  // get_frame_pointer.T
    GetStackPointer,  // get_stack_pointer.T
    GetReturnAddress, // get_return_address.T
    SequencePoint,    // sequence_point

    // ---- Exceptions
    GetExceptionHandlerAddress, // get_exception_handler_address.T block, N
    StackSwitch,                // stack_switch store_ptr, load_ptr, in_payload0 -> out_payload0

    // ---- x86-specific
    X86Pshufb,    // x86_pshufb x, y
    X86Pmulhrsw,  // x86_pmulhrsw x, y
    X86Pmaddubsw, // x86_pmaddubsw x, y
    X86Cvtt2dq,   // x86_cvtt2dq.T x
}

// ------------------------------------
// COMPLETE FUNCTION EXAMPLE  (informational, not part of the grammar)
// ------------------------------------
//
// function %example(i32, i64) -> i32 fast {
// ss0 = explicit_slot 16, align = 4
// gv0 = vmctx
// gv1 = load.i64 notrap readonly gv0+8
// sig0 = (i32, i64) -> i32 fast
// fn0 = colocated u0:0 sig0
// const0 = [0x00 0x01 0x02 0x03 ... 0x0f]  ;; 16-byte SIMD constant
//
// block0(v0: i32, v1: i64):
//     ;; if v0+1 < v0 { goto block1(v0+1) } else { goto block2(v0) }
//
//     v2 = iadd_imm v0, 1
//     v3 = icmp slt v2, v0
//     brif v3, block1(v2), block2(v0)
//
// block1(v4: i32):
//     ;; v5 = *(v1+0)
//     ;; *(v1+0) = v4
//     ;; return v5
//
//     v5 = load.i32 aligned v1+0
//     store aligned v4, v1+0
//     return v5
//
// block2(v6: i32):
//     trap user0
// }
