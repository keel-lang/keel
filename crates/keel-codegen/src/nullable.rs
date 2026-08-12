//! `T?` codegen — `Expr::NullLit`/`NullCoalesce`/`NullFieldGet`
//! (`designs/llvm-compilation.md` §1.1's nullable representation split;
//! §2.3: "`?.`/`??` -> explicit branches").
//!
//! Three representations, keyed off the nullable's *inner* type
//! (`KirType::Nullable`'s doc has the full rationale):
//! - **Scalar** (int/float/bool): an explicit `{ i1 has_value, T }` pair, by
//!   value — no pointer to repurpose.
//! - **Struct**: the same `ptr` as the non-nullable struct; `none` is a
//!   plain null pointer (a native struct record is never `Value`-boxed, so
//!   this costs nothing extra).
//! - **Str/list**: also the same `ptr`, but `none` is a boxed `Value::None`
//!   instead — str/list are already boxed `*const Value` pointers with no
//!   null-pointer bit to spare (`keel-rt-ffi`'s `keel_box_none`/
//!   `keel_is_none`).
//!
//! `?.`'s own null-check (whether the receiver *struct* is present) is
//! always the cheap pointer-null check, regardless of what the accessed
//! field's type is — only the field's *own* nullable-none-building (when
//! the receiver is absent) or unwrapping (when present) depends on the
//! field's type.

use inkwell::AddressSpace;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue};

use keel_kir::ir::{Expr, StructId};
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::func::FuncCtx;
use crate::layout;
use crate::ns_call::declare_or_get;
use crate::rt_call::{call_ptr_fn, call_scalar_fn};

fn llvm_err(e: impl std::fmt::Display) -> CodegenError {
    CodegenError::Llvm(e.to_string())
}

pub(crate) fn nullable_inner<'ctx>(fcx: &FuncCtx<'ctx, '_>, ty: KirType) -> KirType {
    let KirType::Nullable(id) = ty else {
        unreachable!(
            "caller only invokes this on a KirType::Nullable — keel-kir's verify pass \
                      should have rejected anything else"
        );
    };
    fcx.program.nullables[id]
}

/// Builds the `none` value of `ty` (always `KirType::Nullable(_)`).
pub(crate) fn emit_null_lit<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let inner = nullable_inner(fcx, ty);
    match inner {
        KirType::I64 | KirType::F64 | KirType::Bool => {
            let struct_ty = layout::llvm_type(fcx.context, fcx.program, ty)?.into_struct_type();
            let zero: BasicValueEnum = match inner {
                KirType::I64 => fcx.context.i64_type().const_zero().into(),
                KirType::F64 => fcx.context.f64_type().const_zero().into(),
                KirType::Bool => fcx.context.bool_type().const_zero().into(),
                _ => unreachable!(),
            };
            let has_value = fcx.context.bool_type().const_zero();
            Ok(struct_ty
                .const_named_struct(&[has_value.into(), zero])
                .into())
        }
        KirType::Struct(_) => Ok(fcx
            .context
            .ptr_type(AddressSpace::default())
            .const_null()
            .into()),
        KirType::Str | KirType::List(_) => {
            let ptr_type = fcx.context.ptr_type(AddressSpace::default());
            let f = declare_or_get(fcx.module, "keel_box_none", || ptr_type.fn_type(&[], false));
            call_ptr_fn(fcx, f, &[])
        }
        other => Err(CodegenError::Unsupported(format!(
            "nullable `{other}` (only int/float/bool/str/list/struct inner types are modeled)"
        ))),
    }
}

/// Widens a plain, known-present `value` into `ty`'s (`KirType::Nullable`)
/// "some" representation — the checker allows passing a non-nullable `T`
/// wherever `T?` is expected (`Expr::NullSome`'s doc has the full
/// rationale).
pub(crate) fn emit_null_some<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    value: &Expr,
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let inner_ty = nullable_inner(fcx, ty);
    let raw = crate::expr::emit_expr(fcx, value)?;
    let nullable_llvm_ty = layout::llvm_type(fcx.context, fcx.program, ty)?;
    emit_wrap_some(fcx, raw, inner_ty, nullable_llvm_ty)
}

