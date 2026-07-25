//! `KirType` -> LLVM type, per `designs/llvm-compilation.md` §1.1's
//! lowering table. `Unit` has no LLVM *value* representation (call sites
//! branch on it directly, see `func.rs`) and is rejected here. `Str` is
//! always `ptr` — every `str` value is a boxed `*const Value` (`KeelBox`,
//! built via `keel_box_str`; see `expr.rs`'s `ConstStr` codegen), the same
//! representation `keel_rt_call_ns` already expects for call arguments,
//! there is no separate unboxed `KeelStr` in this implementation. A named
//! struct (`KirType::Struct`) is `ptr` too, when it has a heap field
//! (`StructLayout::is_heap`) — an all-scalar struct would be a by-value
//! LLVM aggregate, but that path isn't wired up yet (every M2 fixture uses
//! a struct with at least one `str` field). A simple enum (`KirType::Enum`)
//! is a plain by-value `i32` tag — no heap allocation or RC, unlike `Struct`.

use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;

use keel_kir::ir::KirProgram;
use keel_kir::types::KirType;

use crate::CodegenError;

pub(crate) fn llvm_type<'ctx>(
    context: &'ctx Context,
    program: &KirProgram,
    ty: KirType,
) -> Result<BasicTypeEnum<'ctx>, CodegenError> {
    match ty {
        KirType::I64 => Ok(context.i64_type().into()),
        KirType::F64 => Ok(context.f64_type().into()),
        KirType::Bool => Ok(context.bool_type().into()),
        KirType::Unit => Err(CodegenError::Unsupported(
            "none (Unit) has no LLVM value representation".to_string(),
        )),
        KirType::Str => Ok(context.ptr_type(AddressSpace::default()).into()),
        KirType::Struct(id) => {
            if program.structs[id].is_heap(program) {
                Ok(context.ptr_type(AddressSpace::default()).into())
            } else {
                Err(CodegenError::Unsupported(format!(
                    "all-scalar struct `{}` (by-value aggregate codegen isn't wired up yet — \
                     every M2 fixture uses a struct with at least one heap field)",
                    program.structs[id].name
                )))
            }
        }
        KirType::Enum(_) => Ok(context.i32_type().into()),
        KirType::List(_) | KirType::Map(_) | KirType::Set(_) => {
            Ok(context.ptr_type(AddressSpace::default()).into())
        }
        KirType::Nullable(id) => match program.nullables[id] {
            // A nullable scalar has no spare pointer bit to steal, so it's
            // an explicit `{ i1 has_value, T }` pair, by value.
            KirType::I64 | KirType::F64 | KirType::Bool => {
                let inner = llvm_type(context, program, program.nullables[id])?;
                Ok(context
                    .struct_type(&[context.bool_type().into(), inner], false)
                    .into())
            }
            // A nullable struct/str/list is the same `ptr` as the
            // non-nullable type — `none` is a null pointer for a struct, or
            // a boxed `Value::None` for str/list (see `KirType::Nullable`'s
            // doc); either way the LLVM-level representation is identical.
            KirType::Struct(_) | KirType::Str | KirType::List(_) => {
                Ok(context.ptr_type(AddressSpace::default()).into())
            }
            other => Err(CodegenError::Unsupported(format!(
                "nullable `{other}` (only int/float/bool/str/list/struct inner types are \
                 modeled — `is_nullable_inner_ty` should have rejected this at lowering time)"
            ))),
        },
    }
}

/// The struct's field layout as a real LLVM struct type — used to compute
/// its heap allocation size and to `GEP` into its fields by index. This is
/// *not* the struct's own `KirType` representation (always `ptr`, a heap
/// struct is never passed by value) — it's the shape the pointer points at.
pub(crate) fn struct_layout_type<'ctx>(
    context: &'ctx Context,
    program: &KirProgram,
    struct_id: keel_kir::ir::StructId,
) -> Result<inkwell::types::StructType<'ctx>, CodegenError> {
    let field_types = program.structs[struct_id]
        .fields
        .iter()
        .map(|(_, ty)| llvm_type(context, program, *ty))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(context.struct_type(&field_types, false))
}
