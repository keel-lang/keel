# Design: Compiling Keel to Native Code via LLVM

Status: proposal (pre-implementation)
Scope: `keel build file.keel -o binary` — AOT compilation to a native executable.
Supersedes: the bytecode-VM stub in `src/vm/` (see "Relationship to the VM stub" below).
SPEC alignment: SPEC §22 already lists "LLVM AOT backend → native binary" as a later-TBD stage.

---

## 1. Feasibility assessment

**Verdict: feasible. No hard blockers.** The design axes below each have at least one
proven solution used by a comparable language. The honest cost is effort (multi-month,
a new crate the size of `keel-compiler`) and a permanent two-engine maintenance burden
(interpreter + compiled path must agree semantically).

### 1.1 Type system → LLVM types

Keel is statically typed with inference (`Ty` in `crates/keel-compiler/src/types/ty.rs`
is fully resolved per expression). Every concrete variant has a direct lowering:

| Keel type (`Ty`) | LLVM representation |
|---|---|
| `int` | `i64` |
| `float` | `double` |
| `bool` | `i1` (i8 in memory) |
| `none` | zero-sized; functions returning `none` return `void` |
| `duration` | `double` (seconds — matches `Value::Duration(f64)`) |
| `datetime` | opaque runtime handle (`ptr`); runtime owns the representation |
| `Uuid` | `[16 x i8]` by value (canonicalize from today's string storage) |
| `str` | `ptr` to RC'd immutable `KeelStr` (runtime-provided) |
| `list[T]` / `set[T]` / `map[K,V]` | `ptr` to RC'd runtime container |
| named `struct` | LLVM named struct `%keel.T`; heap + RC when it contains heap fields |
| anonymous struct shape | shape-interned record (same layout rule, name derived from field set) |
| `tuple` | LLVM anonymous struct, by value |
| enum (simple) | `i32` tag |
| enum (rich variants) | `{ i32 tag, ptr payload }`; payload boxed per variant |
| `T?` (nullable) | pointer types: null pointer = `none`; scalars: `{ i1, T }` pair |
| `(A) -> B` (Func) | `{ ptr fn, ptr env }` pair (see closures, §1.3) |
| `dynamic`, `Unknown(ExternalDynamic)` | `ptr` to boxed tagged `KeelBox` (uniform representation) |
| `DbConnection` | opaque runtime handle (`ptr`) |
| `Ty::Error` / `Unresolved` | never reach codegen — `keel build` requires a clean `keel check` |

Generics (generic enums/structs/tasks, resolved by the checker's instantiation logic)
lower by **monomorphization**: each distinct instantiation gets its own LLVM function /
struct. Where a value flows into a `dynamic` position it is boxed instead. No runtime
generic dispatch is needed.

### 1.2 Memory model

Today the interpreter deep-clones `Value` — Keel has **value semantics** (no observable
aliasing of lists/maps/structs). Two properties make this easy to compile:

- No mutable shared references exist in the language surface.
- Closures do not capture their environment (see §1.3), so reference cycles cannot be
  constructed. `AgentRef` is a name, not an object pointer.

Chosen model: **atomic reference counting + copy-on-write**, implemented in the runtime
library (Rust `Arc`-style headers on `KeelStr`/`KeelList`/`KeelMap`/`KeelSet`/boxed
structs). Mutating operations check `refcount == 1` and clone otherwise — preserving the
interpreter's value semantics without deep-copying on every assignment.

- **No tracing GC, no LLVM statepoints/safepoints needed.** This avoids the hardest and
  worst-documented part of LLVM GC tooling entirely.
- No cycle collector needed under current semantics. If lexical closure capture is ever
  added (§6, open question 1), revisit — capture-by-value keeps the acyclic guarantee.
- LLVM supports this trivially: retain/release are ordinary function calls; the optimizer
  can be taught to elide pairs later (Swift-style), but correctness never depends on it.

### 1.3 Dynamic / reflective features audit

| Feature | Compilation story |
|---|---|
| `dynamic`, `json.parse`, `db.query` rows, `cache.get` | Boxed `KeelBox` values; `as T` casts unbox with runtime shape checks (same semantics as `apply_cast` in `interpreter/expr.rs`) |
| `typeof` | Static answer for typed expressions; tag read on boxed values |
| Lambdas | **Non-capturing today** (`call_closure` builds a fresh env from params only — `interpreter/call.rs:15`). They compile to plain function pointers; the `env` slot of the Func pair is reserved and null. |
| `testing.mock` (runtime patching of namespace methods) | **`keel test` stays on the interpreter.** Compiled binaries do not need mocking. This is a stated non-goal of the compiled path. |
| REPL | Stays on the interpreter permanently. |
| `extern` (plugin ABI: shared lib / subprocess+JSON) | Orthogonal — the runtime dispatches externs identically in both engines. |
| Agent `@tools` capability checks | Enforced in the runtime shims at call time, exactly as today (`AllowedTools` moves into the runtime library). |
| `eval` / runtime code loading | Does not exist in Keel. Nothing to do. |

No feature requires runtime code generation. Nothing resists AOT compilation outright.

### 1.4 The async/agent execution model (the one genuinely hard axis)

Keel's concurrency is agent mailboxes + `async.spawn`/`Task[T]` (SPEC §9), and the
interpreter is a fully async tree-walker on tokio. Task bodies *suspend* (LLM calls,
`async.sleep`, `Agent.delegate`, `io.ask`). Three known-good strategies:

1. **Synchronous codegen + runtime-scheduled stacks** (chosen for phase 1).
   Generated code is straight-line synchronous. Every suspending operation is an
   `extern "C"` shim that sends a request to the tokio reactor and parks the calling
   execution context. Contexts are either dedicated OS threads (simplest; agents number
   in the dozens, not millions) or stackful coroutines (e.g. `corosensei`) once thread
   count matters. This is the Go/Pony/Inko school: the language runtime, not LLVM,
   owns suspension. Codegen complexity: none.
2. **LLVM coroutine intrinsics** (`llvm.coro.*`) — what Swift/C++ use. Powerful but the
   worst-documented corner of LLVM, awkward through inkwell. Not phase 1.
3. **CPS / state-machine lowering in our own IR** — how Rust and Kotlin do it. Best
   performance ceiling, most compiler work. Possible phase-2 upgrade behind the same
   runtime ABI.

Because all I/O already lives in `keel-runtime` behind tokio, strategy 1 means the
compiled program embeds the *same* runtime the interpreter uses — the scheduler,
mailboxes, LLM clients, email, HTTP server all work unchanged.

### 1.5 Prior art

- **Inko** — actor-based language, compiler in Rust, LLVM backend via inkwell, ownership
  /RC memory model. The closest existing proof that this exact stack works.
- **Pony** — actor language, LLVM AOT, per-actor heaps. Validates actors-on-LLVM.
- **Swift** — RC + copy-on-write value semantics compiled through LLVM; validates §1.2.
- **Go** — validates "synchronous codegen, runtime owns scheduling" for green threads.
- **Roc, Grain, Crystal** — small static languages with LLVM/AOT backends built by small
  teams; calibrates effort as feasible-but-serious.

### 1.6 Toolchain

- **Bindings: `inkwell`** (safe Rust wrapper over `llvm-sys`). Mature, used by Inko and
  many hobby languages, includes `DIBuilder` for debug info (needed by the companion
  debugging design). Pin to the newest LLVM major that inkwell supports at
  implementation start (LLVM 18 as of this writing — verify then), via the versioned
  feature flag (`features = ["llvm18-1"]`).
- **LLVM install**: require a system LLVM (brew/apt) found via `llvm-sys`'s
  `LLVM_SYS_<ver>_PREFIX`; do **not** build LLVM from source in CI. `keel build` becomes
  a cargo feature (`--features build-backend`) so `keel run`/`check`/`lsp` users never
  need LLVM installed.
- **Linking**: emit an object file via `TargetMachine::write_to_file`, then invoke the
  system linker driver (`cc`) to link against `libkeel_rt.a` + system libs. `lld` as an
  optional fast path later.
- Known platform note from this repo: the macOS linker crash that forced `BoxedParser`
  is a chumsky/type-depth issue, not an LLVM concern; no interaction expected.

### 1.7 Explicitly rejected cheaper alternatives (for the record)

- **Transpile to Rust**: simplest correct backend, but compile times are poor and it
  makes Rust a user-facing dependency. Rejected per project direction (LLVM requested).
- **Cranelift**: pure-Rust, no C++ dependency, but weaker optimization and much weaker
  DWARF story. Could serve as a debug-build backend later; not the primary target.
- **Bytecode VM** (the `src/vm` stub): does not deliver native binaries, which is the
  point of `keel build`. See "Relationship to the VM stub".

---

## 2. Architecture

### 2.1 Pipeline

```
.keel source
  → lexer (keel-syntax, unchanged)
  → parser → AST (keel-syntax, unchanged)
  → type checker → per-expression Ty (keel-compiler, unchanged; build requires clean check)
  → HIR (keel-compiler/src/hir — extended: today read-only for IDE/semantic use)
  → KIR  (NEW: typed, desugared, monomorphized mid-level IR)
  → LLVM IR (NEW: keel-codegen crate, via inkwell)
  → object file → link with libkeel_rt.a → native binary
```

### 2.2 Code organization

Three new crates join the workspace. Existing crates are untouched except where
noted. Dependency rule: **only `keel-codegen` links LLVM**, and **`keel-rt-ffi`
never depends on LLVM** — so `keel run`/`check`/`lsp` builds stay LLVM-free.

```
crates/
  keel-syntax/            # UNCHANGED — lexer, parser, AST, formatter, lint
  keel-compiler/          # UNCHANGED — type checker, HIR, ModuleGraph, IDE queries
  keel-catalog/           # UNCHANGED — stdlib surface tables (checker, docs; also read
                          #   by KIR lowering for namespace/method id assignment)
  keel-runtime/           # runtime half unchanged; interpreter half untouched (see §3)

  keel-kir/               # NEW — typed mid-level IR + AST→KIR lowering
    src/
      lib.rs              # public entry: lower(ModuleGraph, CheckArtifacts) -> KirProgram
      ir.rs               # data model: KirProgram, Function, Block, Stmt, Expr, LocalId
      types.rs            # KirType (Ty minus opaque variants) + size/layout queries
      span_table.rs       # SpanId interning: span_id -> (file_id, line, col); the one
                          #   line-index implementation (reused from lsp/position logic)
      lower/
        mod.rs            # driver: walks ModuleGraph in dependency order
        decl.rs           # tasks, agents (handler extraction, state layout), impl methods
        stmt.rs           # statements, loops (for -> index loop), declare-vs-shadow
        expr.rs           # expressions; desugars interpolation, `?.`, `??`, `when`
        sugar.rs          # shared rewrite helpers (method-call form, spread flattening)
      mono.rs             # monomorphization: collect instantiations, stamp copies
      passes/
        mod.rs            # fixed-order pass manager
        boxing.rs         # insert Box/Unbox at every dynamic boundary
        rc.rs             # retain/release insertion (single owner of RC policy)
        verify.rs         # KIR well-formedness/type verifier (debug builds + tests)
      dump.rs             # textual form for --emit=kir and golden tests

  keel-codegen/           # NEW — KIR -> LLVM IR -> object -> binary (feature-gated)
    src/
      lib.rs              # compile(KirProgram, BuildOptions) -> PathBuf (binary)
      context.rs          # inkwell Context/Module/Builder setup, TargetMachine, opt levels
      layout.rs           # KirType -> LLVM type: struct layouts, enum tagging,
                          #   nullable pairs, Func {fn*, env*} pairs
      runtime_decl.rs     # every `keel_rt_*` extern declared in ONE place, typed
      func.rs             # function emission: result-ABI prologue/epilogue,
                          #   shadow-stack push/pop, lambda `_boxed` wrappers (§2.7)
      stmt.rs / expr.rs   # KIR walkers emitting instructions
      intrinsics.rs       # arithmetic/comparison/truthiness, interpolation concat chains
      descriptor.rs       # program descriptor as static data (agent/task/handler tables)
      debug.rs            # DIBuilder wiring (see designs/debugging-compiled-keel.md)
      link.rs             # object emission, cc driver, dsymutil, platform differences

  keel-rt-ffi/            # NEW — builds libkeel_rt.a (crate-type staticlib + rlib)
    src/
      lib.rs              # re-exports; the C ABI surface lives here
      host.rs             # CompiledHost: the second `Host` impl (see §2.7)
      abi/
        mod.rs            # #[repr(C)] types: KeelStr, KeelList, KeelMap, KeelSet,
                          #   KeelRes, KeelError, KeelFuncRef
        rc.rs             # keel_retain/keel_release, alloc headers, CoW clone-on-write
        marshal.rs        # ABI value <-> interpreter `Value`, both directions
      ns_dispatch.rs      # keel_rt_call_ns(): the generic namespace entry point
      scheduler.rs        # keel_rt_start(), handler execution contexts, park/unpark
      errors.rs           # KeelError construction + miette report rendering (shared code)
      spans.rs            # runtime side of the span registry (.keel_spans data)
```

Changes to existing code:

- **root crate** — `src/cli`: new `keel build` subcommand (`-o`, `--release`,
  `--emit=kir|llvm-ir|obj`, later `--target`); `src/pipeline.rs` gains
  `compile_to_binary()` beside run/check. Both `#[cfg(feature = "build-backend")]`,
  with an actionable "rebuild with the build-backend feature / install LLVM" error
  otherwise.
- **`src/vm/`** — deleted when KIR lands (superseded; see §2.8).
- **`keel-runtime`** — no structural change. The `Host` trait gets its second
  implementation (`CompiledHost`, living in `keel-rt-ffi`). One contained refactor:
  extract `call_method_on_value` (value methods, `interpreter/methods.rs`) from
  `impl Interpreter` to a `Host`-based free function so compiled code reuses it (§2.7).
- **tests** — `tests/conformance/` (harness described in M0);
  `crates/keel-kir/tests/` golden KIR dumps; `crates/keel-codegen/tests/`
  golden LLVM IR + end-to-end compile-and-run (feature-gated in CI).

Cargo wiring:

```toml
# root crate
[features]
build-backend = ["dep:keel-codegen"]

# keel-kir depends on: keel-syntax, keel-compiler, keel-catalog   (no LLVM)
# keel-codegen depends on: keel-kir, inkwell                      (LLVM here only)
# keel-rt-ffi depends on: keel-runtime                            (no LLVM, ever)
```

### 2.3 KIR — the mid-level IR (the load-bearing design piece)

LLVM IR is too low-level to lower Keel's sugar into directly. KIR is a typed,
explicit, small IR:

- **Desugared**: string interpolation → concat calls; `?.`/`??` → explicit branches;
  `when` → decision tree; `for x in xs` → indexed loop over container ABI; method
  sugar (`xs.map(f)`) → namespace/value-method calls; struct spread → field-wise build.
- **Explicitly typed**: every temp has a `Ty`-derived KIR type; boxing/unboxing at
  `dynamic` boundaries appears as explicit `Box`/`Unbox` instructions.
- **Monomorphized**: generic tasks/types stamped per instantiation before codegen.
- **Error-explicit**: `raise`/`try`/`catch` and the `?`/`??` operators lower to a
  result-shaped calling convention (§2.5). No unwinding.
- **RC-explicit**: `retain`/`release` inserted by a dedicated KIR pass (keeps codegen
  dumb and auditable; enables later elision optimizations in one place).

KIR is **structured (tree-shaped), not SSA/CFG**. Rationale: LLVM's `mem2reg` pass
does the SSA construction for free from `alloca`-based locals; a structured IR keeps
lowering close to the AST (small diff surface against interpreter semantics), keeps
the textual dump human-readable, and avoids building a CFG library nobody else needs.
If a KIR-level optimizer ever becomes worthwhile, that's the moment to revisit.

Data model sketch (illustrative, not final):

```rust
pub struct KirProgram {
    pub functions: Vec<KirFunction>,          // includes lambdas + monomorphized stamps
    pub structs: Vec<StructLayout>,           // nominal + interned anonymous shapes
    pub enums: Vec<EnumLayout>,               // tag values fixed here, not in codegen
    pub lists: Vec<KirType>,                  // ─┐ side tables every composite `KirType`
    pub maps: Vec<KirType>,                   //  │ indexes into — see the interning note
    pub sets: Vec<KirType>,                   //  │ below
    pub nullables: Vec<KirType>,              //  │
    pub tuples: Vec<TupleLayout>,             // ─┘
    pub agents: Vec<AgentDescriptor>,         // state layout, handler FuncIds, @tools,
                                              //   @schedule specs (feeds descriptor.rs)
    pub toplevel: FuncId,                     // compiled top-level statements
    pub span_table: SpanTable,                // SpanId -> (file_id, line, col)
}

pub enum KirType {
    I64, F64, Bool, Unit, Duration,           // unboxed scalars
    Str, List(ListId), Map(MapId), Set(SetId),// opaque RC pointers (container ABI)
    Struct(StructId), Enum(EnumId), Tuple(TupleId),
    Nullable(NullableId), Func(FuncTyId),
    Boxed,                                    // `dynamic` — KeelBox*
    Handle(HandleKind),                       // DbConnection, datetime, Task[T]
}

pub struct KirFunction {
    pub id: FuncId,
    pub name: String,                         // pretty name for diagnostics/DWARF
    pub mangled: String,                      // linkage symbol
    pub params: Vec<(LocalId, KirType)>,
    pub ret: KirType,
    pub can_raise: bool,                      // selects result-ABI vs plain return
    pub locals: Vec<(LocalId, KirType)>,      // all bindings incl. shadowing copies
    pub body: Block,
}

pub enum Stmt {
    Let { local: LocalId, init: Expr },       // declaration (assignment always declares)
    Assign { local: LocalId, value: Expr },   // augmented-assign target resolution done
    If { cond: Expr, then_: Block, else_: Block },
    While { cond: Expr, body: Block },
    ForIndex { .. },                          // all `for` forms, post-desugar
    Match { scrutinee: Expr, arms: DecisionTree },  // `when`, exhaustiveness pre-proven
    TryCatch { body: Block, binder: LocalId, handler: Block },
    Return(Expr), Raise { kind: ErrKind, args: Vec<Expr>, span: SpanId },
    Retain(LocalId), Release(LocalId),        // inserted by passes/rc.rs only
    Expr(Expr),
}

pub enum Expr {
    Const(..), Local(LocalId),
    Call { target: CallTarget, args: Vec<Expr>, span: SpanId },
    BinOp(..), UnOp(..),
    FieldGet { .. }, MakeStruct { .. }, MakeEnum { .. },
    MakeTuple { tuple_id: TupleId, elems: Vec<Expr> },
    TupleGet { base: Box<Expr>, index: usize, ty: KirType },
    Box { value: Box<Expr> },                 // typed -> KeelBox*
    Unbox { value: Box<Expr>, ty: KirType, span: SpanId },  // `as T`, may raise
    NullCheck { .. },                         // from `?.` / `??` desugar
}

pub enum CallTarget {
    Fn(FuncId),                               // direct call, compiled Keel fn
    Rt(RtFn),                                 // typed runtime ABI fn (container ops, rc)
    Ns { ns_id: u16, method_id: u16 },        // generic namespace dispatch (§2.7)
    ValueMethod { method_id: u16 },           // `xs.map`, `s.upper` — dispatch on recv tag
    Indirect(Box<Expr>),                      // lambda value: {fn*, env*} pair
}
```

**Composite types are ID-interned, never inline.** Every composite `KirType`
carries a `usize` index into a side table on `KirProgram` — `List(ListId)`, not
`List(Box<KirType>)`; `Tuple(TupleId)`, not `Tuple(Vec<KirType>)`. This keeps
`KirType` `Copy`, which the implementation relies on pervasively: it is passed
by value through lowering, codegen, and `verify`, and stored in every `Expr`.
An inline `Box`/`Vec` payload would take that away and force a clone at each of
those sites. **Any new composite type must follow the same shape** — add a side
table plus an id, not a nested payload.

Two interning disciplines, and the distinction is semantic rather than
incidental:

- **Nominal**, in declaration order — `StructId`, `EnumId`. Two declarations
  with identical shape stay distinct types, because `type Point {x: int}` and
  `type Score {x: int}` are not interchangeable (issue #16).
- **Structural**, deduplicated by shape — `ListId`, `MapId`, `SetId`,
  `NullableId`, `TupleId`. `(str, int)` written anywhere in the program is one
  id, because `SPEC.md` §2.8 makes tuples structural; same for `list[int]`.

Fixed pass order (each pass re-runs `verify` in debug builds):

```
lower (AST+Ty → KIR)  →  mono  →  boxing  →  rc  →  verify  →  codegen
```

- `lower` consumes the checker's per-expression `Ty` results; it is the only stage
  that sees the AST. All desugaring happens here so later passes see one form.
- `mono` walks reachable code from `toplevel` + agent handlers; unreachable generic
  templates are dropped (natural dead-code elimination).
- `boxing` makes every typed↔dynamic transition an explicit instruction, so codegen
  never guesses and the dump shows exactly where boxing costs sit.
- `rc` is the single owner of retain/release placement (naive but correct policy
  first: retain on bind/field-store, release at scope exit; pair-elision later).

KIR gets a textual dump (`keel build --emit=kir`) for testing lowering in isolation,
and `--emit=llvm-ir` for codegen tests. Both formats are test surface, not stable API.

### 2.4 Value ABI (`keel-rt-ffi`)

All heap values share an RC header; containers and strings are opaque to generated
code — every operation is a runtime call:

```c
// illustrative C view of the ABI
typedef struct KeelStr KeelStr;      // RC'd immutable UTF-8
typedef struct KeelList KeelList;    // RC'd, copy-on-write
typedef struct KeelMap KeelMap;      // keys: str|int|bool (MapKey)
typedef struct KeelBox KeelBox;      // tagged dynamic value (mirror of interpreter Value)

void        keel_retain(void* v);
void        keel_release(void* v);
KeelStr*    keel_str_concat(KeelStr*, KeelStr*);
KeelList*   keel_list_push(KeelList*, KeelBoxOrScalar);   // CoW: clones if rc > 1
int64_t     keel_list_len(KeelList*);
KeelBox*    keel_box_int(int64_t);       // dynamic boundary
KeelRes     keel_unbox_as(KeelBox*, KeelTypeId);          // `as T` — may raise
...
```

**`KeelBox` is literally the interpreter's `Value`** (an `Arc<Value>` behind an opaque
pointer). This is deliberate: `dynamic` semantics (`typeof`, `as` casts, truthiness,
display) are then *guaranteed* identical between engines because they run the same
code (`apply_cast`, `type_name`, `is_truthy` from `interpreter/value.rs` and
`interpreter/expr.rs`), and marshalling at the namespace boundary (§2.7) becomes a
cheap wrap/unwrap instead of a structural conversion.

Structs and enums with statically known layout are **not** opaque — generated code
builds and reads them directly; only their heap allocation/RC goes through the ABI.
Marshalling a static struct into a `Value::Struct` (needed when it crosses into a
namespace call or a `dynamic` position) is a generated per-struct `to_value`/
`from_value` pair emitted as part of the program descriptor.

### 2.5 Errors: result calling convention

Every Keel task that can raise compiles to return `{ i1 is_err, payload }`:
on error the payload is a `KeelError*` (carries `RuntimeErrorKind`-equivalent tag,
message, and source-span id — see the debugging design). `try/catch` is a branch on
`is_err`; `?` propagates; `??` substitutes. Runtime shims return the same shape.
No `setjmp`, no C++/Itanium unwinding — deterministic and debugger-friendly.

### 2.6 Agents, entry point, and the runtime handshake

A compiled program's `main` is provided by `keel-rt-ffi`:

1. `main` → `keel_rt_start(program_descriptor)` — boots tokio, the event loop,
   mailbox scheduler, tracer hooks (same code path the interpreter uses today).
2. The *program descriptor* is generated static data: agent table (name, state layout,
   handler function pointers, `@tools` allowlist, `@schedule` specs), task table,
   test/absence flags — the compiled analogue of the interpreter's `ProgramStore`.
3. Top-level statements compile into `keel_user_toplevel()`, run by the runtime after
   registration, mirroring `Interpreter::execute`.
4. Handlers are synchronous compiled functions (§1.4 strategy 1). The scheduler runs
   each on a dedicated execution context; suspending shims park that context and yield
   control back to tokio. `KEEL_ONESHOT`, `KEEL_TRACE`, `KEEL_EVENT_QUEUE_CAPACITY`
   behave identically because it is the same runtime.

### 2.7 Stdlib reuse: the `Host` trait is the load-bearing seam

This is the most important implementation fact in the design. Namespace methods are
**already decoupled from the interpreter**: every method is a
`BuiltinFn = Fn(&mut dyn Host, Vec<CallArgValue>) -> Future<Result<Value>>`
(`interpreter/state.rs:27`), and the `Host` trait
(`interpreter/host.rs`) was documented from day one as existing so that "alternate
execution backends can be introduced without touching namespace code."

The compiled path cashes that in:

- **`CompiledHost`** (in `keel-rt-ffi/src/host.rs`) is the second `Host`
  implementation. Method-by-method:
  - `runtime()` → the same `Arc<RuntimeContext>` booted by `keel_rt_start`.
  - `call_closure(params, body, args)` → there is no AST in a compiled program;
    compiled lambda values are `KeelFuncRef { fn_ptr, boxed_wrapper }`. The runtime
    always invokes the `boxed_wrapper`, a codegen-emitted `extern "C"` function per
    lambda with uniform signature `fn(*const KeelBox, u32) -> KeelRes` that unboxes
    args to the lambda's real types, calls the typed fn, re-boxes the result.
    (`ScheduledClosure` stores a `KeelFuncRef` instead of `LambdaBody`.)
  - `call_task(name, ..)` → lookup in the program descriptor's task table
    (name → boxed wrapper), same wrapper mechanism.
  - `find_impl_task` → descriptor's impl-method table (type name → method → wrapper).
  - `live_agents`, event sender, mock state → same runtime structures; mocks unused.
- **One generic entry point** instead of 23 hand-written shim files:

  ```c
  KeelRes keel_rt_call_ns(uint16_t ns_id, uint16_t method_id,
                          const KeelBox** args, const char** arg_names,
                          uint32_t nargs, uint32_t span_id);
  ```

  `ns_id`/`method_id` are assigned at KIR-lowering time from `keel-catalog`'s
  authoritative table (the same one the checker and docs already consume — the
  existing catalog↔namespaces cross-check test now also pins these ids).
  The entry marshals `KeelBox` → `Value` (a wrap, per §2.4), builds `CallArgValue`
  (names carried for Keel's named-argument semantics), dispatches through the
  existing `Namespace.methods` registry with a `CompiledHost`, and marshals the
  result back. **All 23 namespaces work in compiled programs with zero per-namespace
  code**, including `@tools` capability enforcement, which lives on the same path.

- **Value methods** (`xs.map`, `s.upper`, … — `interpreter/methods.rs`, ~870 lines)
  get the same treatment via `keel_rt_call_value_method(recv, method_id, ...)`, after
  the one contained refactor noted in §2.2: move `call_method_on_value` from
  `impl Interpreter` onto `&mut dyn Host` (it already operates only on `Value` +
  `CallArgValue`, and needs `Host::call_closure` for the lambda-taking methods).

- **Performance posture**: generic dispatch boxes every argument. That is noise for
  the namespaces that matter (`ai`, `http`, `file`, `db`, `email` are I/O-dominated),
  and measurable only for scalar-pure hot paths (`math.*`, some `str` methods).
  Optimization, deferred to M5 and driven by the catalog table: emit *typed* direct
  shims (`double keel_rt_math_sqrt(double)`) for methods marked pure-scalar, and have
  KIR lowering pick `CallTarget::Rt` instead of `CallTarget::Ns` for those. Same
  semantics, no marshalling; adopted site-by-site with conformance coverage.

**Trade-off acknowledged**: generic dispatch means an async runtime call for every
namespace/value-method invocation (each parks the calling context, §1.4). Container
primitives that codegen uses constantly (`len`, index, push, field access on boxed
values) do NOT go through this path — they are synchronous `CallTarget::Rt` calls
into the container ABI (§2.4) from day one.

### 2.8 Relationship to the VM stub

`src/vm/{bytecode,compiler,machine}.rs` (~100 lines of stubs, explicitly untested per
NON-GOALS.md) is superseded by this design: KIR takes the "compiler IR" role, and the
LLVM backend takes the execution role. Delete the stub when KIR lands, or leave it
until then — but do not grow it.

---

## 3. What is shared between interpreted and compiled mode

Direct answer, because it shapes the whole plan: **everything except the tree-walker
and the new codegen path is shared.** The interpreter is not a parallel product — it
is the reference semantics plus the donor of the runtime, the value model, and the
error machinery.

```
                    ┌────────────────────────────────────────────┐
                    │              SHARED (one copy)             │
                    │  keel-syntax   lexer, parser, AST, lint    │
                    │  keel-compiler checker, HIR, ModuleGraph   │
                    │  keel-catalog  stdlib surface + ids        │
                    ├────────────────────────────────────────────┤
                    │  keel-runtime (runtime half):              │
                    │   RuntimeContext, LLM providers, email,    │
                    │   http, db, scheduler, all 23 namespaces,  │
                    │   @tools enforcement, tracer, miette       │
                    │   error rendering, RuntimeErrorKind        │
                    ├────────────────────────────────────────────┤
                    │  keel-runtime (interpreter types reused):  │
                    │   Value, MapKey, CallArgValue, Host trait, │
                    │   apply_cast, value methods (post-refactor)│
                    └───────────────┬──────────────┬─────────────┘
                                    │              │
                  ┌─────────────────┴───┐      ┌───┴──────────────────────┐
                  │ INTERPRETED ONLY    │      │ COMPILED ONLY            │
                  │ interpreter/ tree-  │      │ keel-kir (lowering)      │
                  │ walker: expr, stmt, │      │ keel-codegen (LLVM)      │
                  │ decl, call, binary  │      │ keel-rt-ffi: ABI types,  │
                  │ Environment,        │      │ marshal, CompiledHost,   │
                  │ testing.mock, REPL, │      │ scheduler shims, span    │
                  │ keel test, DAP dbg  │      │ registry, keel_rt_start  │
                  └─────────────────────┘      └──────────────────────────┘
```

Component by component:

| Component | Interpreted | Compiled | Sharing mechanism |
|---|---|---|---|
| Lexer, parser, AST, formatter, lint (`keel-syntax`, ~9k loc) | ✓ | ✓ | identical crates, identical artifacts |
| Type checker, HIR, module graph (`keel-compiler`, ~11k loc) | ✓ | ✓ | `keel build` runs the same `keel check` first |
| Stdlib catalog (`keel-catalog`) | ✓ | ✓ | also assigns the stable `ns_id`/`method_id` pairs |
| All 23 namespace implementations | ✓ | ✓ | `Host` trait: `Interpreter` vs `CompiledHost` (§2.7) |
| LLM/email/http/db backends, agent scheduler, mailboxes, timers (`RuntimeContext`) | ✓ | ✓ | same crate linked into `libkeel_rt.a`; same env vars, same behavior |
| `@tools` capability checks | ✓ | ✓ | live on the shared namespace-dispatch path |
| `Value` + `MapKey` + casts (`as T`) + truthiness | execution representation | boxed-`dynamic` representation + marshalling hub | `KeelBox` *is* `Arc<Value>` (§2.4) |
| Value methods (`xs.map`, `s.upper`, `methods.rs`) | ✓ | ✓ | after the `impl Interpreter` → `dyn Host` extraction (§2.2) |
| Error taxonomy + miette source-snippet reports | ✓ | ✓ | `keel-rt-ffi/errors.rs` calls the same rendering code |
| Examples, integration tests | ✓ | ✓ | conformance harness runs both engines on the same corpus |
| Tree-walker (`interpreter/{expr,stmt,decl,call,binary}.rs`, `Environment`) | ✓ | — | interpreter-only, permanently |
| `testing.mock`, `keel test`, REPL, DAP step-debugger | ✓ | — | stated non-goals of the compiled path |
| KIR, LLVM codegen, C ABI, `keel_rt_start` scheduler shims | — | ✓ | compiled-only |

In line-count terms: of today's ~39k lines across the three core crates, roughly
~30k (frontend + runtime half + reused interpreter types) serve both engines; the
~6–7k lines of tree-walker remain interpreter-only; the compiled path adds new crates
rather than forking anything. The corollary is the drift policy: **any semantic fix
must land in shared code or in both engines, and the conformance harness (M0) is the
mechanism that catches violations.**

---

## 4. Milestones

Each milestone has a hard exit criterion; do not start the next before it passes.

**M0 — Conformance harness + KIR skeleton.**
Runner that executes every `examples/*.keel` and `tests/` program under interpreter
and (when it exists) compiled mode with `KEEL_LLM=mock`, diffing stdout/exit codes.
KIR type + textual dump for the scalar subset. `keel build` CLI flag behind a cargo
feature. *Exit: harness runs green interpreter-vs-interpreter (infrastructure proven).*

**M1 — Native hello world (scalar subset).**
int/float/bool/str, arithmetic, comparisons, `if/else/while/for` over ranges, top-level
tasks, calls, linking pipeline, `keel_rt_start` handshake, `CompiledHost` skeleton with
generic namespace dispatch proving out on `io.print`/`log.*`.
*Exit: a scalar-only example compiles, links, runs, and matches the interpreter.*

**M2 — Data types + errors.**
Lists/maps/sets via container ABI (CoW semantics verified) — tuples are *not*:
per §1.1 they are by-value LLVM aggregates with no heap allocation and no RC, so
they deliberately bypass the container ABI (issue #157). Plus named + anonymous
structs, enums, `when` exhaustive matching, nullable (`?`, `??`, `?.`), `raise`/`try`/
`catch` result convention, string interpolation. *Exit: the conformance harness runs a
curated M2-scope fixture set (not all non-agent, non-I/O examples — most of that corpus
uses lambdas/value-method dispatch, which is M3 scope) green under both engines,
byte-identical stdout and exit codes, covering structs, enums/`when`, containers,
nullable, string interpolation, and `raise`/`try`/`catch`.*

**M3 — Full stdlib + functions as values.**
Generic namespace dispatch covering all 23 namespaces (`ns_id`/`method_id` pinned in
the catalog, `@tools` path verified); the `call_method_on_value` → `Host` extraction
and `keel_rt_call_value_method`; lambdas as `KeelFuncRef` + `_boxed` wrappers; generics
monomorphization; `dynamic` boxing + `as` casts; `json`/`csv`/`file`/`http` round-trips.
*Exit: conformance green on every example not using agents/schedule.*

**M4 — Agents and concurrency.**
Program descriptor, mailboxes, `run`/`stop`/`send`/`delegate`/`broadcast`, `on`
handlers, agent `state`, `@schedule`/`schedule.*`, `async.spawn`/`join_all`/`select`,
`http.serve`. *Exit: full conformance suite green; `KEEL_ONESHOT` examples behave
identically.*

**M5 — Productionization.**
`--release` (LLVM opt pipeline O2), macOS arm64 + Linux x86_64 CI, binary-size pass
(strip, LTO on libkeel_rt), typed fast-path shims for pure-scalar catalog methods
(§2.7), docs (`docs/src/cli/build.md`), SPEC §22 update.
Debug info lands here → see `designs/debugging-compiled-keel.md`.
*Exit: `keel build` documented and shipped behind a `--experimental` flag.*

Deliberately deferred: stackful-coroutine scheduler (threads suffice first), CPS/LLVM-
coroutine codegen, cross-compilation, Windows, RC-elision optimization, `.keelc`.

---

## 5. Risks

| Risk | Mitigation |
|---|---|
| Semantic drift between engines | M0 conformance harness is a merge gate for every later milestone; interpreter remains the reference semantics |
| LLVM as a build dependency burdens contributors | backend behind a cargo feature; only `keel build` needs LLVM |
| inkwell/LLVM version churn across platforms | pin one LLVM major per release; CI installs from official binaries |
| Value-semantics bugs (CoW aliasing) | container ABI unit tests + differential fuzzing of container ops vs interpreter |
| Thread-per-handler scaling ceiling | acceptable for agent counts Keel targets; coroutine scheduler is a swap-in behind the same park/unpark ABI |
| Effort underestimate | milestones are independently shippable; M1–M2 alone validate the stack before the long tail |

---

## 6. Open questions (decide before the affected milestone)

1. **Closure capture** (before M3): today lambdas capture nothing. Codify that in
   SPEC (compile plain fn pointers), or introduce capture-by-value first (needs the
   `env` slot + closure conversion in KIR). Run `design-lang` before changing surface
   semantics.
2. **Anonymous struct interning** (before M2): shape-name mangling scheme for
   anonymous structs so identical shapes unify across modules.
3. **`datetime` representation** (before M2): fix a canonical runtime layout (epoch
   nanos + tz?) rather than inheriting the interpreter's ad-hoc form.
4. **`keel test` on compiled code** (post-M5, likely never): mocking requires a
   dispatch-table indirection on every namespace call; recommend tests stay interpreted.
5. **Module compilation granularity** (before M4): whole-program compilation first
   (matches `ModuleGraph`); per-module object caching is a later optimization.