/// Reports whether `val` (already-evaluated, of `inner_ty`'s nullable
/// representation) is `none`.
fn emit_is_none<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    val: BasicValueEnum<'ctx>,
    inner_ty: KirType,
) -> Result<IntValue<'ctx>, CodegenError> {
    match inner_ty {
        KirType::I64 | KirType::F64 | KirType::Bool => {
            let has_value = fcx
                .builder
                .build_extract_value(val.into_struct_value(), 0, "has_value")
                .map_err(llvm_err)?
                .into_int_value();
            fcx.builder
                .build_not(has_value, "is_none")
                .map_err(llvm_err)
        }
        KirType::Struct(_) => fcx
            .builder
            .build_is_null(val.into_pointer_value(), "is_none")
            .map_err(llvm_err),
        KirType::Str | KirType::List(_) => {
            let ptr_type = fcx.context.ptr_type(AddressSpace::default());
            let f = declare_or_get(fcx.module, "keel_is_none", || {
                fcx.context.i8_type().fn_type(&[ptr_type.into()], false)
            });
            let raw = call_scalar_fn(fcx, f, &[val.into()], "is_none_u8")?.into_int_value();
            fcx.builder
                .build_int_compare(
                    inkwell::IntPredicate::NE,
                    raw,
                    fcx.context.i8_type().const_zero(),
                    "is_none",
                )
                .map_err(llvm_err)
        }
        other => Err(CodegenError::Unsupported(format!(
            "nullable `{other}` (only int/float/bool/str/list/struct inner types are modeled)"
        ))),
    }
}

/// Extracts the plain `inner_ty`-typed value out of a nullable's *known-
/// present* ("some") representation — the inverse of [`emit_wrap_some`].
/// A pointer-typed inner (str/list/struct) is already the right bits and
/// passes through untouched; only a scalar pair needs unpacking.
fn emit_unwrap_some<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    val: BasicValueEnum<'ctx>,
    inner_ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    match inner_ty {
        KirType::I64 | KirType::F64 | KirType::Bool => fcx
            .builder
            .build_extract_value(val.into_struct_value(), 1, "unwrap_some")
            .map_err(llvm_err),
        KirType::Struct(_) | KirType::Str | KirType::List(_) => Ok(val),
        other => Err(CodegenError::Unsupported(format!(
            "nullable `{other}` (only int/float/bool/str/list/struct inner types are modeled)"
        ))),
    }
}

/// `Expr::IsNone` codegen — issue #230. Evaluates `nullable`, then reuses
/// the same [`emit_is_none`] check `??`'s own codegen (`emit_null_coalesce`)
/// uses, so the two can never drift on what "none" means for a given
/// representation.
pub(crate) fn emit_is_none_expr<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    nullable: &Expr,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let inner_ty = nullable_inner(fcx, nullable.ty());
    let nullable_v = crate::expr::emit_expr(fcx, nullable)?;
    Ok(emit_is_none(fcx, nullable_v, inner_ty)?.into())
}

/// `Expr::UnwrapSome` codegen — issue #230. Evaluates `nullable`, then
/// reuses [`emit_unwrap_some`]. No null check of its own: `keel-kir`'s
/// lowering only ever constructs `Expr::UnwrapSome` where an `Expr::IsNone`
/// test on the same `nullable` has already gated the branch this appears
/// in (see `ir.rs`'s doc on the variant) — a wrong or missing guard is a
/// lowering bug, not something this function can detect at codegen time.
pub(crate) fn emit_unwrap_some_expr<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    nullable: &Expr,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let inner_ty = nullable_inner(fcx, nullable.ty());
    let nullable_v = crate::expr::emit_expr(fcx, nullable)?;
    emit_unwrap_some(fcx, nullable_v, inner_ty)
}

/// Builds the nullable-`{ty}`'s "some" representation from a known-present,
/// plain `inner_ty`-typed value — the inverse of [`emit_unwrap_some`].
pub(crate) fn emit_wrap_some<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    raw: BasicValueEnum<'ctx>,
    inner_ty: KirType,
    nullable_llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    match inner_ty {
        KirType::I64 | KirType::F64 | KirType::Bool => {
            let struct_ty = nullable_llvm_ty.into_struct_type();
            let has_value = fcx.context.bool_type().const_int(1, false);
            let undef = struct_ty.get_undef();
            let with_flag = fcx
                .builder
                .build_insert_value(undef, has_value, 0, "with_flag")
                .map_err(llvm_err)?;
            let with_value = fcx
                .builder
                .build_insert_value(with_flag, raw, 1, "with_value")
                .map_err(llvm_err)?;
            Ok(with_value.into_struct_value().into())
        }
        KirType::Struct(_) | KirType::Str | KirType::List(_) => Ok(raw),
        other => Err(CodegenError::Unsupported(format!(
            "nullable `{other}` (only int/float/bool/str/list/struct inner types are modeled)"
        ))),
    }
}

