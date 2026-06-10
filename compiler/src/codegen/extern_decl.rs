//! External declarations for runtime functions
//!
//! Contains:
//! - CRT function declarations
//! - Runtime function declarations (spawn, wait, channel)

use inkwell::module::Linkage;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

use super::CodeGen;

impl<'ctx> CodeGen<'ctx> {
    // =========================================================================
    // External Declarations
    // =========================================================================

    pub(crate) fn declare_externals(&self) {
        self.declare_puts();
        self.declare_write();
    }

    fn declare_puts(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("puts") { return f; }
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let fn_type = i32_type.fn_type(&[ptr_type.into()], false);
        self.module.add_function("puts", fn_type, Some(Linkage::External))
    }

    fn declare_write(&self) -> FunctionValue<'ctx> {
        let name = self.crt_name("write");
        if let Some(f) = self.module.get_function(&name) { return f; }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let fn_type = i64_type.fn_type(&[i64_type.into(), ptr_type.into(), i64_type.into()], false);
        self.module.add_function(&name, fn_type, Some(Linkage::External))
    }

    // =========================================================================
    // Runtime Functions (Concurrency)
    // =========================================================================

    pub(crate) fn get_or_declare_sigil_spawn(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_spawn") { return f; }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        // uint64_t sigil_spawn(void (*func)(void*), void* args, uint64_t args_size)
        let fn_type = i64_type.fn_type(&[ptr_type.into(), ptr_type.into(), i64_type.into()], false);
        self.module.add_function("sigil_spawn", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_wait(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_wait") { return f; }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let void_type = self.context.void_type();
        // void sigil_wait(uint64_t handle, void* result, uint64_t result_size)
        let fn_type = void_type.fn_type(&[i64_type.into(), ptr_type.into(), i64_type.into()], false);
        self.module.add_function("sigil_wait", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_wait_any(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_wait_any") { return f; }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        // uint64_t sigil_wait_any(uint64_t* handles, uint64_t count, void* result, uint64_t result_size)
        // Returns the index (into handles) of the task that completed.
        let fn_type = i64_type.fn_type(
            &[ptr_type.into(), i64_type.into(), ptr_type.into(), i64_type.into()], false);
        self.module.add_function("sigil_wait_any", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_channel_create(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_channel_create") { return f; }
        let i64_type = self.context.i64_type();
        // uint64_t sigil_channel_create(uint64_t elem_size, uint64_t capacity)
        let fn_type = i64_type.fn_type(&[i64_type.into(), i64_type.into()], false);
        self.module.add_function("sigil_channel_create", fn_type, Some(Linkage::External))
    }

    // Scope allocator (memory model: ALLOC / FREE / SCOPE / END_SCOPE).
    pub(crate) fn get_or_declare_sigil_scope_create(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_scope_create") { return f; }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        // SigilScope* sigil_scope_create(SigilScope* parent)
        let fn_type = ptr_type.fn_type(&[ptr_type.into()], false);
        self.module.add_function("sigil_scope_create", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_scope_alloc(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_scope_alloc") { return f; }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        // void* sigil_scope_alloc(SigilScope* s, uint64_t size)
        let fn_type = ptr_type.fn_type(&[ptr_type.into(), i64_type.into()], false);
        self.module.add_function("sigil_scope_alloc", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_scope_free(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_scope_free") { return f; }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let void_type = self.context.void_type();
        // void sigil_scope_free(SigilScope* s, void* ptr)
        let fn_type = void_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
        self.module.add_function("sigil_scope_free", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_scope_destroy(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_scope_destroy") { return f; }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let void_type = self.context.void_type();
        // void sigil_scope_destroy(SigilScope* s)
        let fn_type = void_type.fn_type(&[ptr_type.into()], false);
        self.module.add_function("sigil_scope_destroy", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_channel_send(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_channel_send") { return f; }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        // int64_t sigil_channel_send(uint64_t channel, void* value)
        let fn_type = i64_type.fn_type(&[i64_type.into(), ptr_type.into()], false);
        self.module.add_function("sigil_channel_send", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_channel_receive(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_channel_receive") { return f; }
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        // int64_t sigil_channel_receive(uint64_t channel, void* value)
        let fn_type = i64_type.fn_type(&[i64_type.into(), ptr_type.into()], false);
        self.module.add_function("sigil_channel_receive", fn_type, Some(Linkage::External))
    }

    pub(crate) fn get_or_declare_sigil_channel_close(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("sigil_channel_close") { return f; }
        let i64_type = self.context.i64_type();
        let void_type = self.context.void_type();
        // void sigil_channel_close(uint64_t channel)
        let fn_type = void_type.fn_type(&[i64_type.into()], false);
        self.module.add_function("sigil_channel_close", fn_type, Some(Linkage::External))
    }

    // Network runtime functions removed - use NATIVE behaviors instead
    // Functions sigil_socket, sigil_bind, sigil_listen, sigil_accept, sigil_recv, sigil_send
    // are now provided by runtime and declared via NATIVE behavior compilation.
}
