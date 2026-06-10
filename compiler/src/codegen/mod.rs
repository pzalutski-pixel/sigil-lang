//! Code generation module - translates AST to LLVM IR
//!
//! Implements the Sigil Language Reference specification.
//!
//! Key design: ALL inputs and outputs are handles (pointers).
//! - Caller allocates handles, stores values, passes pointers
//! - Callee uses LOAD to read from input handles
//! - Callee uses STORE to write to output handles
//!
//! NOTE: a few entry points (IR dumping, alternate main compilation) are used
//! by tests or kept for tooling, but not by the default driver path.
#![allow(dead_code)]

mod expr;
mod extern_decl;

use inkwell::context::Context;
use inkwell::module::{Module, Linkage};
use inkwell::builder::Builder;
use inkwell::basic_block::BasicBlock;
use inkwell::values::{FunctionValue, PointerValue, IntValue, FloatValue};
use inkwell::types::{BasicMetadataTypeEnum, FunctionType};
use inkwell::AddressSpace;
use inkwell::targets::{TargetMachine, FileType};
use inkwell::passes::PassBuilderOptions;
use inkwell::IntPredicate;
use std::collections::HashMap;
use std::path::Path;

use crate::ast::{Behavior, Pattern, Executable, PlatformImpl, Node, NodeKind, Expr, Parameter, Contract, Type as AstType};

// =============================================================================
// Domain Types - Model Sigil concepts from the spec
// =============================================================================

/// Memory handle - opaque reference to allocated memory (Spec Section 3.1)
///
/// Properties per spec:
/// - Returned by ALLOC
/// - Carries: base address, size, type interpretation
/// - Opaque: no arithmetic on handles
#[derive(Debug, Clone, Copy)]
pub struct Handle<'ctx> {
    pub ptr: PointerValue<'ctx>,
    pub size: usize,
    /// Type interpretation per Section 2.2 (int, float, bytes)
    /// Used by LOAD to return correctly typed value per Section 4.1
    pub typ: HandleType,
}

/// Type interpretation for handles per Sigil Reference Section 2.2
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum HandleType {
    #[default]
    Int,
    Float,
    Bytes,
    /// A `string` value: the handle points at length-prefixed data
    /// (`[len: 8-byte LE][content]`). At an ABI boundary a string is passed and
    /// stored as an 8-byte pointer to that data (it declares no fixed size).
    String,
}

impl From<&AstType> for HandleType {
    fn from(ast_type: &AstType) -> Self {
        match ast_type {
            AstType::Int => HandleType::Int,
            AstType::Float => HandleType::Float,
            AstType::Bytes => HandleType::Bytes,
            AstType::String => HandleType::String,
        }
    }
}

/// Sigil value - result of expressions
///
/// Distinguishes between:
/// - Handle: pointer to memory (used for LOAD/STORE targets, CALL args)
/// - Immediate: raw value from LOAD or arithmetic (used in computations)
#[derive(Debug, Clone, Copy)]
pub enum SigilValue<'ctx> {
    Handle(Handle<'ctx>),
    Immediate(IntValue<'ctx>),
    Float(FloatValue<'ctx>),
}

impl<'ctx> SigilValue<'ctx> {
    /// Get as handle, or error
    pub fn as_handle(self) -> std::result::Result<Handle<'ctx>, CodeGenError> {
        match self {
            SigilValue::Handle(h) => Ok(h),
            _ => Err(CodeGenError::TypeError("Expected handle, got immediate".into())),
        }
    }

    /// Get as int immediate, or error
    pub fn as_int(self) -> std::result::Result<IntValue<'ctx>, CodeGenError> {
        match self {
            SigilValue::Immediate(i) => Ok(i),
            SigilValue::Handle(h) => Err(CodeGenError::TypeError(
                format!("Expected int immediate, got handle (size={})", h.size)
            )),
            SigilValue::Float(_) => Err(CodeGenError::TypeError(
                "Expected int immediate, got float".into()
            )),
        }
    }

    /// Get as float immediate, or error
    pub fn as_float(self) -> std::result::Result<FloatValue<'ctx>, CodeGenError> {
        match self {
            SigilValue::Float(f) => Ok(f),
            _ => Err(CodeGenError::TypeError("Expected float immediate".into())),
        }
    }
}

/// Output slot info for multi-output behaviors
#[derive(Debug, Clone)]
pub struct OutputSlot {
    pub name: String,
    pub offset: usize,
    pub size: usize,
    /// Interpretation of this output, so callers reading the field (e.g. a
    /// `string` slot holding a pointer) deref it correctly.
    pub typ: HandleType,
}

/// Result of a CALL - handle to output buffer with layout info
#[derive(Debug, Clone)]
pub struct CallOutputs<'ctx> {
    pub buffer: Handle<'ctx>,
    pub slots: Vec<OutputSlot>,
}

impl<'ctx> CallOutputs<'ctx> {
    /// Get handle to a specific field by name
    pub fn field(&self, name: &str) -> Option<(usize, usize)> {
        self.slots.iter()
            .find(|s| s.name == name)
            .map(|s| (s.offset, s.size))
    }
}

/// Behavior signature - captures calling convention
#[derive(Debug, Clone)]
pub struct BehaviorSig {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub input_sizes: Vec<usize>,
    pub outputs: Vec<OutputSlot>,
}

/// Offset for LOAD/STORE - can be constant or computed at runtime
#[derive(Debug, Clone, Copy)]
pub enum Offset<'ctx> {
    Const(usize),
    Dynamic(IntValue<'ctx>),
}

impl BehaviorSig {
    pub fn total_output_size(&self) -> usize {
        self.outputs.iter().map(|o| o.size).sum()
    }
}

// =============================================================================
// Target Platform (Section 12)
// =============================================================================

/// Target platform for code generation
///
/// Per Sigil Reference Section 12: Platform-specific implementations allow
/// behaviors to have different implementations for different platforms.
/// The compiler selects the appropriate implementation based on target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetPlatform {
    Windows,
    Linux,
    MacOS,
}

impl TargetPlatform {
    /// Detect platform from LLVM target triple string
    ///
    /// Examples:
    /// - "x86_64-pc-windows-msvc" -> Windows
    /// - "x86_64-unknown-linux-gnu" -> Linux
    /// - "x86_64-apple-darwin" -> MacOS
    pub fn from_triple(triple: &str) -> Self {
        if triple.contains("windows") {
            TargetPlatform::Windows
        } else if triple.contains("darwin") || triple.contains("apple") {
            TargetPlatform::MacOS
        } else {
            // Default to Linux for unknown targets
            TargetPlatform::Linux
        }
    }

    /// Platform name as used in IMPLEMENTATION[name] syntax
    ///
    /// Per Section 12.1: platform_name = "linux" | "windows" | "macos"
    pub fn as_str(&self) -> &'static str {
        match self {
            TargetPlatform::Windows => "windows",
            TargetPlatform::Linux => "linux",
            TargetPlatform::MacOS => "macos",
        }
    }
}

impl Default for TargetPlatform {
    fn default() -> Self {
        // Default based on compilation target
        #[cfg(target_os = "windows")]
        { TargetPlatform::Windows }
        #[cfg(target_os = "linux")]
        { TargetPlatform::Linux }
        #[cfg(target_os = "macos")]
        { TargetPlatform::MacOS }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        { TargetPlatform::Linux }
    }
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Clone)]
pub enum CodeGenError {
    TypeError(String),
    UndefinedVariable(String),
    UndefinedBehavior(String),
    UndefinedLabel(String),
    UndefinedField(String, String),
    LlvmError(String),
}

impl std::fmt::Display for CodeGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodeGenError::TypeError(msg) => write!(f, "Type error: {}", msg),
            CodeGenError::UndefinedVariable(name) => write!(f, "Undefined variable: {}", name),
            CodeGenError::UndefinedBehavior(name) => write!(f, "Undefined behavior: {}", name),
            CodeGenError::UndefinedLabel(name) => write!(f, "Undefined label: {}", name),
            CodeGenError::UndefinedField(var, field) => write!(f, "Undefined field: {}.{}", var, field),
            CodeGenError::LlvmError(msg) => write!(f, "LLVM error: {}", msg),
        }
    }
}

