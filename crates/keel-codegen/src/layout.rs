//! `KirType` -> LLVM type. M1 walking-skeleton scope: `I64`/`F64`/`Bool`
//! only, per `designs/llvm-compilation.md` §1.1's lowering table — `Str`
//! (needs the `KeelStr` runtime ABI) and `Unit` (has no LLVM *value*
//! representation; call sites branch on it directly, see `func.rs`) are not
//! representable as a [`BasicTypeEnum`] and are rejected here.

use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;

use keel_kir::types::KirType;

use crate::CodegenError;

pub(crate) fn llvm_type<'ctx>(
    context: &'ctx Context,
    ty: KirType,
) -> Result<BasicTypeEnum<'ctx>, CodegenError> {
    match ty {
        KirType::I64 => Ok(context.i64_type().into()),
        KirType::F64 => Ok(context.f64_type().into()),
        KirType::Bool => Ok(context.bool_type().into()),
        KirType::Unit => Err(CodegenError::Unsupported(
            "none (Unit) has no LLVM value representation".to_string(),
        )),
        KirType::Str => Err(CodegenError::Unsupported(
            "str (needs the KeelStr runtime ABI, issue #135)".to_string(),
        )),
    }
}