/// `nullable ?? fallback` — short-circuits: `fallback` is only evaluated
/// (and only its basic block entered) when `nullable` is `none`.
pub(crate) fn emit_null_coalesce<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    nullable: &Expr,
    fallback: &Expr,
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let inner_ty = nullable_inner(fcx, nullable.ty());
    let nullable_v = crate::expr::emit_expr(fcx, nullable)?;
    let is_none = emit_is_none(fcx, nullable_v, inner_ty)?;

    let result_llvm_ty = layout::llvm_type(fcx.context, fcx.program, ty)?;
    let result_ptr = fcx
        .builder
        .build_alloca(result_llvm_ty, "nullcoalesce.result")
        .map_err(llvm_err)?;

    let some_bb = fcx
        .context
        .append_basic_block(fcx.function, "nullcoalesce.some");
    let none_bb = fcx
        .context
        .append_basic_block(fcx.function, "nullcoalesce.none");
    let merge_bb = fcx
        .context
        .append_basic_block(fcx.function, "nullcoalesce.merge");
    fcx.builder
        .build_conditional_branch(is_none, none_bb, some_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(some_bb);
    let some_val = emit_unwrap_some(fcx, nullable_v, inner_ty)?;
    fcx.builder
        .build_store(result_ptr, some_val)
        .map_err(llvm_err)?;
    fcx.builder
        .build_unconditional_branch(merge_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(none_bb);
    let fallback_val = crate::expr::emit_expr(fcx, fallback)?;
    fcx.builder
        .build_store(result_ptr, fallback_val)
        .map_err(llvm_err)?;
    fcx.builder
        .build_unconditional_branch(merge_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(merge_bb);
    fcx.builder
        .build_load(result_llvm_ty, result_ptr, "nullcoalesce.load")
        .map_err(llvm_err)
}

/// `base?.field` — `base` is a nullable-struct-typed expression;
/// short-circuits to `none` (of the field's nullable type) without ever
/// touching the field when `base` is `none`.
pub(crate) fn emit_null_field_get<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    base: &Expr,
    field_index: usize,
    ty: KirType,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let KirType::Struct(struct_id): KirType = nullable_inner(fcx, base.ty()) else {
        return Err(CodegenError::Llvm(
            "`?.` base's nullable inner type is not a struct — keel-kir's verify pass should \
             have rejected this"
                .to_string(),
        ));
    };
    let base_ptr = crate::expr::emit_expr(fcx, base)?.into_pointer_value();
    let is_none = fcx
        .builder
        .build_is_null(base_ptr, "base_is_none")
        .map_err(llvm_err)?;

    let result_llvm_ty = layout::llvm_type(fcx.context, fcx.program, ty)?;
    let result_ptr = fcx
        .builder
        .build_alloca(result_llvm_ty, "nullfieldget.result")
        .map_err(llvm_err)?;

    let some_bb = fcx
        .context
        .append_basic_block(fcx.function, "nullfieldget.some");
    let none_bb = fcx
        .context
        .append_basic_block(fcx.function, "nullfieldget.none");
    let merge_bb = fcx
        .context
        .append_basic_block(fcx.function, "nullfieldget.merge");
    fcx.builder
        .build_conditional_branch(is_none, none_bb, some_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(some_bb);
    let some_val = emit_some_field(fcx, base_ptr, struct_id, field_index, ty, result_llvm_ty)?;
    fcx.builder
        .build_store(result_ptr, some_val)
        .map_err(llvm_err)?;
    fcx.builder
        .build_unconditional_branch(merge_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(none_bb);
    let none_val = emit_null_lit(fcx, ty)?;
    fcx.builder
        .build_store(result_ptr, none_val)
        .map_err(llvm_err)?;
    fcx.builder
        .build_unconditional_branch(merge_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(merge_bb);
    fcx.builder
        .build_load(result_llvm_ty, result_ptr, "nullfieldget.load")
        .map_err(llvm_err)
}

fn emit_some_field<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    base_ptr: inkwell::values::PointerValue<'ctx>,
    struct_id: StructId,
    field_index: usize,
    ty: KirType,
    result_llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let layout_ty = layout::struct_layout_type(fcx.context, fcx.program, struct_id)?;
    let field_index_u32 = u32::try_from(field_index).expect("struct field count fits u32");
    let field_ptr = fcx
        .builder
        .build_struct_gep(layout_ty, base_ptr, field_index_u32, "field_ptr")
        .map_err(llvm_err)?;
    let field_ty = nullable_inner(fcx, ty);
    let field_llvm_ty = layout::llvm_type(fcx.context, fcx.program, field_ty)?;
    let raw_field = fcx
        .builder
        .build_load(field_llvm_ty, field_ptr, "field_load")
        .map_err(llvm_err)?;
    emit_wrap_some(fcx, raw_field, field_ty, result_llvm_ty)
}