impl From<String> for CodeGenError {
    fn from(s: String) -> Self {
        CodeGenError::LlvmError(s)
    }
}

pub(crate) type Result<T> = std::result::Result<T, CodeGenError>;

// =============================================================================
// Code Generator
// =============================================================================

/// Code generator - compiles Sigil behaviors to LLVM IR
pub struct CodeGen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,

    /// Target platform for code generation (Section 12)
    pub(crate) target_platform: TargetPlatform,

    /// Variable bindings: name -> SigilValue
    pub(crate) variables: HashMap<String, SigilValue<'ctx>>,

    /// Call results for field access: variable name -> CallOutputs
    pub(crate) call_outputs: HashMap<String, CallOutputs<'ctx>>,

    /// Compiled behavior signatures
    pub(crate) behavior_sigs: HashMap<String, BehaviorSig>,

    /// Compiled behavior functions
    pub(crate) behavior_fns: HashMap<String, FunctionValue<'ctx>>,

    /// Current function being compiled (for runtime bounds checks)
    pub(crate) current_function: Option<FunctionValue<'ctx>>,

    /// Current memory scope (SigilScope*) for the ALLOC/FREE/SCOPE model.
    /// Set per behavior; None outside a behavior body (executables/patterns),
    /// where ALLOC falls back to a stack allocation.
    pub(crate) current_scope: Option<PointerValue<'ctx>>,

    /// Scalar locals (int/float) that are reassigned more than once in the
    /// current behavior, so they must be memory-backed to survive loop
    /// back-edges (a one-shot SSA value would read its initial value forever).
    pub(crate) reassigned_locals: std::collections::HashSet<String>,

    /// Memory slots for reassigned scalar locals: name -> (alloca ptr, type).
    /// Allocated in the entry block; LLVM mem2reg promotes them back to SSA/phi.
    pub(crate) mutable_slots:
        HashMap<String, (PointerValue<'ctx>, inkwell::types::BasicTypeEnum<'ctx>)>,
}

