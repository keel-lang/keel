# Plan: Operator Type Compatibility

## What it does

Adds a `check_binop` function to the type checker that emits an error when
operand types are incompatible. Currently `infer_binary` silently returns
`Ty::Unknown` on bad combos — this makes them type errors instead.

```
"x" + 5      # passes today, fails at runtime → will be a check-time error
"x" < 5      # same
true + 1     # same
x += "s"     # same (augmented assignment)
```

---

## File 1 — `src/types/checker.rs`

### 1a. Add `op_symbol` helper (after `infer_binary` ~line 2126)

```rust
fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Lt  => "<",
        BinOp::Gt  => ">",
        BinOp::Lte => "<=",
        BinOp::Gte => ">=",
        BinOp::Eq  => "==",
        BinOp::Neq => "!=",
        BinOp::And => "and",
        BinOp::Or  => "or",
    }
}
```

### 1b. Add `check_binop` (immediately after `op_symbol`)

Returns `None` if valid, `Some(error_message)` if not.

```rust
fn check_binop(op: BinOp, l: &Ty, r: &Ty) -> Option<String> {
    let lb = l.strip_nullable();
    let rb = r.strip_nullable();

    // Unknown/Dynamic on either side = gradual typing escape hatch
    if matches!(lb, Ty::Unknown | Ty::Dynamic)
        || matches!(rb, Ty::Unknown | Ty::Dynamic)
    {
        return None;
    }

    let ok = match op {
        BinOp::Add => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
                | (Ty::Str, Ty::Str)
        ) || matches!((lb, rb), (Ty::List(_), Ty::List(_))),

        BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
        ),

        BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
                | (Ty::Str, Ty::Str)
        ),

        // Equality and boolean ops are always valid
        BinOp::Eq | BinOp::Neq | BinOp::And | BinOp::Or => true,
    };

    if ok {
        None
    } else {
        Some(format!(
            "cannot apply `{}` to {} and {}",
            op_symbol(op),
            describe_ty(lb),
            describe_ty(rb)
        ))
    }
}
```

### 1c. Wire into `Expr::BinaryOp` (line ~1205)

```rust
Expr::BinaryOp { left, op, right } => {
    let l = self.infer_expr(left, scope);
    let r = self.infer_expr(right, scope);
    if let Some(msg) = check_binop(*op, &l, &r) {
        self.err(msg);
    }
    infer_binary(*op, &l, &r)
}
```

### 1d. Wire into `Stmt::AugAssign` (line ~839)

```rust
Stmt::AugAssign { name, op, rhs } => {
    let var_ty = scope.get(name)
        .cloned()
        .unwrap_or_else(|| {
            self.err(format!(
                "augmented assignment to undefined variable `{name}`"
            ));
            Ty::Unknown
        });
    let rhs_ty = self.infer_expr(rhs, scope);
    if let Some(msg) = check_binop(*op, &var_ty, &rhs_ty) {
        self.err(msg);
    }
}
```

---

## File 2 — `tests/type_checker_tests.rs`

Use the existing `expect_error` and `type_ok` helpers.

### Invalid combos — must error

```rust
#[test]
fn binop_str_plus_int_is_error() {
    expect_error(
        r#"agent A { @on_start { x = "hi" + 5 } } run(A)"#,
        "cannot apply `+`",
    );
}

#[test]
fn binop_str_minus_int_is_error() {
    expect_error(
        r#"agent A { @on_start { x = "hi" - 1 } } run(A)"#,
        "cannot apply `-`",
    );
}

#[test]
fn binop_str_lt_int_is_error() {
    expect_error(
        r#"agent A { @on_start { x = "hi" < 5 } } run(A)"#,
        "cannot apply `<`",
    );
}

#[test]
fn binop_bool_plus_int_is_error() {
    expect_error(
        r#"agent A { @on_start { x = true + 1 } } run(A)"#,
        "cannot apply `+`",
    );
}

#[test]
fn binop_list_minus_int_is_error() {
    expect_error(
        r#"agent A { @on_start { x = [1, 2] - 1 } } run(A)"#,
        "cannot apply `-`",
    );
}

#[test]
fn aug_assign_type_mismatch_is_error() {
    expect_error(
        r#"agent A { @on_start { x = 0
                                 x += "oops" } } run(A)"#,
        "cannot apply `+`",
    );
}
```

