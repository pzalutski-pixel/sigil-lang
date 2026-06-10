//! Expression compilation - compiles Sigil expressions to LLVM IR

use inkwell::values::{IntValue, FloatValue, BasicMetadataValueEnum};
use inkwell::IntPredicate;
use inkwell::AddressSpace;

use crate::ast::Expr;
use super::{CodeGen, CodeGenError, Result, SigilValue, Handle, HandleType, Offset, BehaviorSig, CallOutputs};

impl<'ctx> CodeGen<'ctx> {
    // =========================================================================
    // Expression Compilation
    // =========================================================================

    pub(crate) fn compile_expr(&mut self, expr: &Expr) -> Result<SigilValue<'ctx>> {
        match expr {
            Expr::IntLit(n) => {
                let val = self.context.i64_type().const_int(*n as u64, *n < 0);
                Ok(SigilValue::Immediate(val))
            }

            Expr::FloatLit(f) => {
                let val = self.context.f64_type().const_float(*f);
                Ok(SigilValue::Float(val))
            }

            Expr::StringLit(s) => {
                // A string is length-prefixed: [len: 8-byte LE][content]. The handle
                // points at the prefix; the value carries its own size, so no separate
                // length argument is ever needed.
                let i8 = self.context.i8_type();
                let mut bytes: Vec<u8> = (s.len() as u64).to_le_bytes().to_vec();
                bytes.extend_from_slice(s.as_bytes());
                let arr_vals: Vec<_> = bytes.iter().map(|b| i8.const_int(*b as u64, false)).collect();
                let arr = i8.const_array(&arr_vals);
                let global = self.module.add_global(arr.get_type(), None, "str");
                global.set_initializer(&arr);
                global.set_constant(true);
                // Private linkage: each object's string constants are local, so
                // identical names across separately-compiled behaviors don't collide.
                global.set_linkage(inkwell::module::Linkage::Private);
                Ok(SigilValue::Handle(Handle {
                    ptr: global.as_pointer_value(),
                    size: bytes.len(),
                    typ: HandleType::String,
                }))
            }

            Expr::Var(name) => self.get_var(name),

            Expr::Field { base, field } => {
                // Per Sigil Reference Section 7.2: field access on CALL results
                if let Expr::Var(base_name) = base.as_ref() {
                    let outputs = self.call_outputs.get(base_name)
                        .ok_or_else(|| CodeGenError::UndefinedField(base_name.clone(), field.clone()))?
                        .clone();
                    let handle = self.get_field(&outputs, field)?;
                    if handle.typ == HandleType::String {
                        // The slot holds a pointer to the length-prefixed string. Load
                        // it so the string value (a pointer to [len][content]) flows on,
                        // matching how a string literal is passed to a callee.
                        let ptr_type = self.context.ptr_type(AddressSpace::default());
                        let loaded = self.builder.build_load(ptr_type, handle.ptr, "str_field")
                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                        Ok(SigilValue::Handle(Handle {
                            ptr: loaded.into_pointer_value(),
                            size: 0,
                            typ: HandleType::String,
                        }))
                    } else {
                        Ok(SigilValue::Handle(handle))
                    }
                } else {
                    Err(CodeGenError::TypeError("Field access base must be variable".into()))
                }
            }

            Expr::Alloc { size, typ, .. } => {
                let size_val = self.eval_const_int(size).unwrap_or(64);
                // The language-level ALLOC: heap-backed via the scope allocator
                // (freed at FREE / END_SCOPE / behavior return), not a stack alloca.
                let handle = self.scope_alloc_handle(size_val, HandleType::from(typ))?;
                Ok(SigilValue::Handle(handle))
            }

            Expr::Load { source, size, offset } => {
                let source_val = self.compile_expr(source)?;
                // Support both handles and pointer values (from previous LOAD)
                let handle = match source_val {
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
                    _ => return Err(CodeGenError::TypeError("LOAD source must be handle or pointer value".into())),
                };
                let off = self.compile_offset(offset.as_ref().map(|b| b.as_ref()))?;
                self.load(handle, *size, off)
            }

            Expr::Call { behavior, args } => {
                self.compile_call(behavior, args)
            }

            // Integer arithmetic
            Expr::Iadd(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_int_add(av, bv, "add")),
            Expr::Isub(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_int_sub(av, bv, "sub")),
            Expr::Imul(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_int_mul(av, bv, "mul")),
            Expr::Idiv(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_int_signed_div(av, bv, "div")),
            Expr::Imod(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_int_signed_rem(av, bv, "mod")),

            Expr::Ineg(a, _) => {
                let av = self.compile_expr(a)?.as_int()?;
                let zero = self.context.i64_type().const_zero();
                let result = self.builder.build_int_sub(zero, av, "neg")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Immediate(result))
            }

