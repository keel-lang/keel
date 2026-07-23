//! `CallTarget::Ns` codegen: boxes each argument into a `KeelBox` (via the
//! `keel_box_*` FFI helpers `keel-rt-ffi` exports) and calls the one
//! generic `keel_rt_call_ns` dispatch entry point
//! (`designs/llvm-compilation.md` §2.7). `ns_id`/`method_id` were already
//! resolved and validated against the catalog at KIR-lowering time
//! (`keel-kir`'s `lower_ns_call`) — codegen just has to get the two structs'
//! ABI shapes byte-for-byte identical to `keel-rt-ffi`'s `#[repr(C)]`
//! definitions (`abi::keel_box_*`, `ns_dispatch::KeelRes`).
//!
//! M1 only wires `Unit`-returning methods (`io.show`, `log.*` — enforced by
//! `keel-kir` rejecting any other result type for now), so the returned
//! `KeelRes` is built with the right calling convention but never inspected:
//! there is no `try`/`catch` lowering yet to branch on `is_err`.

use inkwell::AddressSpace;
use inkwell::module::Module;
use inkwell::types::FunctionType;
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue, ValueKind,
};

use keel_kir::ir::Expr;
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::expr::emit_expr;
use crate::func::FuncCtx;

fn llvm_err(e: impl std::fmt::Display) -> CodegenError {
    CodegenError::Llvm(e.to_string())
}

/// Returns `module`'s existing declaration of `name`, or declares one with
/// `make_ty()`. Every call site needing a given runtime-provided (or, for
/// `malloc`, libc-provided) function shares this one declaration, so this
/// avoids redeclaring (and LLVM silently renaming) it per call site.
pub(crate) fn declare_or_get<'ctx>(
    module: &Module<'ctx>,
    name: &str,
    make_ty: impl FnOnce() -> FunctionType<'ctx>,
) -> FunctionValue<'ctx> {
    module
        .get_function(name)
        .unwrap_or_else(|| module.add_function(name, make_ty(), None))
}

/// Calls a `keel_box_*` helper (always returns a non-null `KeelBox`, never
/// void) and extracts the returned pointer.
fn call_box_fn<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    f: FunctionValue<'ctx>,
    args: &[BasicMetadataValueEnum<'ctx>],
) -> Result<PointerValue<'ctx>, CodegenError> {
    let call = fcx.builder.build_call(f, args, "box").map_err(llvm_err)?;
    match call.try_as_basic_value() {
        ValueKind::Basic(v) => Ok(v.into_pointer_value()),
        ValueKind::Instruction(_) => unreachable!("keel_box_* always returns a KeelBox pointer"),
    }
}

pub(crate) fn emit_box_arg<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    arg: &Expr,
) -> Result<PointerValue<'ctx>, CodegenError> {
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());
    match arg.ty() {
        // Every `str` value is already a boxed `*const Value` (`KeelBox`) by
        // construction (`expr::emit_expr`'s `ConstStr`/`FieldGet`/etc. — see
        // `layout.rs`'s module doc), so no extra boxing is needed here,
        // unlike the scalar cases below (which store an unboxed LLVM
        // primitive and box it fresh at this call boundary).
        KirType::Str => Ok(emit_expr(fcx, arg)?.into_pointer_value()),
        KirType::I64 => {
            let v = emit_expr(fcx, arg)?.into_int_value();
            let f = declare_or_get(fcx.module, "keel_box_int", || {
                ptr_type.fn_type(&[fcx.context.i64_type().into()], false)
            });
            call_box_fn(fcx, f, &[v.into()])
        }
        KirType::F64 => {
            let v = emit_expr(fcx, arg)?.into_float_value();
            let f = declare_or_get(fcx.module, "keel_box_float", || {
                ptr_type.fn_type(&[fcx.context.f64_type().into()], false)
            });
            call_box_fn(fcx, f, &[v.into()])
        }
        KirType::Bool => {
            let v = emit_expr(fcx, arg)?.into_int_value();
            // `keel_box_bool` takes a `u8` (see abi/mod.rs) — Bool is `i1` on
            // the LLVM side (layout.rs), so widen it first.
            let v8 = fcx
                .builder
                .build_int_z_extend(v, fcx.context.i8_type(), "bool_to_u8")
                .map_err(llvm_err)?;
            let f = declare_or_get(fcx.module, "keel_box_bool", || {
                ptr_type.fn_type(&[fcx.context.i8_type().into()], false)
            });
            call_box_fn(fcx, f, &[v8.into()])
        }
        KirType::Unit => Err(CodegenError::Unsupported(
            "unit-typed argument to a namespace call".to_string(),
        )),
        KirType::Struct(_) => Err(CodegenError::Unsupported(
            "struct-typed argument to a namespace call (marshaling a struct into a boxed \
             Value::Struct is a later-M2/M3 concern — see designs/llvm-compilation.md §2.4)"
                .to_string(),
        )),
        KirType::Enum(_) => Err(CodegenError::Unsupported(
            "enum-typed argument to a namespace call (marshaling an enum tag into a boxed \
             Value is a later-M2/M3 concern)"
                .to_string(),
        )),
        // A `list[T]` value is already a boxed `*const Value` (`Value::List`,
        // same representation `keel_rt_call_ns`/every `keel_list_*` expects)
        // — no extra boxing needed, same as `Str`.
        KirType::List(_) => Ok(emit_expr(fcx, arg)?.into_pointer_value()),
    }
}

