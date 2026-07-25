//! `CallTarget::Rt` codegen — the container-ABI runtime calls
//! (`designs/llvm-compilation.md` §2.7: "container primitives... are
//! synchronous `CallTarget::Rt` calls into the container ABI from day
//! one"). Every container value is a boxed `*const Value` (`KeelBox`, same
//! representation `CallTarget::Ns` already uses for `str` — there is no
//! separate `#[repr(C)] KeelList`, see `keel-rt-ffi`'s `keel_list_*` docs),
//! so element marshaling reuses `ns_call::emit_box_arg`'s exact per-type
//! dispatch.

use inkwell::AddressSpace;
use inkwell::values::{BasicValueEnum, ValueKind};

use keel_kir::ir::{Expr, RtFn};
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::func::FuncCtx;
use crate::ns_call::{declare_or_get, emit_box_arg};

fn llvm_err(e: impl std::fmt::Display) -> CodegenError {
    CodegenError::Llvm(e.to_string())
}

pub(crate) fn emit_rt_call<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    rt_fn: RtFn,
    args: &[Expr],
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());

    match rt_fn {
        RtFn::ListNew => {
            let f = declare_or_get(fcx.module, "keel_list_new", || ptr_type.fn_type(&[], false));
            call_ptr_fn(fcx, f, &[])
        }
        RtFn::ListPush => {
            let [list, elem] = args else {
                unreachable!("verified KIR: RtFn::ListPush always takes exactly 2 args");
            };
            let list_ptr = crate::expr::emit_expr(fcx, list)?.into_pointer_value();
            let elem_ptr = emit_box_arg(fcx, elem)?;
            let f = declare_or_get(fcx.module, "keel_list_push", || {
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false)
            });
            call_ptr_fn(fcx, f, &[list_ptr.into(), elem_ptr.into()])
        }
        RtFn::ListLen => {
            let [list] = args else {
                unreachable!("verified KIR: RtFn::ListLen always takes exactly 1 arg");
            };
            let list_ptr = crate::expr::emit_expr(fcx, list)?.into_pointer_value();
            let f = declare_or_get(fcx.module, "keel_list_len", || {
                fcx.context.i64_type().fn_type(&[ptr_type.into()], false)
            });
            let call = fcx
                .builder
                .build_call(f, &[list_ptr.into()], "list_len")
                .map_err(llvm_err)?;
            match call.try_as_basic_value() {
                ValueKind::Basic(v) => Ok(v),
                ValueKind::Instruction(_) => unreachable!("keel_list_len returns i64, never void"),
            }
        }
        RtFn::IntToStr => {
            let [value] = args else {
                unreachable!("verified KIR: RtFn::IntToStr always takes exactly 1 arg");
            };
            let v = crate::expr::emit_expr(fcx, value)?.into_int_value();
            let f = declare_or_get(fcx.module, "keel_int_to_str", || {
                ptr_type.fn_type(&[fcx.context.i64_type().into()], false)
            });
            call_ptr_fn(fcx, f, &[v.into()])
        }
        RtFn::FloatToStr => {
            let [value] = args else {
                unreachable!("verified KIR: RtFn::FloatToStr always takes exactly 1 arg");
            };
            let v = crate::expr::emit_expr(fcx, value)?.into_float_value();
            let f = declare_or_get(fcx.module, "keel_float_to_str", || {
                ptr_type.fn_type(&[fcx.context.f64_type().into()], false)
            });
            call_ptr_fn(fcx, f, &[v.into()])
        }
        RtFn::BoolToStr => {
            let [value] = args else {
                unreachable!("verified KIR: RtFn::BoolToStr always takes exactly 1 arg");
            };
            let v = crate::expr::emit_expr(fcx, value)?.into_int_value();
            // `keel_bool_to_str` takes a `u8` (see abi/mod.rs) — Bool is `i1`
            // on the LLVM side (layout.rs), so widen it first.
            let v8 = fcx
                .builder
                .build_int_z_extend(v, fcx.context.i8_type(), "bool_to_u8")
                .map_err(llvm_err)?;
            let f = declare_or_get(fcx.module, "keel_bool_to_str", || {
                ptr_type.fn_type(&[fcx.context.i8_type().into()], false)
            });
            call_ptr_fn(fcx, f, &[v8.into()])
        }
        RtFn::MapNew => {
            let f = declare_or_get(fcx.module, "keel_map_new", || ptr_type.fn_type(&[], false));
            call_ptr_fn(fcx, f, &[])
        }
        RtFn::MapInsert => {
            let [map, key, val] = args else {
                unreachable!("verified KIR: RtFn::MapInsert always takes exactly 3 args");
            };
            let map_ptr = crate::expr::emit_expr(fcx, map)?.into_pointer_value();
            let key_ptr = emit_box_arg(fcx, key)?;
            let val_ptr = emit_box_arg(fcx, val)?;
            let f = declare_or_get(fcx.module, "keel_map_insert", || {
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), ptr_type.into()], false)
            });
            call_ptr_fn(fcx, f, &[map_ptr.into(), key_ptr.into(), val_ptr.into()])
        }
        RtFn::MapGet => {
            let [map, key] = args else {
                unreachable!("verified KIR: RtFn::MapGet always takes exactly 2 args");
            };
            let map_ptr = crate::expr::emit_expr(fcx, map)?.into_pointer_value();
            let key_ptr = emit_box_arg(fcx, key)?;
            let f = declare_or_get(fcx.module, "keel_map_get", || {
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false)
            });
            let result_boxed =
                call_ptr_fn(fcx, f, &[map_ptr.into(), key_ptr.into()])?.into_pointer_value();
            emit_map_get_result(fcx, result_boxed, ty)
        }
        RtFn::MapLen => {
            let [map] = args else {
                unreachable!("verified KIR: RtFn::MapLen always takes exactly 1 arg");
            };
            let map_ptr = crate::expr::emit_expr(fcx, map)?.into_pointer_value();
            let f = declare_or_get(fcx.module, "keel_map_len", || {
                fcx.context.i64_type().fn_type(&[ptr_type.into()], false)
            });
            let call = fcx
                .builder
                .build_call(f, &[map_ptr.into()], "map_len")
                .map_err(llvm_err)?;
            match call.try_as_basic_value() {
                ValueKind::Basic(v) => Ok(v),
                ValueKind::Instruction(_) => unreachable!("keel_map_len returns i64, never void"),
            }
        }
        RtFn::MapContains => {
            let [map, key] = args else {
                unreachable!("verified KIR: RtFn::MapContains always takes exactly 2 args");
            };
            let map_ptr = crate::expr::emit_expr(fcx, map)?.into_pointer_value();
            let key_ptr = emit_box_arg(fcx, key)?;
            let f = declare_or_get(fcx.module, "keel_map_contains", || {
                fcx.context
                    .i8_type()
                    .fn_type(&[ptr_type.into(), ptr_type.into()], false)
            });
            let raw = call_scalar_fn(fcx, f, &[map_ptr.into(), key_ptr.into()], "map_contains_u8")?
                .into_int_value();
            let b = fcx
                .builder
                .build_int_truncate(raw, fcx.context.bool_type(), "map_contains")
                .map_err(llvm_err)?;
            Ok(b.into())
        }
        RtFn::MapKeys => {
            let [map] = args else {
                unreachable!("verified KIR: RtFn::MapKeys always takes exactly 1 arg");
            };
            let map_ptr = crate::expr::emit_expr(fcx, map)?.into_pointer_value();
            let f = declare_or_get(fcx.module, "keel_map_keys", || {
                ptr_type.fn_type(&[ptr_type.into()], false)
            });
            call_ptr_fn(fcx, f, &[map_ptr.into()])
        }
        RtFn::MapValues => {
            let [map] = args else {
                unreachable!("verified KIR: RtFn::MapValues always takes exactly 1 arg");
            };
            let map_ptr = crate::expr::emit_expr(fcx, map)?.into_pointer_value();
            let f = declare_or_get(fcx.module, "keel_map_values", || {
                ptr_type.fn_type(&[ptr_type.into()], false)
            });
            call_ptr_fn(fcx, f, &[map_ptr.into()])
        }
        // `set[T]` shares `list[T]`'s exact runtime representation (see
        // `KirType::Set`'s doc) — these two variants exist only so
        // `keel-kir`'s verify pass can require the *static* result type to
        // be `KirType::Set`, not `KirType::List`; codegen calls the
        // identical `keel_list_*` symbols `RtFn::ListNew`/`ListPush` do.
        RtFn::SetNew => {
            let f = declare_or_get(fcx.module, "keel_list_new", || ptr_type.fn_type(&[], false));
            call_ptr_fn(fcx, f, &[])
        }
        RtFn::SetInsert => {
            let [set, elem] = args else {
                unreachable!("verified KIR: RtFn::SetInsert always takes exactly 2 args");
            };
            let set_ptr = crate::expr::emit_expr(fcx, set)?.into_pointer_value();
            let elem_ptr = emit_box_arg(fcx, elem)?;
            let f = declare_or_get(fcx.module, "keel_list_push", || {
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false)
            });
            call_ptr_fn(fcx, f, &[set_ptr.into(), elem_ptr.into()])
        }
    }
}

