// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use cranelift_codegen::{
    isa,
    settings::{self, Configurable},
};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, DataId, Linkage, ModuleError, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};

pub struct CodeGenerator<T>
where
    T: cranelift_module::Module,
{
    /// A `Module` is a utility for collecting functions and data objects, and linking them together.
    ///
    /// https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html
    pub generator_module: T,

    /// Compilation context.
    /// Persistent data structures and compilation pipeline.
    ///
    /// https://docs.rs/cranelift-codegen/latest/cranelift_codegen/struct.Context.html
    pub generator_context: cranelift_codegen::Context,

    /// Structure used for translating a series of functions into Cranelift IR.
    ///
    /// In order to reduce memory reallocations when compiling multiple functions,
    /// FunctionBuilderContext holds various data structures which are cleared between
    /// functions, rather than dropped, preserving the underlying allocations.
    ///
    /// https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilderContext.html
    pub function_builder_context: FunctionBuilderContext,

    /// A description of a data object.
    ///
    /// DataDescription is used to define the contents and properties of
    /// a data object before it is defined in the module.
    /// Put it in the Generator struct to avoid creating a new DataDescription for each data object definition,
    /// and to reuse the same DataDescription for multiple data object definitions by clearing it after each definition.
    ///
    /// https://docs.rs/cranelift-module/latest/cranelift_module/struct.DataDescription.html
    pub data_description: DataDescription,
}

/// Create new Generator
///
/// @param symbol_table: a symbol table is a mapping from symbol (function or data) names to their
///        corresponding addresses.
///
/// Reference:
/// - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/
/// - https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/ir.md
/// - https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/trait.InstBuilder.html
/// - https://github.com/bytecodealliance/cranelift-jit-demo
pub fn new_jit_module(symbol_table: Vec<(String, *const u8)>) -> JITModule {
    // The pipeline of creating a Cranelift module:
    //
    // 1. (flag builder -> flags) + isa builder -> isa
    // 2. isa + module builder -> Cranelift module

    // All code generation flags:
    // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html
    let mut flag_builder = settings::builder();

    // Enable Position-Independent Code generation.
    //
    // Ref:
    // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.is_pic
    flag_builder.set("is_pic", "false").unwrap();

    // Optimization level for generated code.
    //
    // Supported levels:
    // - none: Minimise compile time by disabling most optimizations.
    // - speed: Generate the fastest possible code
    // - speed_and_size: like “speed”, but also perform transformations aimed at reducing code size.
    //
    // Ref:
    // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.opt_level
    flag_builder.set("opt_level", "speed").unwrap();

    // Defines the model used to perform TLS accesses.
    //
    // Possible values:
    //
    // - none
    // - elf_gd (ELF)
    // - macho (Mach-O)
    // - coff (COFF)
    //
    // Note: the target "x86_64-unknown-linux-gnu" does not set "tls_model" by default.
    //
    // Ref:
    // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.tls_model
    // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/enum.TlsModel.html
    flag_builder.set("tls_model", "none").unwrap();

    // Preserve frame pointers
    //
    // Preserving frame pointers – even inside leaf functions – makes it easy to capture
    // the stack of a running program, without requiring any side tables or
    // metadata (like .eh_frame sections).
    // Many sampling profilers and similar tools walk frame pointers to capture stacks.
    // Enabling this option will play nice with those tools.
    //
    // Ref:
    // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.preserve_frame_pointers
    flag_builder.set("preserve_frame_pointers", "true").unwrap();

    // Use colocated libcalls.
    //
    // Generate code that assumes that libcalls can be declared “colocated”,
    // meaning they will be defined along with the current function,
    // such that they can use more efficient addressing.
    //
    // - Traditional Libcalls:
    //   In traditional compilation, libcalls are external functions defined
    //   in a separate library. The compiler generates a call instruction that references
    //   the function's address.
    //
    // - Colocated Libcalls:
    //   When this flag is set to true, Cranelift assumes that the libcall definitions are available
    //   within the same compilation unit. This allows the compiler to generate more efficient code by:
    //   - Direct Calls: Instead of generating a full call instruction, the compiler can use
    //     a direct jump to the libcall's address.
    //   - Reduced Relocation Overhead: By avoiding external references, the compiler can reduce
    //     the number of relocations needed, which can improve code size and loading time.
    //
    // Ref:
    // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/settings/struct.Flags.html#method.use_colocated_libcalls
    flag_builder.set("use_colocated_libcalls", "false").unwrap();

    let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
        panic!("The platform of the host machine is not supported: {}", msg);
    });

    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .unwrap();

    let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());

    // Define a symbol in the internal symbol table.
    //
    // The JIT will use the symbol table to resolve names that are declared, but not defined,
    // in the module being compiled. A common example is external functions. With this method,
    // functions and data can be exposed to the code being compiled which are defined by the host.
    //
    // If a symbol is defined more than once, the most recent definition will be retained.
    //
    // If the JIT fails to find a symbol in its internal table, it will fall back to a platform-specific
    // search (this typically involves searching the current process for public symbols,
    // followed by searching the platform’s C runtime).
    //
    // Ref:
    // https://docs.rs/cranelift-jit/latest/cranelift_jit/struct.JITBuilder.html#method.symbol
    jit_builder.symbols(symbol_table);

    JITModule::new(jit_builder)
}

