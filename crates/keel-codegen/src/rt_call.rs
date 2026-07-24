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
    }
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
        KirType::Str | KirType::List(_) => Ok(boxed.into()),
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