### Valid combos — must not error

```rust
#[test]
fn binop_valid_numeric_combos() {
    type_ok(r#"agent A { @on_start { a = 1 + 1
                                      b = 1.0 + 2
                                      c = 1 + 2.0
                                      d = 3.0 - 1.0 } } run(A)"#);
}

#[test]
fn binop_valid_str_concat() {
    type_ok(r#"agent A { @on_start { x = "a" + "b" } } run(A)"#);
}

#[test]
fn binop_valid_list_concat() {
    type_ok(r#"agent A { @on_start { x = [1] + [2] } } run(A)"#);
}

#[test]
fn binop_valid_comparisons() {
    type_ok(r#"agent A { @on_start { a = 1 < 2
                                      b = "a" < "b"
                                      c = 1.0 >= 0 } } run(A)"#);
}

#[test]
fn binop_equality_is_always_valid() {
    type_ok(r#"agent A { @on_start { x = 1 == "hello" } } run(A)"#);
}

#[test]
fn binop_unknown_operand_skips_check() {
    // Ai.prompt returns Unknown — should not trigger a type error
    type_ok(r#"
agent A {
    @on_start {
        v = Ai.prompt("say hi")
        x = v + 1
    }
}
run(A)
"#);
}
```

---

## File 3 — `docs/src/guide/expressions.md`

Add a **"Type rules"** subsection after `## Boolean logic` (before `## Null coalescing`):

```markdown
## Type rules for operators

The type checker validates operand types at `keel check` time.

| Operator | Valid operand types |
|---|---|
| `+` | `int`, `float` (mixed ok), `str + str`, `list + list` |
| `-` `*` `/` `%` | `int`, `float` (mixed ok) |
| `<` `>` `<=` `>=` | `int`, `float` (mixed ok), `str + str` |
| `==` `!=` | any |
| `and` `or` | any |
| `+=` `-=` `*=` `/=` `%=` | same rules as the base operator |

`unknown` / `dynamic` values skip the check (gradual typing escape hatch).

Type mismatches are caught early:

    x = "hello" + 5     # error: cannot apply `+` to str and int
    x = "hello" < 42    # error: cannot apply `<` to str and int
    x = 0
    x += "oops"         # error: cannot apply `+` to int and str
```

---

## File 4 — `SPEC.md`

In the type checker feature list (the prose block that mentions "scope, arity, enum
exhaustiveness, nullable safety ..."), append:

> `check_binop` validates that arithmetic and comparison operands are
> type-compatible; `Unknown`/`Dynamic` operands are always accepted (gradual
> typing escape hatch). Augmented assignment (`+=`, `-=`, etc.) is checked with
> the same rules.

---

## File 5 — `CHANGELOG.md`

Under `[Unreleased]`, add a `### Fixed` section (or append to the existing one):

```markdown
### Fixed

- Type checker: invalid operator combinations (`"x" + 5`, `"x" < 5`, `true + 1`,
  etc.) are now caught at `keel check` time instead of failing at runtime.
  Augmented assignment (`+=`, `-=`, etc.) is checked with the same rules.
  `unknown`/`dynamic` operands are always accepted (gradual typing escape hatch).
```

---

## File 6 — `ROADMAP.md`

Flip the status row from `[ ]` to `[x]`:

```
| Type checker: operator type compatibility | [x] | ...
```

---

## Order of work

1. Add `op_symbol` + `check_binop` to `checker.rs`
2. Wire into `Expr::BinaryOp`
3. Wire into `Stmt::AugAssign`
4. `cargo test type_checker` — existing tests must still pass
5. Add new tests in `type_checker_tests.rs`, run `cargo test type_checker`
6. Update `expressions.md`, `SPEC.md`, `CHANGELOG.md`, `ROADMAP.md`
7. `mdbook build` — must be clean
8. Commit