/// Target Triplets:
///
/// - "x86_64-unknown-linux-gnu"
/// - "aarch64-unknown-linux-gnu"
///
/// Currently, aarch64, riscv64, s390x, and x86-64 are supported.
///
/// Ref:
/// - https://docs.rs/cranelift-object/latest/cranelift_object/
/// - https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/object/tests/basic.rs
pub fn new_object_module(
    module_name: &str,
    target_triplet_opt: Option<&str>,
    tls_mode_opt: Option<&str>,
) -> ObjectModule {
    let mut flag_builder = settings::builder();
    flag_builder.set("is_pic", "true").unwrap();
    flag_builder.set("opt_level", "speed").unwrap();
    flag_builder
        .set("tls_model", tls_mode_opt.unwrap_or("elf_gd"))
        .unwrap();

    let platform = target_triplet_opt.unwrap_or("x86_64-unknown-linux-gnu");
    let isa_builder = isa::lookup_by_name(platform).unwrap_or_else(|msg| {
        panic!(
            "The target platform \"{}\" is not supported: {}",
            platform, msg
        );
    });

    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .unwrap();

    let object_builder = ObjectBuilder::new(isa, module_name, default_libcall_names()).unwrap();

    ObjectModule::new(object_builder)
}

impl<T> CodeGenerator<T>
where
    T: cranelift_module::Module,
{
    pub fn new(generator_module: T) -> Self {
        let generator_context = generator_module.make_context();
        let function_builder_context = FunctionBuilderContext::new();
        let data_description = DataDescription::new();

        Self {
            generator_module,
            generator_context,
            function_builder_context,
            data_description,
        }
    }

    // Define a data with the given contents.
    //
    // The process of accessing a data (which is inside .data/.ro_data/.bss sections) from a function body:
    // 1. let gv = module::declare_data_in_func(...)
    // 2a. let target_data_address = ins().symbol_value(gv)     ;; for symbols
    // 2b. let target_data_address = ins().global_value(gv)     ;; data objects defined in the module
    // 3a. let value = ins().load(Type, target_data_address)    ;; read
    // 3b. ins.store(value, target_data_address)                ;; write
    pub fn define_read_only_data(
        &mut self,
        name: &str,
        data: Vec<u8>,

        // Alignment in bytes. None means that the default alignment of the respective module should be used.
        // The alignment must be a power of two.
        align: Option<u64>,
        export: bool,
        thread_local: bool,
    ) -> Result<DataId, ModuleError> {
        let linkage = if export {
            Linkage::Export
        } else {
            Linkage::Local
        };

        // https://docs.rs/cranelift-module/latest/cranelift_module/struct.DataDescription.html
        self.data_description.define(data.into_boxed_slice());

        if let Some(align) = align {
            self.data_description.set_align(align);
        }

        let data_id = self
            .generator_module
            .declare_data(name, linkage, false, thread_local)?;

        self.generator_module
            .define_data(data_id, &self.data_description)?;

        // clear the data description for the next data definition, to reuse
        // the same DataDescription for multiple data object definitions.
        self.data_description.clear();

        Ok(data_id)
    }

    pub fn define_read_write_data(
        &mut self,
        name: &str,
        data: Vec<u8>,
        align: Option<u64>,
        export: bool,
        thread_local: bool,
    ) -> Result<DataId, ModuleError> {
        let linkage = if export {
            Linkage::Export
        } else {
            Linkage::Local
        };

        self.data_description.define(data.into_boxed_slice());
        if let Some(align) = align {
            self.data_description.set_align(align);
        }

        let data_id = self
            .generator_module
            .declare_data(name, linkage, true, thread_local)?;

        self.generator_module
            .define_data(data_id, &self.data_description)?;

        self.data_description.clear();

        Ok(data_id)
    }

    pub fn define_uninitialized_data(
        &mut self,
        name: &str,
        size: usize,
        align: Option<u64>,
        export: bool,
        thread_local: bool,
    ) -> Result<DataId, ModuleError> {
        let linkage = if export {
            Linkage::Export
        } else {
            Linkage::Local
        };

        self.data_description.define_zeroinit(size);
        if let Some(align) = align {
            self.data_description.set_align(align);
        }

        let data_id = self
            .generator_module
            .declare_data(name, linkage, true, thread_local)?;
        self.generator_module
            .define_data(data_id, &self.data_description)?;

        self.data_description.clear();

        Ok(data_id)
    }

    pub fn get_type_of_pointer(&self) -> cranelift_codegen::ir::Type {
        // Get the pointer type of the target platform.
        // - `self.generator_module.target_config().pointer_type()`
        // - `self.generator_module.isa().pointer_type()`

        self.generator_module.target_config().pointer_type()
    }
}

#[cfg(test)]
mod tests {
    use cranelift_codegen::ir::{
        AbiParam, BlockArg, Function, InstBuilder, MemFlags, StackSlotData, StackSlotKind,
        UserFuncName, condcodes::IntCC, types,
    };
    use cranelift_frontend::FunctionBuilder;
    use cranelift_module::{Linkage, Module};

    use crate::code_generator::{CodeGenerator, new_jit_module, new_object_module};

    /// A simple function used for the importing function testing
    extern "C" fn add(a: i32, b: i32) -> i32 {
        a + b
    }

