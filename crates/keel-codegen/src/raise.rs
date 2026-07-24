//! `raise`/`try`/`catch` codegen — the result calling convention for
//! `CallTarget::Fn` calls to a `can_raise` function (`designs/llvm-
//! compilation.md` §2.5). Every `can_raise` function's LLVM return type is
//! `{ i1 is_err, ptr payload }`: `payload` is always a boxed `*const Value`
//! on success (reusing `keel_box_*`/`unbox_value` — an int/float/bool
//! success value is boxed, a str/list/struct success value is already a
//! native `ptr` and passes through untouched, `none` boxes `Value::None`)
//! and the synthetic `UserRaised` struct pointer (already a native `ptr` —
//! see `ir.rs`'s `Stmt::TryCatch` doc) on error.
//!
//! This struct is purely internal to the LLVM module — it never crosses
//! into `keel-rt-ffi`, so it isn't `#[repr(C)]`/shared with `KeelRes` (the
//! analogous shape `CallTarget::Ns` calls use — a distinct, untouched-by-
//! this-issue path; see `ns_dispatch.rs`'s doc on why namespace calls don't
//! participate in this convention yet).

use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, PointerValue};

use keel_kir::ir::Expr;
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::func::{FuncCtx, llvm_err};
use crate::ns_call::declare_or_get;
use crate::rt_call::{call_ptr_fn, unbox_value};

/// The `{ i1 is_err, ptr payload }` result-ABI struct type every
/// `can_raise` function returns, regardless of its own logical return type.
pub(crate) fn result_abi_type<'ctx>(context: &'ctx Context) -> StructType<'ctx> {
    context.struct_type(
        &[
            context.bool_type().into(),
            context.ptr_type(AddressSpace::default()).into(),
        ],
        false,
    )
}

/// Builds a `{ is_err, payload }` result-ABI value (a runtime `payload`
/// composed via `build_insert_value`, same pattern as `nullable.rs`'s
/// `emit_wrap_some` for the scalar-nullable `{ i1, T }` pair).
fn build_result<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    is_err: bool,
    payload: PointerValue<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let struct_ty = result_abi_type(fcx.context);
    let undef = struct_ty.get_undef();
    let flag = fcx.context.bool_type().const_int(u64::from(is_err), false);
    let with_flag = fcx
        .builder
        .build_insert_value(undef, flag, 0, "res.flag")
        .map_err(llvm_err)?;
    let with_payload = fcx
        .builder
        .build_insert_value(with_flag, payload, 1, "res.payload")
        .map_err(llvm_err)?;
    Ok(with_payload.into_struct_value().into())
}

/// Boxes a `can_raise` function's success value into the result-ABI's
/// uniformly-`ptr` payload slot. `value` is `None` only for `ty ==
/// KirType::Unit`. `ty` is restricted (by `keel-kir`'s `compute_can_raise`)
/// to int/float/bool/str/list/none — struct/enum/nullable success values
/// need `Value` marshaling that doesn't exist yet, a later M2/M3 concern.
pub(crate) fn emit_box_result_value<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    ty: KirType,
    value: Option<BasicValueEnum<'ctx>>,
) -> Result<PointerValue<'ctx>, CodegenError> {
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());
    match ty {
        KirType::Unit => {
            let f = declare_or_get(fcx.module, "keel_box_none", || ptr_type.fn_type(&[], false));
            Ok(call_ptr_fn(fcx, f, &[])?.into_pointer_value())
        }
        KirType::I64 => {
            let v = value
                .expect("non-Unit ty always carries a value")
                .into_int_value();
            let f = declare_or_get(fcx.module, "keel_box_int", || {
                ptr_type.fn_type(&[fcx.context.i64_type().into()], false)
            });
            Ok(call_ptr_fn(fcx, f, &[v.into()])?.into_pointer_value())
        }
        KirType::F64 => {
            let v = value
                .expect("non-Unit ty always carries a value")
                .into_float_value();
            let f = declare_or_get(fcx.module, "keel_box_float", || {
                ptr_type.fn_type(&[fcx.context.f64_type().into()], false)
            });
            Ok(call_ptr_fn(fcx, f, &[v.into()])?.into_pointer_value())
        }
        KirType::Bool => {
            let v = value
                .expect("non-Unit ty always carries a value")
                .into_int_value();
            // `keel_box_bool` takes a `u8` (see abi/mod.rs) — Bool is `i1`
            // on the LLVM side (layout.rs), so widen it first.
            let v8 = fcx
                .builder
                .build_int_z_extend(v, fcx.context.i8_type(), "bool_to_u8")
                .map_err(llvm_err)?;
            let f = declare_or_get(fcx.module, "keel_box_bool", || {
                ptr_type.fn_type(&[fcx.context.i8_type().into()], false)
            });
            Ok(call_ptr_fn(fcx, f, &[v8.into()])?.into_pointer_value())
        }
        KirType::Str | KirType::List(_) | KirType::Struct(_) => Ok(value
            .expect("non-Unit ty always carries a value")
            .into_pointer_value()),
        other => Err(CodegenError::Unsupported(format!(
            "a can-raise function returning `{other}` (struct/enum/nullable success values need \
             Value marshaling, a later M2/M3 concern — keel-kir's compute_can_raise should have \
             rejected this)"
        ))),
    }
}

