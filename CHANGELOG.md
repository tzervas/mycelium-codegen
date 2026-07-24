# Changelog — mycelium-codegen

## Unreleased

### B1 — Match in pre-tail of a pure-tail `Fix` (direct-LLVM)

Closes the native AOT gap where recursive programs using a nested `Match` to
compute the next tail step were hard-refused on the iterative loop path
(`crates/mycelium-mlir/src/llvm.rs`).

**What now lowers**

- A `Match` in a pure-tail `Fix` arm's pre-tail binding sequence (DN-15 §8.5)
  is lowered via the shared `lower_match`, kept on the **iterative tail loop**
  (not forced onto the heap trampoline).
- Dedicated **back-edge blocks** feed the loop header phi, so Match-introduced
  basic blocks never desync phi predecessors.
- Match arm/default merges use **join blocks** so nested Match in an arm body
  is also phi-safe.

**Tests**

- `tests/recursion_b1.rs` — factorial / ackermann / list-fold *style* programs
  (Match-driven Binary{8} counters): three-way interp ≡ env-machine ≡ native
  when `llc`/`clang` are present; emission always gated.
- `tests/recursion_differential.rs` — former refuse of step-via-Match is now
  an agree test; B2 residual pin for `FixGroup` in Fix-arm bindings.
- `tests/recursion_trampoline_differential.rs` — pure-tail Match-in-pre-tail
  stays on the tail loop (no `@myc_tramp_alloc`).
- Unit: `is_pure_tail_single_fix` classifies Match-in-pre-tail as pure tail.

**B2 residual (honest refuse)**

- `FixGroup` in a tail-Fix arm binding sequence remains
  `AotError::UnsupportedNode` with an explicit Wave-B2 residual message.
  Covered by hard-refuse tests; never silently miscompiled.

**Docs**

- Stale “Match in pre-tail is deferred / refused” wording updated in
  `llvm.rs`, `trampoline.rs`, and the recursion differential module docs.