impl<'ctx> CodeGen<'ctx> {
    /// Create a new code generator with the specified target platform
    pub fn new(context: &'ctx Context, module_name: &str, target_platform: TargetPlatform) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        CodeGen {
            context,
            module,
            builder,
            target_platform,
            variables: HashMap::new(),
            call_outputs: HashMap::new(),
            behavior_sigs: HashMap::new(),
            behavior_fns: HashMap::new(),
            current_function: None,
            current_scope: None,
            reassigned_locals: std::collections::HashSet::new(),
            mutable_slots: HashMap::new(),
        }
    }

    /// Create a new code generator using the default platform (based on compilation target)
    pub fn new_default(context: &'ctx Context, module_name: &str) -> Self {
        Self::new(context, module_name, TargetPlatform::default())
    }

    // =========================================================================
    // Platform Selection (Section 12)
    // =========================================================================

    /// Select the appropriate implementation for the target platform
    ///
    /// Per Section 12.3 Semantics:
    /// - Compiler selects one implementation based on target platform
    /// - Platform-qualified implementations override unqualified
    /// - Unqualified IMPLEMENTATION (no platform) applies to all platforms
    fn select_implementation<'a>(&self, behavior: &'a Behavior) -> Option<&'a PlatformImpl> {
        let platform_name = self.target_platform.as_str();

        // First: try to find platform-specific implementation
        if let Some(impl_) = behavior.implementations.iter()
            .find(|i| i.platform.as_deref() == Some(platform_name))
        {
            return Some(impl_);
        }

        // Second: fall back to unqualified implementation (platform: None)
        behavior.implementations.iter()
            .find(|i| i.platform.is_none())
    }

    /// Get CRT function name with platform-appropriate prefix
    ///
    /// Windows MSVC CRT uses underscore prefix: _read, _write, _open, _close, _stat
    /// POSIX (Linux/macOS) uses no prefix: read, write, open, close, stat
    pub(crate) fn crt_name(&self, base: &str) -> String {
        match self.target_platform {
            TargetPlatform::Windows => format!("_{}", base),
            TargetPlatform::Linux | TargetPlatform::MacOS => base.to_string(),
        }
    }

    // =========================================================================
    // Public API
    // =========================================================================

    /// Dump LLVM IR to string (for debugging)
    pub fn dump_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }

    /// Write object file
    pub fn write_object_file(&self, target_machine: &TargetMachine, path: &Path) -> std::result::Result<(), String> {
        target_machine
            .write_to_file(&self.module, FileType::Object, path)
            .map_err(|e| e.to_string())
    }

    /// Run LLVM optimization passes on the module
    ///
    /// Uses the new LLVM pass manager with default -O2 optimization level.
    /// Includes key passes: mem2reg, instcombine, simplifycfg, inline, gvn
    pub fn run_optimization_passes(&self, target_machine: &TargetMachine) -> std::result::Result<(), String> {
        let pass_opts = PassBuilderOptions::create();
        // Use default<O2> for balanced optimization
        // This includes: mem2reg, instcombine, simplifycfg, inline, gvn, etc.
        self.module.run_passes("default<O2>", target_machine, pass_opts)
            .map_err(|e| e.to_string())
    }

    /// Compile a behavior as entry point (main function)
    pub fn compile_as_main(&mut self, behavior: &Behavior) -> std::result::Result<FunctionValue<'ctx>, String> {
        self.compile_main(behavior).map_err(|e| e.to_string())
    }

    /// Compile a behavior as callable function
    pub fn compile_as_function(&mut self, behavior: &Behavior, full_name: &str) -> std::result::Result<FunctionValue<'ctx>, String> {
        self.compile_function(behavior, full_name).map_err(|e| e.to_string())
    }

    /// Compile an executable as main entry point (per Section 11.2)
    ///
    /// EXECUTABLE has no CONTRACT - no inputs/outputs to allocate.
    /// ENTRY block contains only comp_statement (CALL, SET, SPAWN, etc.)
    pub fn compile_as_executable(&mut self, executable: &Executable) -> std::result::Result<FunctionValue<'ctx>, String> {
        self.compile_executable_main(executable).map_err(|e| e.to_string())
    }

    /// Declare an external behavior function (for incremental compilation)
    ///
    /// When compiling behavior A that calls behavior B, we need to declare B
    /// as an external function so the linker can resolve it later.
    pub fn declare_external_behavior(&mut self, name: &str, contract: &crate::ast::Contract) {
        let fn_name = format!("sigil_{}", name.replace('-', "_").replace('/', "_"));

        // Check if already declared
        if self.module.get_function(&fn_name).is_some() {
            return;
        }

        // Build function signature from contract
        let sig = self.build_sig_from_contract(name, contract);
        self.behavior_sigs.insert(name.to_string(), sig);

        // Create function type
        let fn_type = self.build_function_type_from_contract(contract);

        // Declare as external
        self.module.add_function(&fn_name, fn_type, Some(Linkage::External));
    }

    /// Declare an external pattern function (for SPAWN across modules)
    ///
    /// Patterns are compiled to separate .o files. When the entry behavior
    /// uses SPAWN, it needs the pattern function declared as external.
    pub fn declare_external_pattern(&mut self, name: &str, pattern: &crate::ast::Pattern) {
        let fn_name = format!("sigil_{}", name.replace('-', "_").replace('/', "_"));

        // Check if already declared
        if self.module.get_function(&fn_name).is_some() {
            return;
        }

        // Calculate total memory size for pattern
        let memory_size: usize = pattern.memory.iter().map(|m| m.size).sum();

        // Register signature for SPAWN codegen
        let sig = BehaviorSig {
            name: name.to_string(),
            input_sizes: vec![memory_size],
            outputs: vec![],
        };
        self.behavior_sigs.insert(name.to_string(), sig);

        // Pattern takes single pointer parameter (memory buffer)
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let fn_type = self.context.void_type().fn_type(&[ptr_type.into()], false);

        self.module.add_function(&fn_name, fn_type, Some(Linkage::External));
    }

    /// Physical byte size of a parameter slot. A `string` carries no declared
    /// size (it is self-describing); at an ABI boundary it is passed/stored as
    /// an 8-byte pointer to its length-prefixed data. Everything else uses its
    /// declared size.
    fn param_phys_size(p: &Parameter) -> usize {
        match p.typ {
            AstType::String => 8,
            _ => p.size,
        }
    }

    /// Build output slot layout (name/offset/size/type) for a contract, using
    /// physical slot sizes so a `string` output occupies a pointer-sized slot.
    fn output_slots(contract: &Contract) -> Vec<OutputSlot> {
        let mut slots = Vec::with_capacity(contract.outputs.len());
        let mut offset = 0usize;
        for o in &contract.outputs {
            let size = Self::param_phys_size(o);
            slots.push(OutputSlot {
                name: o.name.clone(),
                offset,
                size,
                typ: HandleType::from(&o.typ),
            });
            offset += size;
        }
        slots
    }

    /// Build a BehaviorSig (calling convention) from a contract.
    fn build_sig_from_contract(&self, name: &str, contract: &Contract) -> BehaviorSig {
        BehaviorSig {
            name: name.to_string(),
            input_sizes: contract.inputs.iter().map(Self::param_phys_size).collect(),
            outputs: Self::output_slots(contract),
        }
    }

    /// Build LLVM function type from contract
    fn build_function_type_from_contract(&self, contract: &crate::ast::Contract) -> FunctionType<'ctx> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let void_type = self.context.void_type();

        // All parameters are pointers (inputs + outputs)
        let param_count = contract.inputs.len() + contract.outputs.len();
        let param_types: Vec<BasicMetadataTypeEnum> = (0..param_count)
            .map(|_| ptr_type.into())
            .collect();

        void_type.fn_type(&param_types, false)
    }

    // =========================================================================
    // Runtime Bounds Checking (Sigil Reference Section 14.1)
    // =========================================================================

    /// Emit runtime bounds check per Sigil Reference Section 14.1:
    /// Precondition: offset + size <= allocated_size
    ///
    /// Generates code that aborts if bounds are violated.
    pub(crate) fn emit_bounds_check(
        &mut self,
        offset: IntValue<'ctx>,
        access_size: usize,
        allocated_size: usize,
        function: FunctionValue<'ctx>,
    ) -> Result<()> {
        let i64_type = self.context.i64_type();

        // Calculate end_offset = offset + access_size
        let size_val = i64_type.const_int(access_size as u64, false);
        let end_offset = self.builder.build_int_add(offset, size_val, "end_offset")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Compare: end_offset > allocated_size
        let alloc_size_val = i64_type.const_int(allocated_size as u64, false);
        let is_violation = self.builder.build_int_compare(
            IntPredicate::UGT, end_offset, alloc_size_val, "bounds_check"
        ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Create blocks for bounds check
        let violation_block = self.context.append_basic_block(function, "bounds_violation");
        let continue_block = self.context.append_basic_block(function, "bounds_ok");

        self.builder.build_conditional_branch(is_violation, violation_block, continue_block)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Violation block: call abort
        self.builder.position_at_end(violation_block);
        let abort_fn = self.get_or_declare_abort();
        self.builder.build_call(abort_fn, &[], "")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder.build_unreachable()
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Continue block: proceed with operation
        self.builder.position_at_end(continue_block);

        Ok(())
    }

    /// Get or declare the abort function for bounds violations
    fn get_or_declare_abort(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("abort") { return f; }
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module.add_function("abort", fn_type, Some(Linkage::External))
    }

    // =========================================================================
    // Handle Operations (Spec Section 4.1)
    // =========================================================================

    /// Allocate a handle (ALLOC primitive) per Section 3.1
    pub(crate) fn alloc_handle(&mut self, size: usize, typ: HandleType) -> Result<Handle<'ctx>> {
        let i8_type = self.context.i8_type();
        let array_type = i8_type.array_type(size as u32);
        let ptr = self.builder.build_alloca(array_type, "handle")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(Handle { ptr, size, typ })
    }

    /// Allocate a handle for the language-level ALLOC primitive, via the runtime
    /// scope allocator (heap, freed at FREE / END_SCOPE / behavior return per the
    /// memory model). Falls back to a stack `alloca` when there is no behavior
    /// scope (a context compiled without one), preserving prior behavior there.
    /// Compiler-internal scratch keeps using `alloc_handle` (stack) — it is not
    /// the language's ALLOC and is not governed by the memory model.
    pub(crate) fn scope_alloc_handle(&mut self, size: usize, typ: HandleType) -> Result<Handle<'ctx>> {
        let scope = match self.current_scope {
            Some(s) => s,
            None => return self.alloc_handle(size, typ),
        };
        let alloc_fn = self.get_or_declare_sigil_scope_alloc();
        let size_val = self.context.i64_type().const_int(size as u64, false);
        let call = self.builder
            .build_call(alloc_fn, &[scope.into(), size_val.into()], "alloc")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let ptr = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("sigil_scope_alloc returned void".into()))?
            .into_pointer_value();
        Ok(Handle { ptr, size, typ })
    }

    /// Create a root memory scope (parent = NULL) for an entry point / behavior
    /// invocation. Its allocations are freed when the scope is destroyed.
    fn create_root_scope(&mut self) -> Result<PointerValue<'ctx>> {
        let create_fn = self.get_or_declare_sigil_scope_create();
        let null_scope = self.context.ptr_type(AddressSpace::default()).const_null();
        Ok(self.builder
            .build_call(create_fn, &[null_scope.into()], "root_scope")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("sigil_scope_create returned void".into()))?
            .into_pointer_value())
    }

    /// Destroy a root scope, freeing whatever ALLOCs remain live in it.
    fn destroy_root_scope(&mut self, scope: PointerValue<'ctx>) -> Result<()> {
        let destroy_fn = self.get_or_declare_sigil_scope_destroy();
        self.builder
            .build_call(destroy_fn, &[scope.into()], "")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Load value from handle (LOAD primitive)
    /// Supports both constant and dynamic offsets per spec Section 4.1
    /// Emits runtime bounds checks for dynamic offsets per Section 14.1
    /// Returns correctly typed SigilValue based on handle type per Section 2.2
    pub(crate) fn load(&mut self, handle: Handle<'ctx>, size: usize, offset: Offset<'ctx>) -> Result<SigilValue<'ctx>> {
        // Runtime bounds check for dynamic offsets (Section 14.1)
        if let Offset::Dynamic(offset_val) = offset {
            if let Some(function) = self.current_function {
                if handle.size > 0 {
                    self.emit_bounds_check(offset_val, size, handle.size, function)?;
                }
            }
        }

        let ptr = match offset {
            Offset::Const(0) => handle.ptr,
            Offset::Const(off) => {
                let i8_type = self.context.i8_type();
                let offset_val = self.context.i64_type().const_int(off as u64, false);
                unsafe {
                    self.builder.build_gep(i8_type, handle.ptr, &[offset_val], "load_ptr")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                }
            }
            Offset::Dynamic(offset_val) => {
                let i8_type = self.context.i8_type();
                unsafe {
                    self.builder.build_gep(i8_type, handle.ptr, &[offset_val], "load_ptr")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                }
            }
        };

        // Return correctly typed value based on handle type per Section 2.2
        match handle.typ {
            HandleType::Float => {
                let loaded = if size == 4 {
                    let float_type = self.context.f32_type();
                    self.builder.build_load(float_type, ptr, "load_float")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                } else {
                    let float_type = self.context.f64_type();
                    self.builder.build_load(float_type, ptr, "load_float")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                };
                Ok(SigilValue::Float(loaded.into_float_value()))
            }
            HandleType::Int | HandleType::Bytes | HandleType::String => {
                let int_type = self.int_type((size * 8) as u32);
                let loaded = self.builder.build_load(int_type, ptr, "load")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Immediate(loaded.into_int_value()))
            }
        }
    }

    /// Store value to handle (STORE primitive)
    /// Supports both constant and dynamic offsets per spec Section 4.1
    /// Emits runtime bounds checks for dynamic offsets per Section 14.1
    pub(crate) fn store(&mut self, handle: Handle<'ctx>, value: SigilValue<'ctx>, size: usize, offset: Offset<'ctx>) -> Result<()> {
        // Runtime bounds check for dynamic offsets (Section 14.1)
        if let Offset::Dynamic(offset_val) = offset {
            if let Some(function) = self.current_function {
                if handle.size > 0 {
                    self.emit_bounds_check(offset_val, size, handle.size, function)?;
                }
            }
        }

        let ptr = match offset {
            Offset::Const(0) => handle.ptr,
            Offset::Const(off) => {
                let i8_type = self.context.i8_type();
                let offset_val = self.context.i64_type().const_int(off as u64, false);
                unsafe {
                    self.builder.build_gep(i8_type, handle.ptr, &[offset_val], "store_ptr")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                }
            }
            Offset::Dynamic(offset_val) => {
                let i8_type = self.context.i8_type();
                unsafe {
                    self.builder.build_gep(i8_type, handle.ptr, &[offset_val], "store_ptr")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                }
            }
        };

        let store_type = self.int_type((size * 8) as u32);

        match value {
            SigilValue::Immediate(i) => {
                // Truncate/extend to target size
                let sized = self.resize_int(i, store_type)?;
                self.builder.build_store(ptr, sized)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }
            SigilValue::Handle(h) => {
                // Store pointer as integer
                let ptr_int = self.builder.build_ptr_to_int(h.ptr, self.context.i64_type(), "ptr_to_int")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                let sized = self.resize_int(ptr_int, store_type)?;
                self.builder.build_store(ptr, sized)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }
            SigilValue::Float(f) => {
                self.builder.build_store(ptr, f)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Wrap immediate value in a handle (for passing literals to CALL)
    pub(crate) fn immediate_to_handle(&mut self, value: SigilValue<'ctx>) -> Result<Handle<'ctx>> {
        match value {
            SigilValue::Handle(h) => Ok(h),
            SigilValue::Immediate(i) => {
                let size = (i.get_type().get_bit_width() / 8) as usize;
                let handle = self.alloc_handle(size.max(8), HandleType::Int)?;
                self.builder.build_store(handle.ptr, i)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(handle)
            }
            SigilValue::Float(f) => {
                let size = if f.get_type() == self.context.f32_type() { 4 } else { 8 };
                let handle = self.alloc_handle(size, HandleType::Float)?;
                self.builder.build_store(handle.ptr, f)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(handle)
            }
        }
    }

    /// Get field handle from call outputs
    pub(crate) fn get_field(&mut self, outputs: &CallOutputs<'ctx>, field: &str) -> Result<Handle<'ctx>> {
        let slot = outputs.slots.iter().find(|s| s.name == field)
            .ok_or_else(|| CodeGenError::UndefinedField("call_result".into(), field.into()))?;
        let (offset, size, typ) = (slot.offset, slot.size, slot.typ);

        let ptr = if offset > 0 {
            let i8_type = self.context.i8_type();
            let offset_val = self.context.i64_type().const_int(offset as u64, false);
            unsafe {
                self.builder.build_gep(i8_type, outputs.buffer.ptr, &[offset_val], &format!("{}_ptr", field))
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            }
        } else {
            outputs.buffer.ptr
        };

        Ok(Handle { ptr, size, typ })
    }

    // =========================================================================
    // Behavior Compilation
    // =========================================================================

    /// Compile behavior as main entry point
    fn compile_main(&mut self, behavior: &Behavior) -> Result<FunctionValue<'ctx>> {
        let i32_type = self.context.i32_type();

        self.declare_externals();

        let fn_type = i32_type.fn_type(&[], false);
        let function = self.module.add_function("main", fn_type, None);
        let entry = self.context.append_basic_block(function, "entry");

        self.builder.position_at_end(entry);
        self.variables.clear();
        self.call_outputs.clear();
        self.current_function = Some(function);

        // Root memory scope for this entry point, so CALL output buffers / ALLOCs
        // are heap-backed rather than stack allocas (see compile_executable_main).
        let saved_scope = self.current_scope;
        let root_scope = self.create_root_scope()?;
        self.current_scope = Some(root_scope);

        // Allocate input handles with correct type per Section 2.2
        for input in &behavior.contract.inputs {
            let handle = self.alloc_handle(Self::param_phys_size(input), HandleType::from(&input.typ))?;
            self.variables.insert(input.name.clone(), SigilValue::Handle(handle));
        }

        // Allocate output handles with correct type per Section 2.2
        for output in &behavior.contract.outputs {
            let handle = self.alloc_handle(Self::param_phys_size(output), HandleType::from(&output.typ))?;
            self.variables.insert(output.name.clone(), SigilValue::Handle(handle));
        }

        // Compile body - select implementation per Section 12.3. First scan for
        // reassigned scalar locals so loop counters get memory-backed.
        self.mutable_slots.clear();
        let impl_nodes = self.select_implementation(behavior).map(|i| i.nodes.clone());
        if let Some(nodes) = impl_nodes {
            self.reassigned_locals = Self::collect_reassigned(&nodes);
            self.compile_impl(&nodes, function)?;
        } else if let Some(nodes) = behavior.composition.as_ref().map(|c| c.nodes.clone()) {
            self.reassigned_locals = Self::collect_reassigned(&nodes);
            self.compile_comp(&nodes, function)?;
        }
        self.reassigned_locals.clear();

        // Return status. Free the root scope first, on the normal fall-through exit.
        if self.needs_terminator() {
            self.destroy_root_scope(root_scope)?;
            let status = self.get_return_status(behavior)?;
            self.builder.build_return(Some(&status))
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        self.current_scope = saved_scope;

        Ok(function)
    }

    /// Compile executable as main entry point (Section 11.2)
    ///
    /// EXECUTABLE differs from BEHAVIOR:
    /// - No CONTRACT: no input/output handles to allocate
    /// - ENTRY block: contains comp_statement only (CALL, SET, SPAWN, etc.)
    /// - Returns i32: exit status (0 = success)
    fn compile_executable_main(&mut self, executable: &Executable) -> Result<FunctionValue<'ctx>> {
        let i32_type = self.context.i32_type();

        self.declare_externals();

        let fn_type = i32_type.fn_type(&[], false);
        let function = self.module.add_function("main", fn_type, None);
        let entry = self.context.append_basic_block(function, "entry");

        self.builder.position_at_end(entry);
        self.variables.clear();
        self.call_outputs.clear();
        self.current_function = Some(function);

        // Create a root memory scope so CALL output buffers and ALLOCs in the
        // ENTRY composition are heap-backed (freed at exit) rather than stack
        // allocas. Without it, an executable's pipeline (e.g. read's 65536-byte
        // buffer + concat's 131072-byte result) piles a ~260 KB stack frame into
        // main — the source of the intermittent 0xC0000409 stack fast-fail seen
        // in socket-client executables. Behaviors already get this in
        // `compile_function`; the entry points were the gap.
        let saved_scope = self.current_scope;
        let root_scope = self.create_root_scope()?;
        self.current_scope = Some(root_scope);

        // No CONTRACT - no handles to allocate
        // Compile ENTRY block nodes directly
        self.compile_comp(&executable.entry, function)?;

        // Return 0 for success. Free the root scope first, on the normal
        // fall-through exit (matching the behavior path).
        if self.needs_terminator() {
            self.destroy_root_scope(root_scope)?;
            let zero = i32_type.const_zero();
            self.builder.build_return(Some(&zero))
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        self.current_scope = saved_scope;

        Ok(function)
    }

    /// Compile behavior as callable function
    ///
    /// Per spec: ALL parameters are pointers (handles)
    /// Signature: void behavior(ptr in1, ptr in2, ..., ptr out1, ptr out2, ...)
    ///
    /// NATIVE behaviors (Section 6.4): declared as external, no body compiled.
    /// Implementation provided by runtime library.
    fn compile_function(&mut self, behavior: &Behavior, full_name: &str) -> Result<FunctionValue<'ctx>> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let void_type = self.context.void_type();

        // Build parameter list: all pointers
        let param_count = behavior.contract.inputs.len() + behavior.contract.outputs.len();
        let param_types: Vec<_> = (0..param_count)
            .map(|_| ptr_type.into())
            .collect();

        let fn_type = void_type.fn_type(&param_types, false);
        let fn_name = format!("sigil_{}", full_name.replace('/', "_").replace('-', "_"));

        // NATIVE behaviors: declare as external, no body
        // Implementation is provided by the runtime library
        if behavior.is_native {
            let function = self.module.add_function(&fn_name, fn_type, Some(Linkage::External));

            // Register signature for callers
            let sig = self.build_sig_from_contract(full_name, &behavior.contract);
            self.behavior_sigs.insert(full_name.to_string(), sig);
            self.behavior_fns.insert(full_name.to_string(), function);

            return Ok(function);
        }

        self.declare_externals();

        let function = self.module.add_function(&fn_name, fn_type, None);
        let entry = self.context.append_basic_block(function, "entry");

        // Save state
        let saved_vars = std::mem::take(&mut self.variables);
        let saved_outputs = std::mem::take(&mut self.call_outputs);
        let saved_function = self.current_function;

        self.builder.position_at_end(entry);
        self.current_function = Some(function);

        // Create this behavior's root memory scope. ALLOCs draw from it; it is
        // destroyed before the function returns, freeing whatever is still live.
        let saved_scope = self.current_scope;
        let root_scope = self.create_root_scope()?;
        self.current_scope = Some(root_scope);

        // Map input parameters to handles with correct type per Section 2.2
        for (i, input) in behavior.contract.inputs.iter().enumerate() {
            if let Some(param) = function.get_nth_param(i as u32) {
                let handle = Handle {
                    ptr: param.into_pointer_value(),
                    size: Self::param_phys_size(input),
                    typ: HandleType::from(&input.typ),
                };
                self.variables.insert(input.name.clone(), SigilValue::Handle(handle));
            }
        }

        // Map output parameters to handles with correct type per Section 2.2
        let input_count = behavior.contract.inputs.len();
        for (i, output) in behavior.contract.outputs.iter().enumerate() {
            if let Some(param) = function.get_nth_param((input_count + i) as u32) {
                let handle = Handle {
                    ptr: param.into_pointer_value(),
                    size: Self::param_phys_size(output),
                    typ: HandleType::from(&output.typ),
                };
                self.variables.insert(output.name.clone(), SigilValue::Handle(handle));
            }
        }

        // Compile body - select implementation per Section 12.3. First scan for
        // reassigned scalar locals so loop counters get memory-backed.
        self.mutable_slots.clear();
        let impl_nodes = self.select_implementation(behavior).map(|i| i.nodes.clone());
        if let Some(nodes) = impl_nodes {
            self.reassigned_locals = Self::collect_reassigned(&nodes);
            self.compile_impl(&nodes, function)?;
        } else if let Some(nodes) = behavior.composition.as_ref().map(|c| c.nodes.clone()) {
            self.reassigned_locals = Self::collect_reassigned(&nodes);
            self.compile_comp(&nodes, function)?;
        }
        self.reassigned_locals.clear();

        // Return void. Free the behavior's root memory scope first, but only when
        // the block still needs a terminator (the normal fall-through exit).
        if self.needs_terminator() {
            self.destroy_root_scope(root_scope)?;
            self.builder.build_return(None)
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        self.current_scope = saved_scope;

        // Restore state
        self.variables = saved_vars;
        self.call_outputs = saved_outputs;
        self.current_function = saved_function;

        // Register signature
        let sig = self.build_sig_from_contract(full_name, &behavior.contract);
        self.behavior_sigs.insert(full_name.to_string(), sig);
        self.behavior_fns.insert(full_name.to_string(), function);

        Ok(function)
    }

    /// Compile a pattern as spawnable function (Section 8)
    ///
    /// Pattern function signature: void sigil_<pattern>(ptr memory)
    /// - memory: pointer to buffer containing MEMORY section handles
    ///
    /// The runtime allocates the memory buffer and passes it to the pattern.
    /// Pattern carves handles at known offsets from MEMORY declarations.
    fn compile_pattern(&mut self, pattern: &Pattern) -> Result<FunctionValue<'ctx>> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let void_type = self.context.void_type();

        self.declare_externals();

        // Pattern takes a single pointer parameter: the memory buffer
        let fn_type = void_type.fn_type(&[ptr_type.into()], false);
        let fn_name = format!("sigil_{}", pattern.name.replace('-', "_"));
        let function = self.module.add_function(&fn_name, fn_type, None);
        let entry = self.context.append_basic_block(function, "entry");

        // Save state
        let saved_vars = std::mem::take(&mut self.variables);
        let saved_outputs = std::mem::take(&mut self.call_outputs);
        let saved_function = self.current_function;

        self.builder.position_at_end(entry);
        self.current_function = Some(function);

        // Get memory buffer parameter
        let memory_ptr = function.get_nth_param(0)
            .ok_or_else(|| CodeGenError::LlvmError("Missing memory parameter".into()))?
            .into_pointer_value();

        // Carve handles from memory buffer at known offsets
        let i8_type = self.context.i8_type();
        let mut offset = 0usize;
        for mem_decl in &pattern.memory {
            let handle_ptr = if offset == 0 {
                memory_ptr
            } else {
                let offset_val = self.context.i64_type().const_int(offset as u64, false);
                unsafe {
                    self.builder.build_gep(i8_type, memory_ptr, &[offset_val], &mem_decl.name)
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                }
            };

            let handle = Handle {
                ptr: handle_ptr,
                size: mem_decl.size,
                typ: HandleType::from(&mem_decl.typ),
            };
            self.variables.insert(mem_decl.name.clone(), SigilValue::Handle(handle));
            offset += mem_decl.size;
        }

        // Compile ON_CREATE body
        self.compile_impl(&pattern.on_create, function)?;

        // Return void
        if self.needs_terminator() {
            self.builder.build_return(None)
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }

        // Restore state
        self.variables = saved_vars;
        self.call_outputs = saved_outputs;
        self.current_function = saved_function;

        // Register as spawnable pattern
        self.behavior_fns.insert(pattern.name.clone(), function);

        // Calculate total memory size for runtime
        let total_memory: usize = pattern.memory.iter().map(|m| m.size).sum();
        let sig = BehaviorSig {
            name: pattern.name.clone(),
            input_sizes: vec![total_memory], // Memory buffer size
            outputs: vec![], // Patterns don't have outputs in the traditional sense
        };
        self.behavior_sigs.insert(pattern.name.clone(), sig);

        Ok(function)
    }

    /// Public API: Compile pattern as spawnable function
    pub fn compile_pattern_as_function(&mut self, pattern: &Pattern) -> std::result::Result<FunctionValue<'ctx>, String> {
        self.compile_pattern(pattern).map_err(|e| e.to_string())
    }

    // =========================================================================
    // Implementation Compilation
    // =========================================================================

    fn compile_impl(&mut self, nodes: &[Node], function: FunctionValue<'ctx>) -> Result<()> {
        // First pass: create label blocks
        let labels = self.create_label_blocks(nodes, function);

        // Second pass: compile nodes
        for node in nodes {
            self.compile_node(node, &labels)?;
        }
        Ok(())
    }

    fn compile_comp(&mut self, nodes: &[Node], function: FunctionValue<'ctx>) -> Result<()> {
        // Same as impl for now
        self.compile_impl(nodes, function)
    }

    fn create_label_blocks(&self, nodes: &[Node], function: FunctionValue<'ctx>) -> HashMap<String, BasicBlock<'ctx>> {
        let mut labels = HashMap::new();
        self.collect_label_blocks(nodes, function, &mut labels);
        labels
    }

    /// Register a basic block for every LABEL, recursing into SCOPE blocks so a
    /// label declared inside a scope is resolvable. Semantic label collection
    /// already recurses into scopes; codegen must match it, otherwise a JUMP to a
    /// label inside a SCOPE fails with "Undefined label".
    fn collect_label_blocks(&self, nodes: &[Node], function: FunctionValue<'ctx>, labels: &mut HashMap<String, BasicBlock<'ctx>>) {
        for node in nodes {
            match &node.kind {
                NodeKind::Label(name) => {
                    let block = self.context.append_basic_block(function, name);
                    labels.insert(name.clone(), block);
                }
                NodeKind::Scope { nodes: inner } => {
                    self.collect_label_blocks(inner, function, labels);
                }
                _ => {}
            }
        }
    }

    fn compile_node(&mut self, node: &Node, labels: &HashMap<String, BasicBlock<'ctx>>) -> Result<()> {
        match &node.kind {
            NodeKind::Label(name) => {
                if let Some(block) = labels.get(name) {
                    if self.needs_terminator() {
                        self.builder.build_unconditional_branch(*block)
                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    }
                    self.builder.position_at_end(*block);
                }
            }

            NodeKind::Assignment { target, expr } => {
                let value = self.compile_expr(expr)?;
                // A reassigned scalar local is memory-backed so its update
                // carries across loop back-edges (mem2reg re-promotes it).
                let memory_backed = self.reassigned_locals.contains(target)
                    && matches!(value, SigilValue::Immediate(_) | SigilValue::Float(_));
                if memory_backed {
                    self.store_mutable_local(target, value)?;
                } else {
                    self.variables.insert(target.clone(), value);
                }

                // Track call outputs for field access
                if let Expr::Call { .. } = expr.as_ref() {
                    if let Some(outputs) = self.call_outputs.remove("__pending") {
                        self.call_outputs.insert(target.clone(), outputs);
                    }
                }
            }

            NodeKind::Store { target, value, size, offset } => {
                let target_val = self.get_var(target)?;
                // Support both handles and pointer values (from previous LOAD)
                let handle = match target_val {
                    SigilValue::Handle(h) => h,
                    SigilValue::Immediate(ptr_val) => {
                        // Pointer chasing: allocated_size is unknown (size: 0)
                        // Bounds check will be skipped per "if handle.size > 0" logic
                        let ptr = self.builder.build_int_to_ptr(
                            ptr_val,
                            self.context.ptr_type(AddressSpace::default()),
                            "ptr_from_int"
                        ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                        Handle { ptr, size: 0, typ: HandleType::Int }
                    }
                    _ => return Err(CodeGenError::TypeError("STORE target must be handle or pointer value".into())),
                };
                let val = self.compile_expr(value)?;
                let off = self.compile_offset(offset.as_ref().map(|b| b.as_ref()))?;
                self.store(handle, val, *size, off)?;
            }

            NodeKind::Jump(label) => {
                let block = labels.get(label)
                    .ok_or_else(|| CodeGenError::UndefinedLabel(label.clone()))?;
                self.builder.build_unconditional_branch(*block)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }

            NodeKind::Branch { condition, true_label, false_label } => {
                // Per Section 4.8: BRANCH expr - if expr is nonzero, jump to label1
                // Per Section 7.2: field access like result.is_error is valid in BRANCH
                let cond_val = self.compile_expr(condition)?;
                let cond = match cond_val {
                    SigilValue::Immediate(i) => i,
                    SigilValue::Handle(h) => {
                        // Field access returns handle - load the int value
                        self.load(h, h.size.min(8), Offset::Const(0))?.as_int()?
                    },
                    SigilValue::Float(_) => return Err(CodeGenError::TypeError("BRANCH condition must be int".into())),
                };
                // Zero must match the condition's int width: a BRANCH on a
                // sub-i64 value (e.g. `LOAD flag 1` -> i8) would otherwise build an
                // `icmp` with mismatched operand types (i8 vs i64) — malformed IR
                // that crashes the assertion-free release LLVM (0xC0000005).
                let zero = cond.get_type().const_zero();
                let cond_bool = self.builder.build_int_compare(IntPredicate::NE, cond, zero, "cond")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                let true_block = labels.get(true_label)
                    .ok_or_else(|| CodeGenError::UndefinedLabel(true_label.clone()))?;
                let false_block = labels.get(false_label)
                    .ok_or_else(|| CodeGenError::UndefinedLabel(false_label.clone()))?;

                self.builder.build_conditional_branch(cond_bool, *true_block, *false_block)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }

            NodeKind::Set { target, value } => {
                let val = self.compile_expr(value)?;

                // If target is a handle, store to it; otherwise assign directly
                // Copy handle info to avoid borrow conflict
                let target_handle = self.variables.get(target).and_then(|v| {
                    if let SigilValue::Handle(h) = v { Some(*h) } else { None }
                });

                if let Some(h) = target_handle {
                    if h.typ == HandleType::String {
                        // A `string` output is an 8-byte slot holding a pointer to the
                        // length-prefixed string data. Store the source string's pointer
                        // directly (do NOT load/copy) — `store` writes a Handle as its
                        // address. `val` is the string handle (a literal global, or a
                        // string field already dereferenced to point at its data).
                        self.store(h, val, 8, Offset::Const(0))?;
                    } else {
                        // Load value if it's a handle (e.g., from field access)
                        let to_store = match val {
                            SigilValue::Handle(src) => self.load(src, src.size.min(8), Offset::Const(0))?,
                            other => other,
                        };
                        self.store(h, to_store, h.size, Offset::Const(0))?;
                    }
                } else {
                    self.variables.insert(target.clone(), val);
                }
            }

            NodeKind::Call { target, behavior, args, outputs } => {
                let call_expr = Expr::Call {
                    behavior: behavior.clone(),
                    args: args.clone(),
                };
                let result = self.compile_expr(&call_expr)?;

                if let Some(target_name) = target {
                    self.variables.insert(target_name.clone(), result.clone());
                    if let Some(outs) = self.call_outputs.remove("__pending") {
                        self.call_outputs.insert(target_name.clone(), outs);
                    }
                }

                if !outputs.is_empty() {
                    if let Some(output_slots) = self.call_outputs.remove("__pending") {
                        if let SigilValue::Handle(src) = &result {
                            let src = *src;
                            for (out_name, slot) in outputs.iter().zip(output_slots.slots.iter()) {
                                match self.variables.get(out_name).cloned() {
                                    // `-> out` into a known buffer handle (a declared
                                    // OUTPUT, §7.1): copy the call's output slot into
                                    // that buffer byte-for-byte. Loading + rebinding
                                    // the name (the old path) left a `bytes` OUTPUT
                                    // pointing at junk and never wrote the caller's
                                    // buffer.
                                    Some(SigilValue::Handle(dest)) => {
                                        let src_ptr = if slot.offset > 0 {
                                            let i8_type = self.context.i8_type();
                                            let off = self.context.i64_type()
                                                .const_int(slot.offset as u64, false);
                                            unsafe {
                                                self.builder
                                                    .build_gep(i8_type, src.ptr, &[off], "redirect_src")
                                                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                                            }
                                        } else {
                                            src.ptr
                                        };
                                        // Never write past the output buffer's
                                        // declared capacity.
                                        let n = if dest.size == 0 {
                                            slot.size
                                        } else {
                                            slot.size.min(dest.size)
                                        };
                                        let size_val =
                                            self.context.i64_type().const_int(n as u64, false);
                                        self.builder
                                            .build_memcpy(dest.ptr, 1, src_ptr, 1, size_val)
                                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                                    }
                                    // `-> out` into a local name: bind the produced
                                    // value (§7.8 produced output).
                                    _ => {
                                        let extracted =
                                            self.load(src, slot.size, Offset::Const(slot.offset))?;
                                        self.variables.insert(out_name.clone(), extracted);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            NodeKind::Free(name) => {
                // Free this handle's allocation now (memory model). Only
                // scope-allocated ALLOC handles are registered; sigil_scope_free
                // is a no-op for anything not found, so this is safe regardless.
                if let Some(scope) = self.current_scope {
                    if let Some(SigilValue::Handle(h)) = self.variables.get(name).cloned() {
                        let free_fn = self.get_or_declare_sigil_scope_free();
                        self.builder
                            .build_call(free_fn, &[scope.into(), h.ptr.into()], "")
                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    }
                }
            }

            NodeKind::Discard(_) => {
                // DISCARD (Section 7.8) is a compile-time consumption marker; the
                // value it names was already produced by an earlier statement, so
                // there is nothing to emit.
            }

            NodeKind::Scope { nodes } => {
                let saved = self.variables.clone();
                // Enter a child scope: ALLOCs inside are freed at END_SCOPE, so a
                // SCOPE in a loop stays bounded rather than deferring to return.
                let saved_scope = self.current_scope;
                let child = if let Some(parent) = self.current_scope {
                    let create_fn = self.get_or_declare_sigil_scope_create();
                    let c = self.builder
                        .build_call(create_fn, &[parent.into()], "scope")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                        .try_as_basic_value()
                        .left()
                        .ok_or_else(|| CodeGenError::LlvmError("sigil_scope_create returned void".into()))?
                        .into_pointer_value();
                    self.current_scope = Some(c);
                    Some(c)
                } else {
                    None
                };
                for n in nodes {
                    self.compile_node(n, labels)?;
                }
                // END_SCOPE: free this scope's allocations.
                if let Some(c) = child {
                    let destroy_fn = self.get_or_declare_sigil_scope_destroy();
                    self.builder
                        .build_call(destroy_fn, &[c.into()], "")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                }
                self.current_scope = saved_scope;
                self.variables = saved;
            }

            // =========================================================================
            // Concurrency NodeKinds
            // =========================================================================

            NodeKind::ChannelSend { channel, value } => {
                let send_fn = self.get_or_declare_sigil_channel_send();

                // Get channel handle
                let channel_val = self.variables.get(channel)
                    .ok_or_else(|| CodeGenError::UndefinedVariable(channel.clone()))?
                    .clone();
                let channel_int = channel_val.as_int()?;

                // Compile value and get pointer to it
                let val = self.compile_expr(value)?;
                let val_buf = self.alloc_handle(8, HandleType::Int)?;
                self.store(val_buf, val, 8, Offset::Const(0))?;

                self.builder.build_call(
                    send_fn,
                    &[channel_int.into(), val_buf.ptr.into()],
                    ""
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }

            NodeKind::ChannelClose(channel) => {
                let close_fn = self.get_or_declare_sigil_channel_close();

                // Get channel handle
                let channel_val = self.variables.get(channel)
                    .ok_or_else(|| CodeGenError::UndefinedVariable(channel.clone()))?
                    .clone();
                let channel_int = channel_val.as_int()?;

                self.builder.build_call(
                    close_fn,
                    &[channel_int.into()],
                    ""
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }

            NodeKind::Spawn { target, pattern, args } => {
                // Compile SPAWN as statement (same as expression, but assign to target)
                let spawn_expr = Expr::Spawn {
                    pattern: pattern.clone(),
                    args: args.clone(),
                };
                let result = self.compile_expr(&spawn_expr)?;
                self.variables.insert(target.clone(), result);
            }

            NodeKind::Wait { target, handle } => {
                // Compile WAIT as statement
                let wait_expr = Expr::Wait(handle.clone());
                let result = self.compile_expr(&wait_expr)?;
                self.variables.insert(target.clone(), result);
            }

            NodeKind::WaitAll { targets, handles } => {
                // WAIT_ALL: wait for all handles, collect results
                let wait_fn = self.get_or_declare_sigil_wait();
                let null_ptr = self.context.ptr_type(AddressSpace::default()).const_null();
                let zero = self.context.i64_type().const_zero();

                for (i, handle_name) in handles.iter().enumerate() {
                    let handle_val = self.variables.get(handle_name)
                        .ok_or_else(|| CodeGenError::UndefinedVariable(handle_name.clone()))?
                        .clone();
                    let handle_int = handle_val.as_int()?;

                    self.builder.build_call(
                        wait_fn,
                        &[handle_int.into(), null_ptr.into(), zero.into()],
                        ""
                    ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                    // If there's a corresponding target, assign result
                    if let Some(target) = targets.get(i) {
                        self.variables.insert(target.clone(), SigilValue::Immediate(zero));
                    }
                }
            }

            NodeKind::WaitAny { result, which, handles } => {
                // WAIT_ANY: block until ANY one of the handles completes, and report
                // which one. Build a stack array of handle ids and hand it to the
                // runtime sigil_wait_any, which blocks on a table-wide completion
                // condvar and returns the index of the task that finished.
                let i64_ty = self.context.i64_type();
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let zero = i64_ty.const_zero();
                let count = handles.len() as u64;

                let arr_ty = i64_ty.array_type(count as u32);
                let handles_arr = self.builder.build_alloca(arr_ty, "wait_any_handles")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                for (i, handle_name) in handles.iter().enumerate() {
                    let handle_val = self.variables.get(handle_name)
                        .ok_or_else(|| CodeGenError::UndefinedVariable(handle_name.clone()))?
                        .clone();
                    let handle_int = handle_val.as_int()?;
                    let elem_ptr = unsafe {
                        self.builder.build_in_bounds_gep(
                            arr_ty, handles_arr,
                            &[zero, i64_ty.const_int(i as u64, false)],
                            "wa_elem")
                    }.map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    self.builder.build_store(elem_ptr, handle_int)
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                }

                let wait_any_fn = self.get_or_declare_sigil_wait_any();
                let null_ptr = ptr_ty.const_null();
                let count_val = i64_ty.const_int(count, false);
                let ret = self.builder.build_call(
                    wait_any_fn,
                    &[handles_arr.into(), count_val.into(), null_ptr.into(), zero.into()],
                    "wait_any_which"
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                let which_val = ret.try_as_basic_value().left()
                    .map(|v| v.into_int_value())
                    .unwrap_or(zero);

                // Spawned tasks carry no result value (data flows via channels), so
                // `result` is 0; `which` is the real returned index.
                self.variables.insert(result.clone(), SigilValue::Immediate(zero));
                self.variables.insert(which.clone(), SigilValue::Immediate(which_val));
            }
        }
        Ok(())
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    pub(crate) fn get_var(&self, name: &str) -> Result<SigilValue<'ctx>> {
        // A memory-backed reassigned local: load its current value.
        if let Some(&(ptr, ty)) = self.mutable_slots.get(name) {
            let loaded = self.builder.build_load(ty, ptr, name)
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            return Ok(if matches!(ty, inkwell::types::BasicTypeEnum::FloatType(_)) {
                SigilValue::Float(loaded.into_float_value())
            } else {
                SigilValue::Immediate(loaded.into_int_value())
            });
        }
        self.variables.get(name)
            .copied()
            .ok_or_else(|| CodeGenError::UndefinedVariable(name.into()))
    }

    /// Create an alloca at the top of the current function's entry block, so the
    /// slot is created once (outside any loop) and is promotable by mem2reg.
    fn create_entry_alloca(
        &self,
        name: &str,
        ty: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> Result<PointerValue<'ctx>> {
        let func = self.current_function
            .ok_or_else(|| CodeGenError::LlvmError("no current function for local slot".into()))?;
        let entry = func.get_first_basic_block()
            .ok_or_else(|| CodeGenError::LlvmError("function has no entry block".into()))?;
        let tmp = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(first) => tmp.position_before(&first),
            None => tmp.position_at_end(entry),
        }
        tmp.build_alloca(ty, name)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))
    }

    /// Store a value into a reassigned scalar local's memory slot, creating the
    /// entry-block slot on first assignment.
    fn store_mutable_local(&mut self, name: &str, value: SigilValue<'ctx>) -> Result<()> {
        let (ptr, ty) = match self.mutable_slots.get(name) {
            Some(&(p, t)) => (p, t),
            None => {
                // A reassigned int local always gets a canonical i64 slot, NOT the
                // width of its first assignment. Otherwise a local first assigned a
                // narrow value (e.g. `b = LOAD buf 1 i` -> i8) would get an i8 slot,
                // and a later wider assignment (`b = AND b 255` -> i64) would be
                // TRUNCATED back into it; reloading then sign-extends, corrupting any
                // byte >= 0x80. Storing widens via resize_int (zero-extend), matching
                // Sigil's "values are 64-bit, size is just access width" model.
                let ty: inkwell::types::BasicTypeEnum<'ctx> = match value {
                    SigilValue::Immediate(_) => self.context.i64_type().into(),
                    SigilValue::Float(f) => f.get_type().into(),
                    _ => return Err(CodeGenError::LlvmError("non-scalar mutable local".into())),
                };
                let p = self.create_entry_alloca(name, ty)?;
                self.mutable_slots.insert(name.to_string(), (p, ty));
                (p, ty)
            }
        };
        match value {
            SigilValue::Immediate(i) => {
                let sized = self.resize_int(i, ty.into_int_type())?;
                self.builder.build_store(ptr, sized)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }
            SigilValue::Float(f) => {
                self.builder.build_store(ptr, f)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }
            _ => return Err(CodeGenError::LlvmError("non-scalar mutable local".into())),
        }
        Ok(())
    }

    /// Names assigned more than once in `nodes` (recursing into SCOPE), which
    /// therefore need memory backing to survive loop back-edges.
    fn collect_reassigned(nodes: &[Node]) -> std::collections::HashSet<String> {
        fn walk(nodes: &[Node], counts: &mut HashMap<String, u32>) {
            for n in nodes {
                match &n.kind {
                    NodeKind::Assignment { target, .. } => {
                        *counts.entry(target.clone()).or_insert(0) += 1;
                    }
                    NodeKind::Scope { nodes } => walk(nodes, counts),
                    _ => {}
                }
            }
        }
        let mut counts: HashMap<String, u32> = HashMap::new();
        walk(nodes, &mut counts);
        counts.into_iter().filter(|(_, c)| *c >= 2).map(|(k, _)| k).collect()
    }

    pub(crate) fn int_type(&self, bits: u32) -> inkwell::types::IntType<'ctx> {
        match bits {
            1 => self.context.bool_type(),
            8 => self.context.i8_type(),
            16 => self.context.i16_type(),
            32 => self.context.i32_type(),
            64 => self.context.i64_type(),
            _ => self.context.custom_width_int_type(bits),
        }
    }

    pub(crate) fn resize_int(&self, value: IntValue<'ctx>, target: inkwell::types::IntType<'ctx>) -> Result<IntValue<'ctx>> {
        let src_bits = value.get_type().get_bit_width();
        let dst_bits = target.get_bit_width();

        if src_bits == dst_bits {
            Ok(value)
        } else if src_bits > dst_bits {
            self.builder.build_int_truncate(value, target, "trunc")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))
        } else {
            self.builder.build_int_z_extend(value, target, "zext")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))
        }
    }

    pub(crate) fn eval_const_int(&self, expr: &Expr) -> Result<usize> {
        match expr {
            Expr::IntLit(n) => Ok(*n as usize),
            _ => Err(CodeGenError::TypeError("Expected constant integer".into())),
        }
    }

    /// Compile offset expression - returns Offset::Const for literals, Offset::Dynamic for expressions
    pub(crate) fn compile_offset(&mut self, expr: Option<&Expr>) -> Result<Offset<'ctx>> {
        match expr {
            None => Ok(Offset::Const(0)),
            Some(Expr::IntLit(n)) => Ok(Offset::Const(*n as usize)),
            Some(e) => {
                let val = self.compile_expr(e)?.as_int()?;
                Ok(Offset::Dynamic(val))
            }
        }
    }

    pub(crate) fn needs_terminator(&self) -> bool {
        self.builder.get_insert_block()
            .map(|b| b.get_terminator().is_none())
            .unwrap_or(false)
    }

    fn get_return_status(&mut self, behavior: &Behavior) -> Result<IntValue<'ctx>> {
        let i32_type = self.context.i32_type();

        if let Some(output) = behavior.contract.outputs.first() {
            if output.typ == AstType::Int && output.size <= 8 {
                if let Some(SigilValue::Handle(h)) = self.variables.get(&output.name) {
                    let loaded = self.load(*h, output.size.min(4), Offset::Const(0))?;
                    let int_val = loaded.as_int()?;
                    return self.resize_int(int_val, i32_type);
                }
            }
        }
        Ok(i32_type.const_zero())
    }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    /// Helper: compile behavior to IR string
    fn compile_to_ir(source: &str) -> std::result::Result<String, String> {
        let tokens = lex(source).map_err(|e| e.message)?;
        let behavior = parse(&tokens, source).map_err(|e| e.to_string())?;

        let context = Context::create();
        let mut codegen = CodeGen::new_default(&context, "test");
        codegen.compile_as_main(&behavior).map_err(|e| e.to_string())?;
        Ok(codegen.dump_ir())
    }

    /// Helper: compile and expect success
    fn must_compile(source: &str) -> String {
        compile_to_ir(source).expect("Compilation failed")
    }

    // =========================================================================
    // Basic Compilation
    // =========================================================================

    #[test]
    fn test_minimal_behavior_compiles() {
        let source = r#"
BEHAVIOR minimal

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH mini1234

IMPLEMENTATION
  SET status 0
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("define i32 @main()"));
    }

    #[test]
    fn test_integer_arithmetic_ir() {
        let source = r#"
BEHAVIOR arith

CONTRACT
  INPUT a int 8
  INPUT b int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH arit1234

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  sum = IADD va vb 8
  STORE result sum 8
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("add i64"));
    }

    #[test]
    fn test_load_store_ir() {
        let source = r#"
BEHAVIOR loadstore

CONTRACT
  INPUT x int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH load1234

IMPLEMENTATION
  v = LOAD x 8
  STORE result v 8
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("load"));
        assert!(ir.contains("store"));
    }

    #[test]
    fn test_branch_jump_ir() {
        let source = r#"
BEHAVIOR control

CONTRACT
  INPUT cond int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH ctrl1234

IMPLEMENTATION
  c = LOAD cond 8
  BRANCH c true_br false_br

  LABEL true_br
    SET result 1
    JUMP done

  LABEL false_br
    SET result 0
    JUMP done

  LABEL done
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("br i1"));
        assert!(ir.contains("br label"));
    }

    #[test]
    fn test_alloc_generates_alloca() {
        let source = r#"
BEHAVIOR alloc-test

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH allo1234

IMPLEMENTATION
  h = ALLOC 64 bytes
  FREE h
  SET status 0
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("alloca"));
    }

    #[test]
    fn test_comparison_generates_icmp() {
        let source = r#"
BEHAVIOR compare

CONTRACT
  INPUT a int 8
  INPUT b int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH comp1234

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  cmp = ILT va vb 8
  STORE result cmp 8
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("icmp slt"));
    }

    #[test]
    fn test_bitwise_and_ir() {
        let source = r#"
BEHAVIOR bitwise

CONTRACT
  INPUT a int 8
  INPUT b int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH bitw1234

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  r = AND va vb 8
  STORE result r 8
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("and i64"));
    }

    #[test]
    fn test_label_inside_scope_resolves() {
        // Regression: a LABEL inside a SCOPE must get a basic block. Codegen
        // previously skipped scope contents in label collection and failed with
        // "Undefined label" on a JUMP to such a label.
        let source = r#"
BEHAVIOR scope-label

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  SCOPE
    tmp = ALLOC 8 int
    JUMP skip
    LABEL skip
    FREE tmp
  END_SCOPE
  SET status 0
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("define i32 @main()"));
    }

    #[test]
    fn test_string_literal_creates_global() {
        let source = r#"
BEHAVIOR string-test

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH strg1234

IMPLEMENTATION
  msg = "Hello"
  SET status 0
END
"#;
        let ir = must_compile(source);
        assert!(ir.contains("Hello"));
    }
}