/// Wraps `keel_map_get`'s boxed `*const Value` result (a boxed `Value::None`
/// on a missing key, the boxed value otherwise) into `ty`'s
/// (`KirType::Nullable`) representation — `keel_is_none` distinguishes the
/// two cases, then either unboxes-and-wraps or builds a fresh `none`,
/// mirroring `crate::nullable`'s own `?.`/`??` branch-and-merge shape.
fn emit_map_get_result<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    boxed: inkwell::values::PointerValue<'ctx>,
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let value_ty = crate::nullable::nullable_inner(fcx, ty);
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());

    let is_none_fn = declare_or_get(fcx.module, "keel_is_none", || {
        fcx.context.i8_type().fn_type(&[ptr_type.into()], false)
    });
    let raw_is_none =
        call_scalar_fn(fcx, is_none_fn, &[boxed.into()], "map_get_is_none_u8")?.into_int_value();
    let is_none = fcx
        .builder
        .build_int_compare(
            inkwell::IntPredicate::NE,
            raw_is_none,
            fcx.context.i8_type().const_zero(),
            "map_get_is_none",
        )
        .map_err(llvm_err)?;

    let result_llvm_ty = crate::layout::llvm_type(fcx.context, fcx.program, ty)?;
    let result_ptr = fcx
        .builder
        .build_alloca(result_llvm_ty, "map_get.result")
        .map_err(llvm_err)?;

    let some_bb = fcx.context.append_basic_block(fcx.function, "map_get.some");
    let none_bb = fcx.context.append_basic_block(fcx.function, "map_get.none");
    let merge_bb = fcx
        .context
        .append_basic_block(fcx.function, "map_get.merge");
    fcx.builder
        .build_conditional_branch(is_none, none_bb, some_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(some_bb);
    let raw_val = unbox_value(fcx, boxed, value_ty)?;
    let some_val = crate::nullable::emit_wrap_some(fcx, raw_val, value_ty, result_llvm_ty)?;
    fcx.builder
        .build_store(result_ptr, some_val)
        .map_err(llvm_err)?;
    fcx.builder
        .build_unconditional_branch(merge_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(none_bb);
    let none_val = crate::nullable::emit_null_lit(fcx, ty)?;
    fcx.builder
        .build_store(result_ptr, none_val)
        .map_err(llvm_err)?;
    fcx.builder
        .build_unconditional_branch(merge_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(merge_bb);
    fcx.builder
        .build_load(result_llvm_ty, result_ptr, "map_get.load")
        .map_err(llvm_err)
}

/// Calls `keel_list_len` directly (not through an `Expr::Call` — used by
/// `stmt::emit_for_each`'s internal loop counter, which isn't itself a KIR
/// node).
pub(crate) fn emit_list_len<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    list_ptr: inkwell::values::PointerValue<'ctx>,
) -> Result<inkwell::values::IntValue<'ctx>, CodegenError> {
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());
    let f = declare_or_get(fcx.module, "keel_list_len", || {
        fcx.context.i64_type().fn_type(&[ptr_type.into()], false)
    });
    Ok(call_ptr_fn(fcx, f, &[list_ptr.into()])?.into_int_value())
}