            // Integer comparisons
            Expr::Ieq(a, b, _) => self.compile_int_cmp(a, b, IntPredicate::EQ),
            Expr::Ine(a, b, _) => self.compile_int_cmp(a, b, IntPredicate::NE),
            Expr::Ilt(a, b, _) => self.compile_int_cmp(a, b, IntPredicate::SLT),
            Expr::Igt(a, b, _) => self.compile_int_cmp(a, b, IntPredicate::SGT),
            Expr::Ile(a, b, _) => self.compile_int_cmp(a, b, IntPredicate::SLE),
            Expr::Ige(a, b, _) => self.compile_int_cmp(a, b, IntPredicate::SGE),

            // Bitwise operations
            Expr::And(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_and(av, bv, "and")),
            Expr::Or(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_or(av, bv, "or")),
            Expr::Xor(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_xor(av, bv, "xor")),
            Expr::Shl(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_left_shift(av, bv, "shl")),
            Expr::Shr(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_right_shift(av, bv, false, "shr")),
            Expr::Sar(a, b, _) => self.compile_int_binop(a, b, |bld, av, bv| bld.build_right_shift(av, bv, true, "sar")),

            Expr::Not(a, _) => {
                let av = self.compile_expr(a)?.as_int()?;
                let result = self.builder.build_not(av, "not")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Immediate(result))
            }

            // Float operations
            Expr::Fadd(a, b, _) => self.compile_float_binop(a, b, |bld, av, bv| bld.build_float_add(av, bv, "fadd")),
            Expr::Fsub(a, b, _) => self.compile_float_binop(a, b, |bld, av, bv| bld.build_float_sub(av, bv, "fsub")),
            Expr::Fmul(a, b, _) => self.compile_float_binop(a, b, |bld, av, bv| bld.build_float_mul(av, bv, "fmul")),
            Expr::Fdiv(a, b, _) => self.compile_float_binop(a, b, |bld, av, bv| bld.build_float_div(av, bv, "fdiv")),

            Expr::Fneg(a, _) => {
                let av = self.compile_expr(a)?.as_float()?;
                let result = self.builder.build_float_neg(av, "fneg")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Float(result))
            }

            // Float comparisons
            Expr::Feq(a, b, _) => self.compile_float_cmp(a, b, inkwell::FloatPredicate::OEQ),
            Expr::Fne(a, b, _) => self.compile_float_cmp(a, b, inkwell::FloatPredicate::ONE),
            Expr::Flt(a, b, _) => self.compile_float_cmp(a, b, inkwell::FloatPredicate::OLT),
            Expr::Fgt(a, b, _) => self.compile_float_cmp(a, b, inkwell::FloatPredicate::OGT),
            Expr::Fle(a, b, _) => self.compile_float_cmp(a, b, inkwell::FloatPredicate::OLE),
            Expr::Fge(a, b, _) => self.compile_float_cmp(a, b, inkwell::FloatPredicate::OGE),

            // Conversions
            Expr::Ftoi { value, int_size, .. } => {
                let fv = self.compile_expr(value)?.as_float()?;
                let int_type = self.int_type((*int_size * 8) as u32);
                let result = self.builder.build_float_to_signed_int(fv, int_type, "ftoi")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Immediate(result))
            }

            Expr::Itof { value, float_size, .. } => {
                let iv = self.compile_expr(value)?.as_int()?;
                let float_type = if *float_size == 4 { self.context.f32_type() } else { self.context.f64_type() };
                let result = self.builder.build_signed_int_to_float(iv, float_type, "itof")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Float(result))
            }

            // Atomics
            Expr::AtomicLoad(source, size) => {
                let handle = self.compile_expr(source)?.as_handle()?;
                self.load(handle, *size, Offset::Const(0))
            }

            Expr::AtomicStore(target, value, size) => {
                let handle = self.compile_expr(target)?.as_handle()?;
                let val = self.compile_expr(value)?;
                self.store(handle, val, *size, Offset::Const(0))?;
                Ok(val)
            }

            Expr::Cas { addr, expected, new, .. } => {
                let ptr = self.compile_expr(addr)?.as_handle()?.ptr;
                let exp = self.compile_expr(expected)?.as_int()?;
                let new_val = self.compile_expr(new)?.as_int()?;

                let result = self.builder.build_cmpxchg(
                    ptr, exp, new_val,
                    inkwell::AtomicOrdering::SequentiallyConsistent,
                    inkwell::AtomicOrdering::SequentiallyConsistent,
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                let success = self.builder.build_extract_value(result, 1, "cas_success")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                let ext = self.builder.build_int_z_extend(success.into_int_value(), self.context.i64_type(), "cas_ext")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Immediate(ext))
            }

            Expr::AtomicAdd(target, value, _) => {
                let ptr = self.compile_expr(target)?.as_handle()?.ptr;
                let val = self.compile_expr(value)?.as_int()?;
                let result = self.builder.build_atomicrmw(
                    inkwell::AtomicRMWBinOp::Add, ptr, val,
                    inkwell::AtomicOrdering::SequentiallyConsistent,
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Immediate(result))
            }

            Expr::AtomicSub(target, value, _) => {
                let ptr = self.compile_expr(target)?.as_handle()?.ptr;
                let val = self.compile_expr(value)?.as_int()?;
                let result = self.builder.build_atomicrmw(
                    inkwell::AtomicRMWBinOp::Sub, ptr, val,
                    inkwell::AtomicOrdering::SequentiallyConsistent,
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(SigilValue::Immediate(result))
            }

            // =========================================================================
            // Concurrency Primitives
            // =========================================================================

            Expr::Spawn { pattern, args } => {
                let spawn_fn = self.get_or_declare_sigil_spawn();

                // Get function pointer for the pattern
                let fn_name = format!("sigil_{}", pattern.replace('-', "_").replace('/', "_"));
                let pattern_fn = self.module.get_function(&fn_name)
                    .ok_or_else(|| CodeGenError::UndefinedBehavior(pattern.clone()))?;

                // Look up pattern signature to get memory size
                // For patterns: input_sizes[0] contains the MEMORY buffer size
                // For behaviors (fallback): use args-based size
                let memory_size = self.behavior_sigs.get(pattern)
                    .and_then(|sig| sig.input_sizes.first().copied())
                    .unwrap_or(0);

                // Buffer size: pattern MEMORY size or args size, whichever is larger
                let args_size = args.len() * 8;
                let buffer_size = if memory_size > 0 { memory_size } else { args_size };
                let args_buf = self.alloc_handle(buffer_size, HandleType::Bytes)?;

                // Store each arg into buffer (for patterns with args after memory)
                for (i, arg) in args.iter().enumerate() {
                    let val = self.compile_expr(arg)?;
                    let offset = i * 8;
                    match val {
                        SigilValue::Handle(h) => {
                            // Store pointer to arg handle
                            let ptr_as_int = self.builder.build_ptr_to_int(
                                h.ptr, self.context.i64_type(), "ptr_as_int"
                            ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                            self.store(args_buf, SigilValue::Immediate(ptr_as_int), 8, Offset::Const(offset))?;
                        }
                        SigilValue::Immediate(i) => {
                            self.store(args_buf, SigilValue::Immediate(i), 8, Offset::Const(offset))?;
                        }
                        SigilValue::Float(f) => {
                            // Cast float to int for storage
                            let as_int = self.builder.build_bit_cast(f, self.context.i64_type(), "f_as_i")
                                .map_err(|e: inkwell::builder::BuilderError| CodeGenError::LlvmError(e.to_string()))?
                                .into_int_value();
                            self.store(args_buf, SigilValue::Immediate(as_int), 8, Offset::Const(offset))?;
                        }
                    }
                }

                // Call sigil_spawn(func_ptr, args_buf, buffer_size)
                let func_ptr = pattern_fn.as_global_value().as_pointer_value();
                let size_val = self.context.i64_type().const_int(buffer_size as u64, false);

                let result = self.builder.build_call(
                    spawn_fn,
                    &[func_ptr.into(), args_buf.ptr.into(), size_val.into()],
                    "spawn"
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                let handle = result.try_as_basic_value().left()
                    .map(|v| v.into_int_value())
                    .unwrap_or_else(|| self.context.i64_type().const_zero());

                Ok(SigilValue::Immediate(handle))
            }

            Expr::Wait(handle_name) => {
                let wait_fn = self.get_or_declare_sigil_wait();

                // Get the handle value
                let handle_val = self.variables.get(handle_name)
                    .ok_or_else(|| CodeGenError::UndefinedVariable(handle_name.clone()))?
                    .clone();
                let handle_int = handle_val.as_int()?;

                // WAIT is a synchronization barrier: sigil_wait blocks until the
                // task completes. A spawned pattern has no OUTPUT contract, so there
                // is no result data to copy back (results flow via channels) — pass a
                // null result buffer.
                let null_ptr = self.context.ptr_type(AddressSpace::default()).const_null();
                let zero = self.context.i64_type().const_zero();

                self.builder.build_call(
                    wait_fn,
                    &[handle_int.into(), null_ptr.into(), zero.into()],
                    ""
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                // Yield a completion status (0 = completed). Data is passed via channels.
                Ok(SigilValue::Immediate(self.context.i64_type().const_zero()))
            }

            Expr::Channel { typ: _, capacity } => {
                let create_fn = self.get_or_declare_sigil_channel_create();

                // Channels carry 8-byte slots — a handle or an int value — which is
                // the unit the handle-based model passes between behaviors.
                let elem_size = self.context.i64_type().const_int(8, false);
                let cap_val = self.compile_expr(capacity)?.as_int()?;

                let result = self.builder.build_call(
                    create_fn,
                    &[elem_size.into(), cap_val.into()],
                    "channel"
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                let handle = result.try_as_basic_value().left()
                    .map(|v| v.into_int_value())
                    .unwrap_or_else(|| self.context.i64_type().const_zero());

                Ok(SigilValue::Immediate(handle))
            }

            Expr::ChannelReceive(channel_name) => {
                let recv_fn = self.get_or_declare_sigil_channel_receive();

                // Get channel handle
                let channel_val = self.variables.get(channel_name)
                    .ok_or_else(|| CodeGenError::UndefinedVariable(channel_name.clone()))?
                    .clone();
                let channel_int = channel_val.as_int()?;

                // Allocate buffer for received value
                let recv_buf = self.alloc_handle(8, HandleType::Int)?;

                // The runtime fills recv_buf with the received element and returns a
                // status (0 = received, -1 = closed-and-empty). We surface the VALUE,
                // not the status — `value = CHANNEL_RECEIVE ch` must bind the payload.
                self.builder.build_call(
                    recv_fn,
                    &[channel_int.into(), recv_buf.ptr.into()],
                    "recv"
                ).map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                let i64_ty = self.context.i64_type();
                let received = self.builder.build_load(i64_ty, recv_buf.ptr, "recv_val")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                Ok(SigilValue::Immediate(received.into_int_value()))
            }
        }
    }

    /// Compile a behavior CALL
    pub(crate) fn compile_call(&mut self, behavior: &str, args: &[Expr]) -> Result<SigilValue<'ctx>> {
        let fn_name = format!("sigil_{}", behavior.replace('-', "_").replace('/', "_"));

        let callee = self.behavior_fns.get(behavior)
            .copied()
            .or_else(|| self.module.get_function(&fn_name))
            .ok_or_else(|| CodeGenError::UndefinedBehavior(behavior.into()))?;

        let sig = self.behavior_sigs.get(behavior).cloned()
            .unwrap_or_else(|| BehaviorSig {
                name: behavior.into(),
                input_sizes: vec![8; args.len()],
                outputs: vec![],
            });

        // Compile args and wrap in handles
        let mut call_args: Vec<BasicMetadataValueEnum> = Vec::new();
        for arg in args {
            let val = self.compile_expr(arg)?;
            let handle = self.immediate_to_handle(val)?;
            call_args.push(handle.ptr.into());
        }

        // Allocate the call's output buffer. This can be large — a native like
        // `read`/`receive` declares a 65536-byte OUTPUT buffer — so it must go on
        // the heap via the scope allocator, not a stack `alloca` (a 64 KB stack
        // buffer in a frame with other locals overflows the stack: 0xC0000409).
        // It is freed at the behavior's scope/return, the same lifetime a stack
        // alloca had. Outside a behavior scope this falls back to a stack alloca.
        let total_size = sig.total_output_size();
        let output_handle = if total_size > 0 {
            // Output buffer type defaults to Bytes (multi-field composite)
            let handle = self.scope_alloc_handle(total_size, HandleType::Bytes)?;

            // Add output pointers to args
            for slot in &sig.outputs {
                let ptr = if slot.offset > 0 {
                    let i8_type = self.context.i8_type();
                    let offset = self.context.i64_type().const_int(slot.offset as u64, false);
                    unsafe {
                        self.builder.build_gep(i8_type, handle.ptr, &[offset], &format!("{}_out", slot.name))
                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                    }
                } else {
                    handle.ptr
                };
                call_args.push(ptr.into());
            }
            Some(handle)
        } else {
            None
        };

        // Make the call
        self.builder.build_call(callee, &call_args, "")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Store call outputs for field access
        if let Some(handle) = output_handle {
            self.call_outputs.insert("__pending".into(), CallOutputs {
                buffer: handle,
                slots: sig.outputs.clone(),
            });
            Ok(SigilValue::Handle(handle))
        } else {
            Ok(SigilValue::Immediate(self.context.i64_type().const_zero()))
        }
    }

    // =========================================================================
    // Expression Compilation Helpers
    // =========================================================================

    /// Normalize two int values to the same type (extend smaller to larger)
    /// This fixes type mismatches like comparing i8 with i64
    fn normalize_int_types(&mut self, av: IntValue<'ctx>, bv: IntValue<'ctx>) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let av_bits = av.get_type().get_bit_width();
        let bv_bits = bv.get_type().get_bit_width();

        if av_bits == bv_bits {
            Ok((av, bv))
        } else if av_bits > bv_bits {
            // Extend b to match a's type
            let extended = self.builder.build_int_s_extend(bv, av.get_type(), "ext")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            Ok((av, extended))
        } else {
            // Extend a to match b's type
            let extended = self.builder.build_int_s_extend(av, bv.get_type(), "ext")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            Ok((extended, bv))
        }
    }

    pub(crate) fn compile_int_binop<F>(&mut self, a: &Expr, b: &Expr, op: F) -> Result<SigilValue<'ctx>>
    where
        F: FnOnce(&inkwell::builder::Builder<'ctx>, IntValue<'ctx>, IntValue<'ctx>) -> std::result::Result<IntValue<'ctx>, inkwell::builder::BuilderError>
    {
        let av = self.compile_expr(a)?.as_int()?;
        let bv = self.compile_expr(b)?.as_int()?;
        let (av, bv) = self.normalize_int_types(av, bv)?;
        let result = op(&self.builder, av, bv)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(SigilValue::Immediate(result))
    }

    pub(crate) fn compile_int_cmp(&mut self, a: &Expr, b: &Expr, pred: IntPredicate) -> Result<SigilValue<'ctx>> {
        let av = self.compile_expr(a)?.as_int()?;
        let bv = self.compile_expr(b)?.as_int()?;
        let (av, bv) = self.normalize_int_types(av, bv)?;
        let cmp = self.builder.build_int_compare(pred, av, bv, "cmp")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let ext = self.builder.build_int_z_extend(cmp, self.context.i64_type(), "cmp_ext")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(SigilValue::Immediate(ext))
    }

    pub(crate) fn compile_float_binop<F>(&mut self, a: &Expr, b: &Expr, op: F) -> Result<SigilValue<'ctx>>
    where
        F: FnOnce(&inkwell::builder::Builder<'ctx>, FloatValue<'ctx>, FloatValue<'ctx>) -> std::result::Result<FloatValue<'ctx>, inkwell::builder::BuilderError>
    {
        let av = self.compile_expr(a)?.as_float()?;
        let bv = self.compile_expr(b)?.as_float()?;
        let result = op(&self.builder, av, bv)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(SigilValue::Float(result))
    }

    pub(crate) fn compile_float_cmp(&mut self, a: &Expr, b: &Expr, pred: inkwell::FloatPredicate) -> Result<SigilValue<'ctx>> {
        let av = self.compile_expr(a)?.as_float()?;
        let bv = self.compile_expr(b)?.as_float()?;
        let cmp = self.builder.build_float_compare(pred, av, bv, "fcmp")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let ext = self.builder.build_int_z_extend(cmp, self.context.i64_type(), "fcmp_ext")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(SigilValue::Immediate(ext))
    }
}
