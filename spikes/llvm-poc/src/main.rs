//! Toolchain spike for `designs/llvm-compilation.md` §1.6 (issue #110).
//!
//! Proves the full loop on this machine: build LLVM IR with inkwell for a
//! `main` that calls `puts("keel poc")`, emit a native object file through
//! `TargetMachine`, link it with the system `cc` driver, run the binary,
//! and assert on its exit code and stdout.
//!
//! Throwaway code: panics (`expect`) are the intended failure mode — any
//! failure means the toolchain assumption is broken and the spike must stop.

use std::env;
use std::path::Path;
use std::process::Command;

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::module::Linkage;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

/// The exact line the compiled binary must print.
const EXPECTED_OUTPUT: &str = "keel poc";

fn main() {
    // 1. Build the module: `int main(void) { puts("keel poc"); return 0; }`.
    let context = Context::create();
    let module = context.create_module("keel_poc");
    let builder = context.create_builder();

    let i32_type = context.i32_type();
    let ptr_type = context.ptr_type(inkwell::AddressSpace::default());

    // `puts` is declared, not defined — the C library provides it at link time.
    let puts_type = i32_type.fn_type(&[ptr_type.into()], false);
    let puts_fn = module.add_function("puts", puts_type, Some(Linkage::External));

    let main_type = i32_type.fn_type(&[], false);
    let main_fn = module.add_function("main", main_type, None);
    let entry = context.append_basic_block(main_fn, "entry");
    builder.position_at_end(entry);

    let greeting = builder
        .build_global_string_ptr(EXPECTED_OUTPUT, "greeting")
        .expect("build global string");
    builder
        .build_call(puts_fn, &[greeting.as_pointer_value().into()], "puts_call")
        .expect("build call to puts");
    builder
        .build_return(Some(&i32_type.const_zero()))
        .expect("build return");

    module.verify().expect("LLVM module verification failed");
    println!("--- LLVM IR ---\n{}", module.print_to_string().to_string());

    // 2. Emit a native object file for the host target.
    Target::initialize_native(&InitializationConfig::default()).expect("initialize native target");
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).expect("target from host triple");
    let machine = target
        .create_target_machine(
            &triple,
            TargetMachine::get_host_cpu_name()
                .to_str()
                .expect("cpu name utf-8"),
            TargetMachine::get_host_cpu_features()
                .to_str()
                .expect("cpu features utf-8"),
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .expect("create target machine");

    let out_dir = env::temp_dir().join("keel-llvm-poc");
    std::fs::create_dir_all(&out_dir).expect("create output dir");
    let obj_path = out_dir.join("keel_poc.o");
    let bin_path = out_dir.join("keel_poc");
    machine
        .write_to_file(&module, FileType::Object, &obj_path)
        .expect("emit object file");
    println!("emitted object: {}", obj_path.display());

    // 3. Link with the system `cc` driver.
    link(&obj_path, &bin_path);

    // 4. Run the binary and assert exit code + output.
    let run = Command::new(&bin_path)
        .output()
        .expect("run compiled binary");
    let stdout = String::from_utf8(run.stdout).expect("binary stdout is utf-8");
    assert!(
        run.status.success(),
        "binary exited with {:?}",
        run.status.code()
    );
    assert_eq!(stdout.trim_end(), EXPECTED_OUTPUT, "unexpected stdout");

    println!(
        "triple: {}",
        triple.as_str().to_str().expect("triple utf-8")
    );
    println!("binary: {}", bin_path.display());
    println!("exit code: 0, stdout: {stdout:?}");
    println!("POC OK: emit -> link -> run loop verified");
}

/// Links a single object file into an executable via the system `cc` driver.
fn link(obj: &Path, bin: &Path) {
    let status = Command::new("cc")
        .arg(obj)
        .arg("-o")
        .arg(bin)
        .status()
        .expect("spawn cc");
    assert!(status.success(), "cc link step failed: {status:?}");
    println!("linked binary: {}", bin.display());
}