/// Calls `keel_list_get` directly, returning the still-boxed element (the
/// caller unboxes via [`unbox_value`]) — same rationale as
/// [`emit_list_len`].
pub(crate) fn emit_list_get<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    list_ptr: inkwell::values::PointerValue<'ctx>,
    index: inkwell::values::IntValue<'ctx>,
) -> Result<inkwell::values::PointerValue<'ctx>, CodegenError> {
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());
    let f = declare_or_get(fcx.module, "keel_list_get", || {
        ptr_type.fn_type(&[ptr_type.into(), fcx.context.i64_type().into()], false)
    });
    Ok(call_ptr_fn(fcx, f, &[list_ptr.into(), index.into()])?.into_pointer_value())
}

/// Emits `xs[i]` — bounds-checked in `keel_list_get` itself (see its doc),
/// then unboxes the result to `ty` (the reverse of `emit_box_arg`: a
/// scalar comes back as a raw LLVM primitive, a `str` element is already a
/// `ptr` and needs no unboxing).
pub(crate) fn emit_index<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    list: &Expr,
    index: &Expr,
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let list_ptr = crate::expr::emit_expr(fcx, list)?.into_pointer_value();
    let index_v = crate::expr::emit_expr(fcx, index)?.into_int_value();
    let elem_ptr = emit_list_get(fcx, list_ptr, index_v)?;
    unbox_value(fcx, elem_ptr, ty)
}

