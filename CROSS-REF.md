# CROSS-REF — mycelium-codegen

Mycelium-internal dependencies only (steer handoff §6.1; external crates stay in Cargo
metadata). Pinned revs are the fixed (buildable) tips recorded by the Phase-B wave;
content hash = git tree hash of the pinned rev.

| Interface consumed | Repo | Pinned rev | Content hash | Notes |
|---|---|---|---|---|
| mycelium-cert | https://github.com/tzervas/mycelium-runtime | `ab9cee665b620ed80ab74ea61ea639817dc49077` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-cert` (see monorepo `docs/api-index/INDEX.md#mycelium-cert`) |
| mycelium-core | https://github.com/tzervas/mycelium-core | `781d3fcceba82acfe6b0eb46650513bd78a2416b` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-core` (see monorepo `docs/api-index/INDEX.md#mycelium-core`) |
| mycelium-dense | https://github.com/tzervas/mycelium-value | `fce92daed05e9f10202c202648ec43fb0a6991d7` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-dense` (see monorepo `docs/api-index/INDEX.md#mycelium-dense`) |
| mycelium-interp | https://github.com/tzervas/mycelium-runtime | `ab9cee665b620ed80ab74ea61ea639817dc49077` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-interp` (see monorepo `docs/api-index/INDEX.md#mycelium-interp`) |
| mycelium-numerics | https://github.com/tzervas/mycelium-value | `fce92daed05e9f10202c202648ec43fb0a6991d7` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-numerics` (see monorepo `docs/api-index/INDEX.md#mycelium-numerics`) |
| mycelium-rt-abi | https://github.com/tzervas/mycelium-runtime | `ab9cee665b620ed80ab74ea61ea639817dc49077` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-rt-abi` (see monorepo `docs/api-index/INDEX.md#mycelium-rt-abi`) |
| mycelium-sched | https://github.com/tzervas/mycelium-runtime | `ab9cee665b620ed80ab74ea61ea639817dc49077` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-sched` (see monorepo `docs/api-index/INDEX.md#mycelium-sched`) |
| mycelium-select | https://github.com/tzervas/mycelium-runtime | `ab9cee665b620ed80ab74ea61ea639817dc49077` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-select` (see monorepo `docs/api-index/INDEX.md#mycelium-select`) |
| mycelium-vsa | https://github.com/tzervas/mycelium-value | `fce92daed05e9f10202c202648ec43fb0a6991d7` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-vsa` (see monorepo `docs/api-index/INDEX.md#mycelium-vsa`) |
| mycelium-workstack | https://github.com/tzervas/mycelium-core | `781d3fcceba82acfe6b0eb46650513bd78a2416b` | tree `(tree hash: fetch dep rev locally to resolve)` | Rust API of `mycelium-workstack` (see monorepo `docs/api-index/INDEX.md#mycelium-workstack`) |

**Owning docs:** ADR-007 (MLIR→LLVM AOT path).
**Source provenance:** extracted from `tzervas/mycelium` archive `aad96b7a…`; fixed by
the course-correction Phase B (workspace root, git pins, toolchain + supply-chain
replicas, CI v2). Full program record: monorepo
`docs/planning/course-correction-2026-07-18/PROGRAM.md`.
