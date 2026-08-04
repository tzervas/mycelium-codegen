# AOT capability matrix (S-AOT-CAPABILITY-CORPUS)

**Source of truth:** `crates/mycelium-mlir/tests/capability_corpus.rs`. This table is a snapshot of
what that file measured on the merge commit noted below, on a runner with `llc`/`clang` 18 present.
It is **not** hand-asserted — every row is a literal `Result` variant a `#[test]` in that file pins.
Regenerate this table (by hand, until a script exists — see "How to regenerate" below) whenever
`capability_corpus.rs` changes; do not let it drift from the test file's actual assertions.

Package: [PKG-WP9-AOT](https://github.com/tzervas/mycelium-lang/issues/47) ·
Surfaces: `S-AOT-NATIVE-COMPILE`, `S-AOT-CAPABILITY-CORPUS` ·
Measured against: mycelium-codegen `main` @ `08ae6f4` (codegen#19, B1/B2, merged 2026-08-04).

## Lowers (`Ok`)

| Category | API | Fixture | Result |
|---|---|---|---|
| Scalar bit ops (`Binary{w}`: `core.id`, `bit.not/and/or/xor`) | `llvm::compile_and_run` | `scalar_bit_ops_lower` | `Ok` |
| Scalar trit ops (`Ternary{m}`: `trit.neg/add/sub/mul`) | `llvm::compile_and_run` | `scalar_trit_ops_lower` | `Ok` |
| Non-recursive `Construct`/`Match` (tagged stack struct, M-373 Increment-1) | `llvm::compile_and_run` | `non_recursive_construct_match_lowers` | `Ok` |
| Tail-position `Fix` (B1 — Match-driven step, iterative tail loop) | `llvm::compile_and_run` | `tail_position_fix_lowers` | `Ok` |
| `FixGroup` applied inside a pure-tail `Fix` arm (B2, heap trampoline, linear-defunctionalized shape) — **gated on codegen#19**, merged 08ae6f4 (2026-08-04T13:19:26Z) | `llvm::compile_and_run` | `fixgroup_in_tail_fix_arm_lowers` | `Ok` |
| Un-quantized F32/BF16 Dense element-wise (`dense.add/sub/neg/scale/dot/similarity`, M-853) — separate API/error type from the row above | `dense_codegen::dense_compile_and_run` | `dense_unquantized_element_wise_lowers` | `Ok(DenseResult::Value)` |
| Closures over any `Binary{w}`/`Ternary{m}` width, curried application, **applied** (specialize-at-application / inlining, M-851) — measured; note this is a *wider* lowering surface than "closures stay refused" | `llvm::compile_and_run` | not in this file — see `closure_widening_differential.rs` (pre-existing, not duplicated here) | `Ok` |

## Refused (never silent — G2)

| Category | API | Fixture | Result |
|---|---|---|---|
| A closure-valued **program result** (bare `Lam`, or a partially-applied curry) — not printable by the read-back protocol (DN-15 §7.4) | `llvm::compile_and_run` | `refused_closure_valued_program_result` | `Err(AotError::UnsupportedNode)` |
| A `Construct` in a non-tail `Fix` trampoline's pre-call sequence — this corpus's measured stand-in for "wide/heap ADT beyond the non-recursive stack-alloca fragment"; the trampoline pre-call is restricted to straight-line `Binary{8}` const/alias/op only (Wave-B2 residual) | `llvm::emit_llvm_ir` | `refused_construct_in_trampoline_precall` | `Err(AotError::UnsupportedNode)` naming `Construct` |
| `Repr::Vsa` reaching the **generic** bit/trit `Node` path (VSA has its own dedicated `vsa_codegen` API, not exercised by this row) | `llvm::emit_llvm_ir` | `refused_vsa_repr_on_generic_node_path` | `Err(AotError::UnsupportedRepr)` naming `Vsa` |
| wild/host-effect calls — **proxy fixture, documented as such**: `mycelium_core::Node` has zero wild/host-effect-specific variant (verified — no `HostCall`/`Wild` node; zero `wild`/`HostCall` token in `llvm.rs`/`trampoline.rs`), so there is no Core-IR shape to construct directly. The fixture instead feeds an unrecognized host-effect-shaped `Op` prim, which falls through to the same catch-all refusal any unknown primitive hits | `llvm::emit_llvm_ir` | `refused_wild_host_effect_proxy_unrecognized_prim` | `Err(AotError::UnsupportedPrim)` |

## Known gap — left OPEN by design (codegen#8, non-goal of this package)

| Category | API | Fixture | Result |
|---|---|---|---|
| `classify_arm`'s stray-self scan (tail-Fix pre-tail check) omits the member-name shadowing guard `anf_refs_name` applies elsewhere for `Rhs::FixGroup` — a group member literally named the same as the outer `Fix`'s self-name can be **false-refused** even though the program's only self-reference is the legitimate tail call. **This fixture reproduces the false-refuse described in codegen#8, measured, not merely quoted from the issue.** | `llvm::emit_llvm_ir` | `known_gap_codegen_8_fixgroup_self_name_shadow` | `Err(AotError::UnsupportedNode("non-tail self-reference…"))` — the false-refuse itself, pinned so a future accidental change is visible, never silent |

## Documented unknown — NOT asserted (not a row that can be measured today)

- **Quantized Dense** (`DenseAotError::QuantRefused`): declared and documented in `dense_codegen.rs`,
  but **measured**: nothing in the current value model constructs a quantized `Repr::Dense` value (the
  ADR-030 `QuantDesc` descriptor E20-1 would add does not exist yet), so `QuantRefused` is currently
  dead code — unreachable from any fixture. Left as an explicit gap, not asserted either way, per
  "prefer unknown/needs-human over false closure." Re-visit once E20-1 lands a constructible value.

## Non-goals of this package (verified still refused, not attempted here)

- General recursive/heap ADTs + strings beyond the non-recursive stack-alloca fragment.
- Repr::Vsa as a general native-lowering target (only its own dedicated `vsa_codegen` API exists;
  never through the generic `Node` path — see the refusal row above).
- `wild`/host-effect codegen (zero representation anywhere in this crate today).

## How to regenerate

```
cargo test -p mycelium-mlir --test capability_corpus -- --nocapture
```

Every row above is one `#[test]` in that file; if you change a fixture's expected variant, update
this table in the same commit. There is no automated doc-generation script in this crate yet (no
existing convention to model it on was found in this repo) — this is a hand-transcribed snapshot of
measured `cargo test` output, not a build artifact. That is a known, documented gap versus the
S-AOT-CAPABILITY-CORPUS surface's "generated" framing — flagged, not silently narrowed.