/// Unboxes a `*const Value` (`KeelBox`) to its raw LLVM representation:
/// scalars come back as `i64`/`f64`/`i1` via a `keel_unbox_*` call; a `str`
/// (or another `list[T]`) is already the right representation (a `ptr`) and
/// passes through untouched.
pub(crate) fn unbox_value<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    boxed: inkwell::values::PointerValue<'ctx>,
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());
    match ty {
        KirType::I64 => {
            let f = declare_or_get(fcx.module, "keel_unbox_int", || {
                fcx.context.i64_type().fn_type(&[ptr_type.into()], false)
            });
            call_scalar_fn(fcx, f, &[boxed.into()], "unbox_int")
        }
        KirType::F64 => {
            let f = declare_or_get(fcx.module, "keel_unbox_float", || {
                fcx.context.f64_type().fn_type(&[ptr_type.into()], false)
            });
            call_scalar_fn(fcx, f, &[boxed.into()], "unbox_float")
        }
        KirType::Bool => {
            let f = declare_or_get(fcx.module, "keel_unbox_bool", || {
                fcx.context.i8_type().fn_type(&[ptr_type.into()], false)
            });
            let raw = call_scalar_fn(fcx, f, &[boxed.into()], "unbox_bool_u8")?.into_int_value();
            let b = fcx
                .builder
                .build_int_truncate(raw, fcx.context.bool_type(), "unbox_bool")
                .map_err(llvm_err)?;
            Ok(b.into())
        }
        KirType::Str | KirType::List(_) | KirType::Map(_) | KirType::Set(_) => Ok(boxed.into()),
        KirType::Unit | KirType::Struct(_) | KirType::Enum(_) | KirType::Nullable(_) => {
            Err(CodegenError::Unsupported(
                "a list element type other than int/float/bool/str (struct/enum/nullable \
                 elements need Value marshaling, a later M2/M3 concern)"
                    .to_string(),
            ))
        }
    }
}

/// Calls a runtime-ABI function that always returns a `ptr` (a boxed
/// `KeelBox` or similar), never void. Shared beyond this module's own
/// `keel_list_*` calls by [`crate::nullable`] (`keel_box_none`).
pub(crate) fn call_ptr_fn<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    f: inkwell::values::FunctionValue<'ctx>,
    args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let call = fcx
        .builder
        .build_call(f, args, "rt_call")
        .map_err(llvm_err)?;
    match call.try_as_basic_value() {
        ValueKind::Basic(v) => Ok(v),
        ValueKind::Instruction(_) => {
            unreachable!("every keel_list_* fn returns a value, never void")
        }
    }
}

/// Calls a runtime-ABI function that always returns a scalar, never void.
/// Shared beyond this module's own `keel_unbox_*` calls by
/// [`crate::nullable`] (`keel_is_none`).
pub(crate) fn call_scalar_fn<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    f: inkwell::values::FunctionValue<'ctx>,
    args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
    name: &str,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let call = fcx.builder.build_call(f, args, name).map_err(llvm_err)?;
    match call.try_as_basic_value() {
        ValueKind::Basic(v) => Ok(v),
        ValueKind::Instruction(_) => {
            unreachable!("every keel_unbox_* fn returns a value, never void")
        }
    }
}
