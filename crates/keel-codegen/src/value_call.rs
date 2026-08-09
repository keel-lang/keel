//! `CallTarget::ValueMethod` codegen (issue #214): structurally identical to
//! `ns_call.rs`'s `emit_ns_call` — box every argument, call the one generic
//! runtime dispatch entry point, unbox the result — except the receiver is
//! boxed and passed separately, and dispatch goes by method *name* (embedded
//! as a NUL-terminated C string constant) rather than a numeric
//! `ns_id`/`method_id` pair, since value methods aren't in the
//! `keel-catalog` namespace registry.

use inkwell::AddressSpace;
use inkwell::values::{BasicValueEnum, ValueKind};

use keel_kir::ir::Expr;
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::func::FuncCtx;
use crate::ns_call::{declare_or_get, emit_box_arg};

fn llvm_err(e: impl std::fmt::Display) -> CodegenError {
    CodegenError::Llvm(e.to_string())
}

/// Embeds `s` as a NUL-terminated C string constant and returns a pointer to
/// it — the `*const c_char` shape `keel_rt_call_value_method`'s `method`
/// parameter expects, distinct from `ns_call::emit_box_str_const`'s boxed
/// `Value::String` (a `KeelBox`, not a bare C string).
fn emit_cstr_const<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    s: &str,
) -> Result<inkwell::values::PointerValue<'ctx>, CodegenError> {
    let const_arr = fcx.context.const_string(s.as_bytes(), true);
    let arr_ty = fcx.context.i8_type().array_type(s.len() as u32 + 1);

    // Every call site adds a global under the same base name ("method_name")
    // regardless of which method it names — LLVM auto-uniquifies same-named
    // globals with *different* initializers (appends `.N`), so two distinct
    // method names never collide into one buffer; only two call sites naming
    // the *same* method could ever share a global, which is safe since their
    // content is identical. Verified end to end by
    // `value_method_calls.rs`'s `distinct_method_name_constants_do_not_collide`.
    let global = fcx.module.add_global(arr_ty, None, "method_name");
    global.set_initializer(&const_arr);
    global.set_constant(true);
    Ok(global.as_pointer_value())
}

/// Emits a `CallTarget::ValueMethod { method }` call site. `args[0]` is the
/// receiver (boxed separately from the rest, matching
/// `keel_rt_call_value_method`'s signature); `args[1..]` are the method's
/// own arguments.
pub(crate) fn emit_value_method_call<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    method: &str,
    args: &[Expr],
    ty: KirType,
    span_id: u32,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let ptr_type = fcx.context.ptr_type(AddressSpace::default());
    let i32_type = fcx.context.i32_type();

    let [receiver, rest @ ..] = args else {
        unreachable!("verified KIR: CallTarget::ValueMethod always has a receiver in args[0]");
    };
    let receiver_ptr = emit_box_arg(fcx, receiver)?;
    let method_ptr = emit_cstr_const(fcx, method)?;

    let boxed: Vec<_> = rest
        .iter()
        .map(|arg| emit_box_arg(fcx, arg))
        .collect::<Result<_, _>>()?;

    let nargs = u32::try_from(boxed.len()).expect("KIR call arg count fits u32");
    let array_ty = ptr_type.array_type(nargs.max(1));
    let args_alloca = fcx
        .builder
        .build_alloca(array_ty, "vm_args")
        .map_err(llvm_err)?;
    let names_alloca = fcx
        .builder
        .build_alloca(array_ty, "vm_arg_names")
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
        .struct_type(&[fcx.context.i8_type().into(), ptr_type.into()], false);
    let keel_rt_call_value_method = declare_or_get(fcx.module, "keel_rt_call_value_method", || {
        keel_res_ty.fn_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                ptr_type.into(),
                ptr_type.into(),
                i32_type.into(),
                i32_type.into(),
            ],
            false,
        )
    });

    let call = fcx
        .builder
        .build_call(
            keel_rt_call_value_method,
            &[
                receiver_ptr.into(),
                method_ptr.into(),
                args_alloca.into(),
                names_alloca.into(),
                i32_type.const_int(u64::from(nargs), false).into(),
                i32_type.const_int(u64::from(span_id), false).into(),
            ],
            "value_method_call",
        )
        .map_err(llvm_err)?;

    let res = match call.try_as_basic_value() {
        ValueKind::Basic(v) => v.into_struct_value(),
        ValueKind::Instruction(_) => {
            unreachable!("keel_rt_call_value_method returns KeelRes, never void")
        }
    };
    let payload = fcx
        .builder
        .build_extract_value(res, 1, "value_method_call.payload")
        .map_err(llvm_err)?
        .into_pointer_value();
    crate::rt_call::unbox_value(fcx, payload, ty)
}