    /// Test function definition and function calling.
    #[test]
    fn test_code_generator_jit_define_functions() {
        let jit_module = new_jit_module(vec![]);
        let mut code_generator = CodeGenerator::new(jit_module);

        // Building function "inc"
        //
        // ```pseudo
        // fn inc (a:i32) -> i32 {
        //    a+11
        // }
        // ```
        let func_inc_id = {
            let mut func_inc_sig = code_generator.generator_module.make_signature();
            func_inc_sig.params.push(AbiParam::new(types::I32));
            func_inc_sig.returns.push(AbiParam::new(types::I32));

            // Declare a function in this module and get its FuncId.
            //
            // Ref:
            // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#tymethod.declare_function
            let func_inc_id = code_generator
                .generator_module
                .declare_function("inc", Linkage::Local, &func_inc_sig)
                .unwrap();

            let mut func_inc = Function::with_name_signature(
                UserFuncName::user(0, func_inc_id.as_u32()),
                func_inc_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_inc, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();

            // Add block parameters according to the function parameters to the entry block.
            // We can also use `FunctionBuilder::append_block_param` to add block parameters one by one.
            //
            // Ref:
            // - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html#method.append_block_params_for_function_params
            // - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html#method.append_block_params_for_function_returns
            // - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html#method.append_block_param
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);

            // Declares that all the predecessors of this block are known.
            // Function to call with block as soon as the last branch instruction to block has been created.
            // Forgetting to call this method on every block will cause inconsistencies in the produced functions.
            //
            // Since the entry block has no predecessors, we can seal it immediately after creating it.
            // https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html#method.seal_block
            function_builder.seal_block(entry_block);

            // Instructions reference:
            // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/trait.InstBuilder.html
            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.block_params(entry_block)[0];
            let value_2 = function_builder.ins().iadd(value_0, value_1);
            function_builder.ins().return_(&[value_2]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR:
            // `println!("{}", func_inc.display());`

            // Generate `func_inc` body machine code
            //
            // Define a function, producing the function body from the given Context.
            // Returns the size of the function’s code and constant data.
            //
            // Unlike define_function_with_control_plane this uses a default ControlPlane for convenience.
            //
            // Note: After calling this function the given Context will contain the compiled function.
            //
            // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#method.define_function
            code_generator.generator_context.func = func_inc;

            code_generator
                .generator_module
                .define_function(func_inc_id, &mut code_generator.generator_context)
                .unwrap();

            // Clear the given Context and reset it for use with a new function.
            // This ensures that the Context is initialized with the default calling convention for the TargetIsa.
            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            // Return the FuncId of the function "inc"
            func_inc_id
        };

        // Building function "main"
        //
        // ```pseudo
        // fn main () -> i32 {
        //    inc(13)
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            // The linkage of function `main` should be `export`, so that the linker can find it.
            //
            // Ref:
            // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#tymethod.declare_function
            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            // Add a reference to the function "inc" in the function "main" body.
            //
            // Ref:
            // https://docs.rs/cranelift-module/latest/cranelift_module/trait.Module.html#method.declare_func_in_func
            let func_inc_ref = code_generator
                .generator_module
                .declare_func_in_func(func_inc_id, function_builder.func);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            let value0 = function_builder.ins().iconst(types::I32, 13);
            let call0 = function_builder.ins().call(func_inc_ref, &[value0]);
            let value1 = {
                let results = function_builder.inst_results(call0);
                results[0]
            };
            function_builder.ins().return_(&[value1]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        // Linking
        //
        // Finalize all functions and data objects that are defined but not yet finalized.
        // All symbols referenced in their bodies that are declared as needing a definition must be defined by this point.
        //
        // Use get_finalized_function and get_finalized_data to obtain the final artifacts.
        //
        // Returns ModuleError in case of allocation or syscall failure
        //
        // https://docs.rs/cranelift-jit/latest/cranelift_jit/struct.JITModule.html#method.finalize_definitions
        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        // Get function pointers
        //
        // Returns the address of a finalized function.
        //
        // The pointer remains valid until either JITModule::free_memory is called or in the future some way of
        // deallocating this individual function is used.
        // https://docs.rs/cranelift-jit/latest/cranelift_jit/struct.JITModule.html#method.get_finalized_function
        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        // Cast `ptr` to Rust function
        let fn_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(fn_main(), 24);

        // Free memory allocated for code and data segments of compiled functions.
        //
        // Safety:
        //
        // Because this function invalidates any pointers retrieved from the corresponding module,
        // it should only be used when none of the functions from that module are currently executing
        // and none of the fn pointers are called afterwards.
        unsafe { code_generator.generator_module.free_memory() };
    }

    /// Test defining a data object and accessing it from a function body.
    #[test]
    fn test_code_generator_jit_define_data() {
        let jit_module = new_jit_module(vec![]);
        let mut code_generator = CodeGenerator::new(jit_module);

        let pointer_type = code_generator.get_type_of_pointer();

        let bin0 = 11_i32.to_le_bytes().to_vec();
        let bin1 = 13_i32.to_le_bytes().to_vec();

        let data_id0 = code_generator
            .define_read_only_data("d0", bin0, Some(2), false, false)
            .unwrap();
        let data_id1 = code_generator
            .define_read_write_data("d1", bin1, Some(2), false, false)
            .unwrap();
        let data_id2 = code_generator
            .define_uninitialized_data("d2", 4, Some(2), false, false)
            .unwrap();

        // Building function "main"
        //
        // ```pseudo
        // fn main() -> int {
        //     let d0 = read_only_data(11)
        //     let d1 = read_write_data(13)
        //     let d2 = uninitialized_data
        //
        //     let v0 = load(d1)
        //     let v1 = v0 + 17
        //     store(v1, d2)        ;; now d2 contains 30
        //     let v2 = load(d0)    ;; v2 is 11
        //     let v3 = load(d2)    ;; v3 is 30
        //     v2 + v3              ;; return 41
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            // read and write
            let gv0 = code_generator
                .generator_module
                .declare_data_in_func(data_id0, function_builder.func);
            let gv1 = code_generator
                .generator_module
                .declare_data_in_func(data_id1, function_builder.func);
            let gv2 = code_generator
                .generator_module
                .declare_data_in_func(data_id2, function_builder.func);

            let p0 = function_builder.ins().global_value(pointer_type, gv0);
            let p1 = function_builder.ins().global_value(pointer_type, gv1);
            let p2 = function_builder.ins().global_value(pointer_type, gv2);

            // The memory flags to use for load and store instructions.
            // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/struct.MemFlags.html
            let mem_flags = MemFlags::new();

            // Memory load
            // https://docs.rs/cranelift-codegen/latest/cranelift_codegen/ir/trait.InstBuilder.html#method.load
            let value_0 = function_builder.ins().load(types::I32, mem_flags, p1, 0);
            let value_1 = function_builder.ins().iadd_imm(value_0, 17);
            function_builder.ins().store(mem_flags, value_1, p2, 0);

            let value_2 = function_builder.ins().load(types::I32, mem_flags, p0, 0);
            let value_3 = function_builder.ins().load(types::I32, mem_flags, p2, 0);
            let value_add = function_builder.ins().iadd(value_2, value_3);

            function_builder.ins().return_(&[value_add]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        let fn_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(fn_main(), 41);

        unsafe { code_generator.generator_module.free_memory() };
    }

    /// Test calling external function by importing symbols.
    /// Note that only the JIT module supports importing symbols, the `Object` module does not support it.
    #[test]
    fn test_code_generator_jit_import_function() {
        // Convertion between Rust function and function pointer:
        //
        // - Obtains the pointer of a Rust function:
        //   `let ptr = fn_name as *const u8;`
        //
        // - Convert the pointer to function:
        //   - `let fn_name: extern "C" fn(...) -> ... = unsafe { std::mem::transmute(ptr) };`
        //   - `let fn_name = fn_add_ptr as *const extern "C" fn(i32,i32)->i32;`
        let fn_add_ptr = add as *const u8;

        // A symbol table is a mapping from symbol (function or data) names to their corresponding addresses.
        let symbol_table = vec![("add".to_owned(), fn_add_ptr)];

        let jit_module = new_jit_module(symbol_table);
        let mut code_generator = CodeGenerator::new(jit_module);

        // Importing the external function "add" into the module, and get its FuncId.
        let mut fn_add_sig = code_generator.generator_module.make_signature();
        fn_add_sig.params.push(AbiParam::new(types::I32));
        fn_add_sig.params.push(AbiParam::new(types::I32));
        fn_add_sig.returns.push(AbiParam::new(types::I32));

        let fn_add_id = code_generator
            .generator_module
            .declare_function("add", Linkage::Import, &fn_add_sig)
            .unwrap();

        // Building function "main"
        //
        // ```pseudo
        // extern "C" fn add(i32, i32) -> i32;
        //
        // fn main() -> int {
        //     add(11, 13)
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let fn_add_ref = code_generator
                .generator_module
                .declare_func_in_func(fn_add_id, function_builder.func);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.ins().iconst(types::I32, 13);
            let call0 = function_builder.ins().call(fn_add_ref, &[value_0, value_1]);
            let value_2 = function_builder.inst_results(call0)[0];

            function_builder.ins().return_(&[value_2]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        let fn_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(fn_main(), 24);

        unsafe { code_generator.generator_module.free_memory() };
    }

    /// Test importing data by importing symbols.
    /// Note that only the JIT module supports importing symbols, the `Object` module does not support it.
    #[test]
    fn test_code_generator_jit_import_data() {
        let data0: i32 = 11;
        let mut data1: i32 = 13;

        let data0_ptr = &data0 as *const i32 as *const u8;
        let data1_ptr = &mut data1 as *mut i32 as *const u8;

        let jit_module = new_jit_module(vec![
            ("data0".to_string(), data0_ptr),
            ("data1".to_string(), data1_ptr),
        ]);
        let mut code_generator = CodeGenerator::new(jit_module);

        let pointer_type = code_generator.get_type_of_pointer();

        // import data
        let data0_id = code_generator
            .generator_module
            .declare_data("data0", Linkage::Import, false, false)
            .unwrap();
        let data1_id = code_generator
            .generator_module
            .declare_data("data1", Linkage::Import, true, false)
            .unwrap();

        // Building function "main"
        //
        // ```pseudo
        // fn main(ptr0, ptr1) -> int {
        //     let v0 = load(ptr1)
        //     let v1 = v0 + 17
        //     store(ptr1, v1)      ;; now data1 contains 30
        //     let v2 = load(ptr0)  ;; v2 is 11
        //     let v3 = load(ptr1)  ;; v3 is 30
        //     v2 + v3              ;; return 41
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            // read and write
            let gv0 = code_generator
                .generator_module
                .declare_data_in_func(data0_id, function_builder.func);
            let gv1 = code_generator
                .generator_module
                .declare_data_in_func(data1_id, function_builder.func);

            let p0 = function_builder.ins().symbol_value(pointer_type, gv0);
            let p1 = function_builder.ins().symbol_value(pointer_type, gv1);

            let mem_flags = MemFlags::new();

            let value_0 = function_builder.ins().load(types::I32, mem_flags, p1, 0);
            let value_1 = function_builder.ins().iadd_imm(value_0, 17);
            function_builder.ins().store(mem_flags, value_1, p1, 0);

            let value_2 = function_builder.ins().load(types::I32, mem_flags, p0, 0);
            let value_3 = function_builder.ins().load(types::I32, mem_flags, p1, 0);
            let value_add = function_builder.ins().iadd(value_2, value_3);

            function_builder.ins().return_(&[value_add]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        let fn_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(fn_main(), 41);
        assert_eq!(data1, 30);

        unsafe { code_generator.generator_module.free_memory() };
    }

    /// Test the usage of stack slot, which is a region of memory on the stack (in the function's stack frame) that
    /// can be used to store temporary values (they are local variables also) during function execution.
    ///
    /// Example of stack slot usage:
    ///
    /// ```rust
    /// let ss = function_builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 2, None));
    /// function_builder.ins().stack_store(Value, ss, Offset);
    /// let v = function_builder.ins().stack_load(Type, ss, Offset);
    /// ```
    ///
    /// Ref:
    /// - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html#method.create_sized_stack_slot
    /// - https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/stackslot/struct.StackSlotData.html
    /// - https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/trait.InstBuilder.html#method.stack_load
    /// - https://docs.rs/cranelift-codegen/0.131.0/cranelift_codegen/ir/trait.InstBuilder.html#method.stack_store
    #[test]
    fn test_code_generator_jit_stack_slot() {
        let jit_module = new_jit_module(vec![]);
        let mut code_generator = CodeGenerator::new(jit_module);

        // Building function "main"
        //
        // ```pseudo
        // fn main() -> int {
        //     let ss = create_sized_stack_slot(8,2)
        //     stack_store(11, ss, 0)
        //     stack_store(13, ss, 4)
        //     let v0 = stack_load(i32, ss, 0)
        //     let v1 = stack_load(i32, ss, 4)
        //     v0 + v1
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            let ss = function_builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                2,
            ));

            // store
            let value_s0 = function_builder.ins().iconst(types::I32, 11);
            let value_s1 = function_builder.ins().iconst(types::I32, 13);
            function_builder.ins().stack_store(value_s0, ss, 0);
            function_builder.ins().stack_store(value_s1, ss, 4);

            // load
            let value_l0 = function_builder.ins().stack_load(types::I32, ss, 0);
            let value_l1 = function_builder.ins().stack_load(types::I32, ss, 4);
            let value_add = function_builder.ins().iadd(value_l0, value_l1);

            function_builder.ins().return_(&[value_add]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        let fn_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(fn_main(), 24);

        unsafe { code_generator.generator_module.free_memory() };
    }

    /// Test the usage of local variables.
    ///
    /// Example of local variables usage:
    ///
    /// ```
    /// let x = function_builder.declare_var(types::I32);   // declare a local variable `x` of type i32
    /// function_builder.def_var(x, tmp);                   // set value
    /// let .. = function_builder.use_var(x);               // get value
    /// ```
    ///
    /// Ref:
    /// - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/
    /// - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html#method.declare_var
    /// - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html#method.def_var
    /// - https://docs.rs/cranelift-frontend/latest/cranelift_frontend/struct.FunctionBuilder.html#method.use_var
    #[test]
    fn test_code_generator_jit_local_variables() {
        let jit_module = new_jit_module(vec![]);
        let mut code_generator = CodeGenerator::new(jit_module);

        // Building function "main"
        //
        // ```pseudo
        // fn main() -> int {
        //     let var0 = declare_var(types::I32)
        //     let var1 = declare_var(types::I32)
        //     def_var(var0, 11)
        //     def_var(var1, 13)
        //     let v0 = use_var(var0)
        //     let v1 = use_var(var1)
        //     let v2 =v0 + v1
        //     def_var(var0, v2)
        //     use_var(var0)
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            let var0 = function_builder.declare_var(types::I32);
            let var1 = function_builder.declare_var(types::I32);

            // write
            let value_s0 = function_builder.ins().iconst(types::I32, 11);
            let value_s1 = function_builder.ins().iconst(types::I32, 13);
            function_builder.def_var(var0, value_s0);
            function_builder.def_var(var1, value_s1);

            // read
            let value_0 = function_builder.use_var(var0);
            let value_1 = function_builder.use_var(var1);
            let value_2 = function_builder.ins().iadd(value_0, value_1);

            // overwrite var0
            function_builder.def_var(var0, value_2);

            // read var0 again
            let value_3 = function_builder.use_var(var0);

            function_builder.ins().return_(&[value_3]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        let fn_main: extern "C" fn() -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(fn_main(), 24);

        unsafe { code_generator.generator_module.free_memory() };
    }

    /// Test calling external function by function pointer using `call_indirect` instruction.
    #[test]
    fn test_code_generator_jit_indirect_call() {
        let jit_module = new_jit_module(vec![]);
        let mut code_generator = CodeGenerator::new(jit_module);

        let pointer_type = code_generator.get_type_of_pointer();

        // Building function "main"
        //
        // ```rust
        // sig add(i32, i32) -> i32;
        //
        // fn main(ptr) -> int {
        //     call_indirect(ptr, 11, 13)
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.params.push(AbiParam::new(pointer_type));
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.ins().iconst(types::I32, 13);
            let value_2 = function_builder.block_params(entry_block)[0];

            // The signature of the function "add"
            let mut fn_add_sig = code_generator.generator_module.make_signature();
            fn_add_sig.params.push(AbiParam::new(types::I32));
            fn_add_sig.params.push(AbiParam::new(types::I32));
            fn_add_sig.returns.push(AbiParam::new(types::I32));

            let fn_add_sig_ref = function_builder.import_signature(fn_add_sig);

            let call0 =
                function_builder
                    .ins()
                    .call_indirect(fn_add_sig_ref, value_2, &[value_0, value_1]);

            let value_3 = function_builder.inst_results(call0)[0];

            function_builder.ins().return_(&[value_3]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        let fn_main: extern "C" fn(*const u8) -> i32 =
            unsafe { std::mem::transmute(func_main_ptr) };

        // Get the pointer of the Rust function `add`
        let fn_add_ptr = add as *const u8;

        assert_eq!(fn_main(fn_add_ptr), 24);

        unsafe { code_generator.generator_module.free_memory() };
    }

    #[test]
    fn test_code_generator_jit_conditional_branch() {
        let jit_module = new_jit_module(vec![]);
        let mut code_generator = CodeGenerator::new(jit_module);

        // Building function "main"
        //
        // ```pseudo
        // fn main(in) -> int {
        //     if in > 50 {
        //         1
        //     } else {
        //         0
        //     }
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.params.push(AbiParam::new(types::I32));
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            let exit_block = function_builder.create_block();

            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.append_block_params_for_function_returns(exit_block);

            // conditional branch
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);
            let value_in = function_builder.block_params(entry_block)[0];
            let value_0 = function_builder.ins().iconst(types::I32, 0);
            let value_1 = function_builder.ins().iconst(types::I32, 1);
            let value_cmp =
                function_builder
                    .ins()
                    .icmp_imm(IntCC::UnsignedGreaterThan, value_in, 50);
            function_builder.ins().brif(
                value_cmp,
                exit_block,
                &[BlockArg::Value(value_1)],
                exit_block,
                &[BlockArg::Value(value_0)],
            );

            // exit block
            function_builder.switch_to_block(exit_block);
            function_builder.seal_block(exit_block);
            let value_out = function_builder.block_params(exit_block)[0];
            function_builder.ins().return_(&[value_out]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        let fn_main: extern "C" fn(i32) -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(fn_main(30), 0);
        assert_eq!(fn_main(50), 0);
        assert_eq!(fn_main(51), 1);
        assert_eq!(fn_main(80), 1);

        unsafe { code_generator.generator_module.free_memory() };
    }

    #[test]
    fn test_code_generator_jit_loop() {
        let jit_module = new_jit_module(vec![]);
        let mut code_generator = CodeGenerator::new(jit_module);

        // Building function "main"
        //
        // ```pseudo
        // fn main(max) -> int {
        //     block_entry:
        //         jump block_loop(0,0)
        //     block_loop(sum,i)
        //         if i > max {
        //             jump block_exit(sum)
        //         } else {
        //             let sum = sum + i
        //             let i = i + 1
        //             jump block_loop(sum, i)
        //         }
        //     block_exit(sum)
        //         return sum
        // }
        // ```
        let func_main_id = {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.params.push(AbiParam::new(types::I32));
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            let loop_block = function_builder.create_block();
            let exit_block = function_builder.create_block();

            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.append_block_params_for_function_returns(exit_block);

            // entry block
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);
            let value_max = function_builder.block_params(entry_block)[0];
            let value_0 = function_builder.ins().iconst(types::I32, 0);
            function_builder.ins().jump(
                loop_block,
                &[BlockArg::Value(value_0), BlockArg::Value(value_0)],
            );

            // loop block
            function_builder.append_block_param(loop_block, types::I32); // sum
            function_builder.append_block_param(loop_block, types::I32); // i

            function_builder.switch_to_block(loop_block);
            let value_sum = function_builder.block_params(loop_block)[0];
            let value_i = function_builder.block_params(loop_block)[1];
            let value_cmp =
                function_builder
                    .ins()
                    .icmp(IntCC::UnsignedGreaterThan, value_i, value_max);

            let next_sum = function_builder.ins().iadd(value_sum, value_i);
            let next_i = function_builder.ins().iadd_imm(value_i, 1);

            function_builder.ins().brif(
                value_cmp,
                exit_block,
                &[BlockArg::Value(value_sum)],
                loop_block,
                &[BlockArg::Value(next_sum), BlockArg::Value(next_i)],
            );

            // exit block
            function_builder.switch_to_block(exit_block);
            function_builder.seal_block(exit_block);
            let value_out = function_builder.block_params(exit_block)[0];
            function_builder.ins().return_(&[value_out]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Display the text of IR
            // `println!("{}", func_main.display());`

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_main_id
        };

        code_generator
            .generator_module
            .finalize_definitions()
            .unwrap();

        let func_main_ptr = code_generator
            .generator_module
            .get_finalized_function(func_main_id);

        let fn_main: extern "C" fn(i32) -> i32 = unsafe { std::mem::transmute(func_main_ptr) };

        assert_eq!(fn_main(10), 55);
        assert_eq!(fn_main(100), 5050);

        unsafe { code_generator.generator_module.free_memory() };
    }

    #[test]
    fn test_code_generator_object_define_functions() {
        let object_module = new_object_module("test", None, None);
        let mut code_generator = CodeGenerator::new(object_module);

        // Building function "inc"
        //
        // ```pseudo
        // fn inc (a:i32) -> i32 {
        //    a+11
        // }
        // ```
        let func_inc_id = {
            let mut func_inc_sig = code_generator.generator_module.make_signature();
            func_inc_sig.params.push(AbiParam::new(types::I32));
            func_inc_sig.returns.push(AbiParam::new(types::I32));

            let func_inc_id = code_generator
                .generator_module
                .declare_function("inc", Linkage::Local, &func_inc_sig)
                .unwrap();

            let mut func_inc = Function::with_name_signature(
                UserFuncName::user(0, func_inc_id.as_u32()),
                func_inc_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_inc, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();

            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);

            function_builder.seal_block(entry_block);

            let value_0 = function_builder.ins().iconst(types::I32, 11);
            let value_1 = function_builder.block_params(entry_block)[0];
            let value_2 = function_builder.ins().iadd(value_0, value_1);
            function_builder.ins().return_(&[value_2]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            code_generator.generator_context.func = func_inc;

            code_generator
                .generator_module
                .define_function(func_inc_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            func_inc_id
        };

        // Building function "main"
        //
        // ```pseudo
        // fn main () -> i32 {
        //    inc(13)
        // }
        // ```
        {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let func_inc_ref = code_generator
                .generator_module
                .declare_func_in_func(func_inc_id, function_builder.func);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            let value0 = function_builder.ins().iconst(types::I32, 13);
            let call0 = function_builder.ins().call(func_inc_ref, &[value0]);
            let value1 = {
                let results = function_builder.inst_results(call0);
                results[0]
            };
            function_builder.ins().return_(&[value1]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);
        }

        // Finalize all relocations and output an object.
        //
        // https://docs.rs/cranelift-object/latest/cranelift_object/struct.ObjectModule.html#method.finish
        let product = code_generator.generator_module.finish();

        // Save the object file to disk.
        let binary = product.emit().unwrap();

        let tmp_dir = std::env::temp_dir();
        let tmp_object_file = tmp_dir.join("test-ancc-code-generator-define-functions.o");
        std::fs::write(&tmp_object_file, binary).unwrap();

        // Use command `gcc -o test.elf test.o` to link the object file into an executable, and then execute it.
        let tmp_executable_file = tmp_dir.join("test-ancc-code-generator-define-functions.elf");
        let output = std::process::Command::new("gcc")
            .args([
                "-static",
                "-o",
                tmp_executable_file.to_str().unwrap(),
                tmp_object_file.to_str().unwrap(),
            ])
            .output()
            .expect("failed to execute GCC to link object file");

        assert!(output.status.success(), "failed to link object file");

        let output = std::process::Command::new(tmp_executable_file.to_str().unwrap())
            .output()
            .expect("failed to execute process");

        assert_eq!(output.status.code(), Some(24));

        // Remove the temporary files
        std::fs::remove_file(tmp_object_file).unwrap();
        std::fs::remove_file(tmp_executable_file).unwrap();
    }

    #[test]
    fn test_code_generator_object_define_data() {
        let object_module = new_object_module("test", None, None);
        let mut code_generator = CodeGenerator::new(object_module);

        let pointer_type = code_generator.get_type_of_pointer();

        let bin0 = 11_i32.to_le_bytes().to_vec();
        let bin1 = 13_i32.to_le_bytes().to_vec();

        let data_id0 = code_generator
            .define_read_only_data("d0", bin0, Some(2), false, false)
            .unwrap();
        let data_id1 = code_generator
            .define_read_write_data("d1", bin1, Some(2), false, false)
            .unwrap();
        let data_id2 = code_generator
            .define_uninitialized_data("d2", 4, Some(2), false, false)
            .unwrap();

        // Building function "main"
        //
        // ```pseudo
        // fn main() -> int {
        //     let d0 = read_only_data(11)
        //     let d1 = read_write_data(13)
        //     let d2 = uninitialized_data
        //
        //     let v0 = load(d1)
        //     let v1 = v0 + 17
        //     store(v1, d2)        ;; now d2 contains 30
        //     let v2 = load(d0)    ;; v2 is 11
        //     let v3 = load(d2)    ;; v3 is 30
        //     v2 + v3              ;; return 41
        // }
        // ```
        {
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            // read and write
            let gv0 = code_generator
                .generator_module
                .declare_data_in_func(data_id0, function_builder.func);
            let gv1 = code_generator
                .generator_module
                .declare_data_in_func(data_id1, function_builder.func);
            let gv2 = code_generator
                .generator_module
                .declare_data_in_func(data_id2, function_builder.func);

            let p0 = function_builder.ins().global_value(pointer_type, gv0);
            let p1 = function_builder.ins().global_value(pointer_type, gv1);
            let p2 = function_builder.ins().global_value(pointer_type, gv2);

            let mem_flags = MemFlags::new();

            let value_0 = function_builder.ins().load(types::I32, mem_flags, p1, 0);
            let value_1 = function_builder.ins().iadd_imm(value_0, 17);
            function_builder.ins().store(mem_flags, value_1, p2, 0);

            let value_2 = function_builder.ins().load(types::I32, mem_flags, p0, 0);
            let value_3 = function_builder.ins().load(types::I32, mem_flags, p2, 0);
            let value_add = function_builder.ins().iadd(value_2, value_3);

            function_builder.ins().return_(&[value_add]);
            function_builder.seal_all_blocks();
            function_builder.finalize();

            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);
        }

        // Finalize all relocations and output an object.
        let product = code_generator.generator_module.finish();

        // Save the object file to disk.
        let binary = product.emit().unwrap();

        let tmp_dir = std::env::temp_dir();
        let tmp_object_file = tmp_dir.join("test-ancc-code-generator-define-data.o");
        std::fs::write(&tmp_object_file, binary).unwrap();

        // Use command `gcc -o test.elf test.o` to link the object file into an executable, and then execute it.
        let tmp_executable_file = tmp_dir.join("test-ancc-code-generator-define-data.elf");
        let output = std::process::Command::new("gcc")
            .args([
                "-static",
                "-o",
                tmp_executable_file.to_str().unwrap(),
                tmp_object_file.to_str().unwrap(),
            ])
            .output()
            .expect("failed to execute GCC to link object file");

        assert!(output.status.success(), "failed to link object file");

        let output = std::process::Command::new(tmp_executable_file.to_str().unwrap())
            .output()
            .expect("failed to execute process");

        assert_eq!(output.status.code(), Some(41));

        // Remove the temporary files
        std::fs::remove_file(tmp_object_file).unwrap();
        std::fs::remove_file(tmp_executable_file).unwrap();
    }

    /// Test a program with multiple object files.
    #[test]
    fn test_code_generator_object_multiple_objects() {
        // This program consists of two object files, `lib.o` and `main.o`.
        //
        // Where `lib.o` contains a public data `num:i32` and a public function `get_and_inc() -> i32`
        // that returns the value of `num` plus 1.
        //
        // And `main.o` contains a function `main() -> i32` that write `41` to `num`,
        // and then calls `get_and_inc()` to get the value of `num` and returns it.

        let tmp_dir = std::env::temp_dir();
        let tmp_object_file_lib = tmp_dir.join("test-ancc-code-generator-multi-object-lib.o");
        let tmp_object_file_main = tmp_dir.join("test-ancc-code-generator-multi-object-main.o");
        let tmp_executable_file = tmp_dir.join("test-ancc-code-generator-multi-object.elf");

        // Build `lib.o`
        {
            let object_module = new_object_module("test", None, None);
            let mut code_generator = CodeGenerator::new(object_module);

            let pointer_type = code_generator.get_type_of_pointer();

            let data_id = code_generator
                .define_uninitialized_data("num", 4, Some(2), true, false)
                .unwrap();

            // Building function "get_and_inc"
            // ```pseudo
            // fn get_and_inc() -> i32 {
            //     let num = load(data_id)
            //     num + 1
            // }

            let mut func_sig = code_generator.generator_module.make_signature();
            func_sig.returns.push(AbiParam::new(types::I32));

            let func_id = code_generator
                .generator_module
                .declare_function("get_and_inc", Linkage::Export, &func_sig)
                .unwrap();

            let mut func =
                Function::with_name_signature(UserFuncName::user(0, func_id.as_u32()), func_sig);

            let mut function_builder =
                FunctionBuilder::new(&mut func, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            let gv = code_generator
                .generator_module
                .declare_data_in_func(data_id, function_builder.func);
            let p = function_builder.ins().global_value(pointer_type, gv);
            let mem_flags = MemFlags::new();
            let num = function_builder.ins().load(types::I32, mem_flags, p, 0);
            let num_plus_1 = function_builder.ins().iadd_imm(num, 1);
            function_builder.ins().return_(&[num_plus_1]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            code_generator.generator_context.func = func;

            code_generator
                .generator_module
                .define_function(func_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            // Finalize all relocations and output an object.
            let product = code_generator.generator_module.finish();

            // Save the object file to disk.
            let binary = product.emit().unwrap();

            std::fs::write(&tmp_object_file_lib, binary).unwrap();
        }

        // Build `main.o`
        {
            let object_module = new_object_module("test", None, None);
            let mut code_generator = CodeGenerator::new(object_module);

            let pointer_type = code_generator.get_type_of_pointer();

            // Import `num`.
            let data_id = code_generator
                .generator_module
                .declare_data("num", Linkage::Import, true, false)
                .unwrap();

            // Import `get_and_inc`.
            let mut func_get_and_inc_num_sig = code_generator.generator_module.make_signature();
            func_get_and_inc_num_sig
                .returns
                .push(AbiParam::new(types::I32));

            let func_get_and_inc_num_id = code_generator
                .generator_module
                .declare_function("get_and_inc", Linkage::Import, &func_get_and_inc_num_sig)
                .unwrap();

            // Building function "main"
            // ```pseudo
            // fn main() -> i32 {
            //     import num: i32
            //     import get_and_inc_num() -> i32
            //
            //     store(41, data_id)
            //     get_and_inc_num()
            // }
            // ```
            let mut func_main_sig = code_generator.generator_module.make_signature();
            func_main_sig.returns.push(AbiParam::new(types::I32));

            let func_main_id = code_generator
                .generator_module
                .declare_function("main", Linkage::Export, &func_main_sig)
                .unwrap();

            let mut func_main = Function::with_name_signature(
                UserFuncName::user(0, func_main_id.as_u32()),
                func_main_sig,
            );

            let mut function_builder =
                FunctionBuilder::new(&mut func_main, &mut code_generator.function_builder_context);

            let entry_block = function_builder.create_block();
            function_builder.append_block_params_for_function_params(entry_block);
            function_builder.switch_to_block(entry_block);
            function_builder.seal_block(entry_block);

            // Write 41 to `num`
            let gv = code_generator
                .generator_module
                .declare_data_in_func(data_id, function_builder.func);

            let p = function_builder.ins().symbol_value(pointer_type, gv);
            let mem_flags = MemFlags::new();
            let value_0 = function_builder.ins().iconst(types::I32, 41);
            function_builder.ins().store(mem_flags, value_0, p, 0);

            // Call `get_and_inc_num()` to get the value of `num` and return it.
            let func_get_and_inc_num_ref = code_generator
                .generator_module
                .declare_func_in_func(func_get_and_inc_num_id, function_builder.func);

            let call = function_builder.ins().call(func_get_and_inc_num_ref, &[]);
            let value = function_builder.inst_results(call)[0];
            function_builder.ins().return_(&[value]);

            function_builder.seal_all_blocks();
            function_builder.finalize();

            // Generate function's code
            code_generator.generator_context.func = func_main;

            code_generator
                .generator_module
                .define_function(func_main_id, &mut code_generator.generator_context)
                .unwrap();

            code_generator
                .generator_module
                .clear_context(&mut code_generator.generator_context);

            // Finalize all relocations and output an object.
            let product = code_generator.generator_module.finish();

            // Save the object file to disk.
            let binary = product.emit().unwrap();

            std::fs::write(&tmp_object_file_main, binary).unwrap();
        }

        // Use command `gcc -o test.elf lib.o main.o` to link the object file into an executable, and then execute it.
        let output = std::process::Command::new("gcc")
            .args([
                "-static",
                "-o",
                tmp_executable_file.to_str().unwrap(),
                tmp_object_file_lib.to_str().unwrap(),
                tmp_object_file_main.to_str().unwrap(),
            ])
            .output()
            .expect("failed to execute GCC to link object file");

        assert!(output.status.success(), "failed to link object file");

        let output = std::process::Command::new(tmp_executable_file.to_str().unwrap())
            .output()
            .expect("failed to execute process");

        assert_eq!(output.status.code(), Some(42));

        // Remove the temporary files
        std::fs::remove_file(tmp_object_file_lib).unwrap();
        std::fs::remove_file(tmp_object_file_main).unwrap();
        std::fs::remove_file(tmp_executable_file).unwrap();
    }
}
