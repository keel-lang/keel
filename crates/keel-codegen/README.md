# keel-codegen

`KirType`/KIR → LLVM IR → native object → linked binary, via [`inkwell`](https://docs.rs/inkwell).
The only crate in the workspace that links LLVM (`designs/llvm-compilation.md` §2.2)
— `keel run`/`check`/`lsp` never need it, since the root crate only depends on
this one behind the `build-backend` cargo feature.

## Toolchain requirement

Building (or testing) this crate — or the root crate with `--features
build-backend` — requires a system LLVM 22 install, found via `llvm-sys`'s
`LLVM_SYS_221_PREFIX` environment variable. Full install steps and the
verified proof-of-concept for both macOS and Linux live in
[`designs/llvm-toolchain-spike.md`](../../designs/llvm-toolchain-spike.md);
short version:

```sh
# macOS
brew install llvm@22
export LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm@22

# Linux (Ubuntu)
curl -fsSL https://apt.llvm.org/llvm.sh -o /tmp/llvm.sh && chmod +x /tmp/llvm.sh
sudo /tmp/llvm.sh 22
sudo apt-get install -y libpolly-22-dev zlib1g-dev libzstd-dev
export LLVM_SYS_221_PREFIX=/usr/lib/llvm-22
```

Without `LLVM_SYS_221_PREFIX` set (and no `llvm-config`/`llvm-config-22` on
`PATH`), `cargo build -p keel-codegen` fails at the `llvm-sys` build script
with an actionable error naming the missing prefix.
