# Toolchain Spike: LLVM/inkwell on this machine

Appendix to `designs/llvm-compilation.md` §1.6. Resolves issue #110.
Result: **the emit → object → link → run loop works on both target platforms**,
verified by independently building and running the proof below on macOS arm64
natively and on Linux x86_64 in an emulated container (not just agent self-report).

## Pinned versions

| Component | Version | Notes |
|---|---|---|
| `inkwell` | `0.9.0` | latest on crates.io at time of writing; supports up to `llvm22-1` |
| Cargo feature | `llvm22-1` | selects the LLVM 22 binding generation in inkwell/llvm-sys |
| `llvm-sys` (resolved) | `221.0.1` | pulled transitively by inkwell's feature |
| LLVM (system, macOS) | `22.1.8` via Homebrew (`llvm@22`) | keg-only, not symlinked into `/opt/homebrew/bin` |
| LLVM (system, Linux) | `22.1.8` via apt.llvm.org (`llvm-22`, Ubuntu 24.04) | see Install below — needs an extra static-lib package |
| Host triples verified | `arm64-apple-darwin25.5.0`, `x86_64-pc-linux-gnu` | macOS arm64 native; Linux x86_64 in Docker (`--platform linux/amd64` under emulation on an arm64 host) |

## Install

### macOS

```sh
brew install llvm@22
export LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm@22
```

Homebrew keeps `llvm@22` keg-only (no `llvm-config` on `PATH` — confirmed: neither
`llvm-config` nor `llvm-config-22` resolves after install), so `llvm-sys` needs the
prefix pointed at explicitly. Runtime linking uses the system `cc` (Xcode CLT); no
separate `lld` setup was needed for this proof.

### Linux (Ubuntu 24.04)

```sh
curl -fsSL https://apt.llvm.org/llvm.sh -o /tmp/llvm.sh && chmod +x /tmp/llvm.sh
/tmp/llvm.sh 22
apt-get install -y libpolly-22-dev zlib1g-dev libzstd-dev
export LLVM_SYS_221_PREFIX=/usr/lib/llvm-22
```

`llvm.sh 22` alone is not sufficient: `llvm-sys` links `libLLVMPolly.a` and it isn't
pulled in by `llvm-22-dev` — the build fails with `could not find native static
library Polly` until `libpolly-22-dev` (plus `zlib1g-dev`/`libzstd-dev`, LLVM's own
static-link dependencies) is installed explicitly. Runtime linking uses the
system `cc` (`build-essential`'s gcc); `lld-22` is pulled in as a dependency but
wasn't needed for this proof.

The env var name is version-coupled (`LLVM_SYS_221_PREFIX` for llvm-sys 221.x) on
both platforms — it will change if inkwell moves to a different LLVM major.

## Proof

`spikes/llvm-poc/` (standalone crate, own empty `[workspace]` table so it never joins
the root workspace — confirmed `cargo build --workspace` at the repo root does not
pull it in and stays LLVM-free):

```
// int main(void) { puts("keel poc"); return 0; }
```

Built via inkwell → verified module → emitted a native object with
`TargetMachine::write_to_file` → linked with `cc obj -o bin` → ran the binary.

macOS arm64, native:

```
$ LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm@22 cargo run --release
triple: arm64-apple-darwin25.5.0
exit code: 0, stdout: "keel poc\n"
POC OK: emit -> link -> run loop verified
```

Linux x86_64, `ubuntu:24.04` container (`docker run --platform linux/amd64`, emulated
on an arm64 host via the install steps above):

```
$ LLVM_SYS_221_PREFIX=/usr/lib/llvm-22 cargo run --release
triple: x86_64-pc-linux-gnu
exit code: 0, stdout: "keel poc\n"
POC OK: emit -> link -> run loop verified
```

Full loop confirmed end to end on both platforms.

## CI recipe (not yet implemented, sketch only)

- **macOS runner**: `brew install llvm@22`, export `LLVM_SYS_221_PREFIX`, cache the
  Homebrew keg between runs (the install is the slow step, several minutes).
- **Linux runner**: the apt.llvm.org install above (`llvm.sh 22` +
  `libpolly-22-dev zlib1g-dev libzstd-dev`), same `LLVM_SYS_221_PREFIX` pattern
  pointed at `/usr/lib/llvm-22`. Verified working in this spike, just not yet wired
  into an actual CI job.
- Gate this entirely behind the `build-backend` cargo feature (per
  `designs/llvm-compilation.md` §2.2) so `keel run`/`check`/`lsp` CI jobs never
  install LLVM.

## Open items for later milestones

- Version drift: re-pin when inkwell ships support for a newer LLVM major; the
  `llvm22-1` feature flag and `LLVM_SYS_221_PREFIX` var both move together.
- Linux verification above ran under x86_64 emulation on an arm64 dev machine, not
  real x86_64 CI hardware — expected to be faster there but not yet measured.
- `lld` as a faster link driver: not evaluated in this spike; `cc` was sufficient on
  both platforms.