/// Inverse of `emit_box_result_value` — unboxes a `can_raise` call's
/// success payload back to `ty`'s native representation. Delegates to
/// `rt_call::unbox_value` for the cases it already handles correctly
/// (that function is list-element-unboxing scoped, so `Unit`/`Struct`
/// — never valid list elements — are handled here instead, not folded
/// into its own match).
pub(crate) fn emit_unbox_result_value<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    ptr: PointerValue<'ctx>,
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    match ty {
        // Matches `expr.rs`'s existing void-call convention: a Unit-typed
        // value is never inspected by any caller, only ever discarded.
        KirType::Unit => Ok(fcx.context.bool_type().const_zero().into()),
        KirType::Struct(_) => Ok(ptr.into()),
        KirType::I64 | KirType::F64 | KirType::Bool | KirType::Str | KirType::List(_) => {
            unbox_value(fcx, ptr, ty)
        }
        other => Err(CodegenError::Unsupported(format!(
            "a can-raise function returning `{other}` (struct/enum/nullable success values need \
             Value marshaling, a later M2/M3 concern)"
        ))),
    }
}

/// Builds `{ is_err: 0, payload: boxed(value) }` and emits `return` with it
/// — the success path of a `can_raise` function's result-ABI, from both an
/// explicit `return` (`stmt.rs`'s `emit_return`) and control falling off
/// the end of a `none`-returning `can_raise` function (`func.rs`'s
/// `finish_block`).
pub(crate) fn emit_ok_return<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    ty: KirType,
    value: Option<BasicValueEnum<'ctx>>,
) -> Result<(), CodegenError> {
    let payload = emit_box_result_value(fcx, ty, value)?;
    let result = build_result(fcx, false, payload)?;
    fcx.builder.build_return(Some(&result)).map_err(llvm_err)?;
    Ok(())
}

/// Emits `raise error` (`stmt.rs`'s `emit_raise`) — evaluates the already-
/// constructed `UserRaised` struct value (`error`, an `Expr::MakeStruct` —
/// see `ir.rs`'s `Stmt::Raise` doc) and immediately returns the current
/// (always `can_raise`) function's error branch with it.
pub(crate) fn emit_raise<'ctx>(fcx: &FuncCtx<'ctx, '_>, error: &Expr) -> Result<(), CodegenError> {
    let error_ptr = crate::expr::emit_expr(fcx, error)?.into_pointer_value();
    let result = build_result(fcx, true, error_ptr)?;
    fcx.builder.build_return(Some(&result)).map_err(llvm_err)?;
    Ok(())
}

/// Emits the `is_err` branch after a call to a `can_raise` `CallTarget::Fn`
/// callee (`expr.rs`'s `emit_call`): the error arm either stores the
/// payload into the innermost active `try`'s binder and jumps to its
/// handler (`fcx.catch_stack`'s top entry), or — if no `try` is active —
/// propagates by returning this function's own error branch with the same
/// payload (always sound: an uncaught call to a `can_raise` callee is
/// exactly what makes *this* function `can_raise` too, per `keel-kir`'s
/// `compute_can_raise`). Returns the unboxed success value, with the
/// builder positioned at the "ok" continuation block — composes with
/// whatever expression context the call appeared in, same as a plain call.
pub(crate) fn emit_call_result_branch<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    result: BasicValueEnum<'ctx>,
    ret_ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let result = result.into_struct_value();
    let is_err = fcx
        .builder
        .build_extract_value(result, 0, "call.is_err")
        .map_err(llvm_err)?
        .into_int_value();
    let payload = fcx
        .builder
        .build_extract_value(result, 1, "call.payload")
        .map_err(llvm_err)?
        .into_pointer_value();

    let err_bb = fcx.context.append_basic_block(fcx.function, "call.err");
    let ok_bb = fcx.context.append_basic_block(fcx.function, "call.ok");
    fcx.builder
        .build_conditional_branch(is_err, err_bb, ok_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(err_bb);
    match fcx.catch_stack.last() {
        Some(&(handler_bb, binder_ptr)) => {
            fcx.builder
                .build_store(binder_ptr, payload)
                .map_err(llvm_err)?;
            fcx.builder
                .build_unconditional_branch(handler_bb)
                .map_err(llvm_err)?;
        }
        None => {
            let propagated = build_result(fcx, true, payload)?;
            fcx.builder
                .build_return(Some(&propagated))
                .map_err(llvm_err)?;
        }
    }

    fcx.builder.position_at_end(ok_bb);
    emit_unbox_result_value(fcx, payload, ret_ty)
}