pub(crate) fn emit_box_str_const<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    s: &str,
) -> Result<PointerValue<'ctx>, CodegenError> {
    let bytes = s.as_bytes();
    let const_arr = fcx.context.const_string(bytes, false);
    let arr_ty = fcx.context.i8_type().array_type(bytes.len() as u32);

    // LLVM auto-uniquifies colliding global names (appends `.N`), so every
    // string-literal call site can share this one literal name.
    let global = fcx.module.add_global(arr_ty, None, "str_const");
    global.set_initializer(&const_arr);
    global.set_constant(true);
    let ptr = global.as_pointer_value();
    let len = fcx.context.i64_type().const_int(bytes.len() as u64, false);

    let ptr_type = fcx.context.ptr_type(AddressSpace::default());
    let f = declare_or_get(fcx.module, "keel_box_str", || {
        ptr_type.fn_type(&[ptr_type.into(), fcx.context.i64_type().into()], false)
    });
    call_box_fn(fcx, f, &[ptr.into(), len.into()])
}

/// Emits a `CallTarget::Ns { ns_id, method_id }` call site: boxes every
/// argument, builds the `args`/`arg_names` arrays `keel_rt_call_ns` expects,
/// and calls it. `arg_names` is always all-null — `keel-kir`'s
/// `lower_ns_call` already rejects named arguments to stdlib calls, so no
/// compiled call site can produce one yet.
pub(crate) fn emit_ns_call<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    ns_id: u16,
    method_id: u16,
    args: &[Expr],
    span_id: u32,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());
    let i16_type = fcx.context.i16_type();
    let i32_type = fcx.context.i32_type();
    let i8_type = fcx.context.i8_type();

    let boxed: Vec<PointerValue<'ctx>> = args
        .iter()
        .map(|arg| emit_box_arg(fcx, arg))
        .collect::<Result<_, _>>()?;

    let nargs = u32::try_from(boxed.len()).expect("KIR call arg count fits u32");
    let array_ty = ptr_type.array_type(nargs.max(1));
    let args_alloca = fcx
        .builder
        .build_alloca(array_ty, "ns_args")
        .map_err(llvm_err)?;
    let names_alloca = fcx
        .builder
        .build_alloca(array_ty, "ns_arg_names")
        .map_err(llvm_err)?;

    for (i, ptr) in boxed.iter().enumerate() {
        let idx = i32_type.const_int(i as u64, false);
        // SAFETY: `idx` is always in bounds — it ranges over `boxed`, whose
        // length is exactly `array_ty`'s element count (or less, when the
        // `.max(1)` padding above allocated one unused slot for `nargs == 0`).
        let slot = unsafe {
            fcx.builder
                .build_gep(
                    array_ty,
                    args_alloca,
                    &[i32_type.const_zero(), idx],
                    "arg_slot",
                )
                .map_err(llvm_err)?
        };
        fcx.builder.build_store(slot, *ptr).map_err(llvm_err)?;

        // SAFETY: see above.
        let name_slot = unsafe {
            fcx.builder
                .build_gep(
                    array_ty,
                    names_alloca,
                    &[i32_type.const_zero(), idx],
                    "name_slot",
                )
                .map_err(llvm_err)?
        };
        fcx.builder
            .build_store(name_slot, ptr_type.const_null())
            .map_err(llvm_err)?;
    }

    let keel_res_ty = fcx
        .context
        .struct_type(&[i8_type.into(), ptr_type.into()], false);
    let keel_rt_call_ns = declare_or_get(fcx.module, "keel_rt_call_ns", || {
        keel_res_ty.fn_type(
            &[
                i16_type.into(),
                i16_type.into(),
                ptr_type.into(),
                ptr_type.into(),
                i32_type.into(),
                i32_type.into(),
            ],
            false,
        )
    });

    fcx.builder
        .build_call(
            keel_rt_call_ns,
            &[
                i16_type.const_int(u64::from(ns_id), false).into(),
                i16_type.const_int(u64::from(method_id), false).into(),
                args_alloca.into(),
                names_alloca.into(),
                i32_type.const_int(u64::from(nargs), false).into(),
                i32_type.const_int(u64::from(span_id), false).into(),
            ],
            "ns_call",
        )
        .map_err(llvm_err)?;

    // M1 only wires Unit-returning methods (enforced by keel-kir's
    // lower_ns_call), so the caller (expr::emit_call) never inspects this —
    // same convention as a CallTarget::Fn void call.
    Ok(fcx.context.bool_type().const_zero().into())
}
