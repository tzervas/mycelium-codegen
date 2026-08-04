//! S-AOT-CAPABILITY-CORPUS (PKG-WP9-AOT, hub tzervas/mycelium-lang#47) — measured, not asserted.
//!
//! One `#[test]` per construct category, each calling `mycelium-codegen`'s own public API directly
//! (`llvm::{compile, compile_and_run, emit_llvm_ir, AotError}` for the generic bit/trit + data path;
//! `dense_codegen::{dense_compile_and_run, DenseAotError}` for the dedicated Dense path) and
//! asserting the **literal** `Result` variant it returns *today* — never a bare `.is_err()`/`.is_ok()`.
//! This is the only place "which constructs lower vs refuse, and at which layer" is measured before
//! `myc build --native` exists at all (the CLI lane is a different repo/package deliverable).
//!
//! Grounded in fixtures already proven in this crate's own differential suite
//! (`recursion_b1.rs`/`recursion_b2.rs`/`native_differential.rs`/`closure_widening_differential.rs`/
//! `vsa_differential.rs`/`dense_differential.rs`) — this file does not re-derive lowering behavior,
//! it packages representative instances of each and pins the exact error/success shape as a
//! regression gate for S-AOT-CAPABILITY-CORPUS.
//!
//! Toolchain-dependent fixtures skip gracefully on `AotError::ToolchainMissing` /
//! `DenseAotError::ToolchainMissing` (the house idiom) — measured on a runner with `llc`+`clang`
//! present in this session, but must not hard-fail where they are absent.

use mycelium_core::{Alt, Meta, Node, Payload, Provenance, Repr, ScalarKind, SparsityClass, Value};
use mycelium_mlir::{
    dense_compile_and_run, AotError, DenseAotError, DenseCgOp, DenseProgram, DenseResult,
};

// ─── shared value/node builders (mirrors the house style in the sibling differential tests) ───────

fn byte_n(n: u8) -> Value {
    let bits: Vec<bool> = (0..8).map(|i| (n >> i) & 1 == 1).collect();
    Value::new(
        Repr::Binary { width: 8 },
        Payload::Bits(bits),
        Meta::exact(Provenance::Root),
    )
    .expect("8-bit value")
}

fn trit3(a: mycelium_core::Trit, b: mycelium_core::Trit, c: mycelium_core::Trit) -> Value {
    Value::new(
        Repr::Ternary { trits: 3 },
        Payload::Trits(vec![a, b, c]),
        Meta::exact(Provenance::Root),
    )
    .expect("3-trit value")
}

fn var(x: &str) -> Node {
    Node::Var(x.to_owned())
}

/// Skip a fixture gracefully when the native toolchain (llc/clang) is absent — the house idiom
/// (never a false CI failure on a runner without the tools). Returns `true` if the caller should
/// bail out early.
macro_rules! skip_if_toolchain_missing {
    ($result:expr, $label:expr) => {
        if let Err(AotError::ToolchainMissing(tool)) = &$result {
            eprintln!("{}: skipping — {tool} not installed (house idiom)", $label);
            return;
        }
    };
}

// ─── Category 1 (Ok): scalar bit ops — Binary{w} core.id / bit.not/and/or/xor ──────────────────────

#[test]
fn scalar_bit_ops_lower() {
    let prog = Node::Op {
        prim: "bit.xor".into(),
        args: vec![
            Node::Op {
                prim: "bit.not".into(),
                args: vec![Node::Const(byte_n(0b0000_1111))],
            },
            Node::Const(byte_n(0b1010_1010)),
        ],
    };
    let result = mycelium_mlir::compile_and_run(&prog);
    skip_if_toolchain_missing!(result, "scalar_bit_ops_lower");
    match result {
        Ok(v) => assert_eq!(
            v.payload(),
            &Payload::Bits({
                // !0b0000_1111 = 0b1111_0000; ^ 0b1010_1010 = 0b0101_1010
                let n = 0b0101_1010u8;
                (0..8).map(|i| (n >> i) & 1 == 1).collect()
            }),
            "scalar bit-op chain must compute the expected byte"
        ),
        other => panic!("scalar bit ops must lower to Ok, got {other:?}"),
    }
}

// ─── Category 2 (Ok): scalar trit ops — Ternary{m} trit.neg/add/sub/mul ────────────────────────────

#[test]
fn scalar_trit_ops_lower() {
    use mycelium_core::Trit;
    let prog = Node::Op {
        prim: "trit.neg".into(),
        args: vec![Node::Const(trit3(Trit::Pos, Trit::Zero, Trit::Neg))],
    };
    let result = mycelium_mlir::compile_and_run(&prog);
    skip_if_toolchain_missing!(result, "scalar_trit_ops_lower");
    match result {
        Ok(v) => assert_eq!(
            v.payload(),
            &Payload::Trits(vec![Trit::Neg, Trit::Zero, Trit::Pos]),
            "trit.neg must flip each trit's sign"
        ),
        other => panic!("scalar trit ops must lower to Ok, got {other:?}"),
    }
}

// ─── Category 3 (Ok): non-recursive Construct/Match over a tagged stack struct (M-373 Increment-1) ─

#[test]
fn non_recursive_construct_match_lowers() {
    use mycelium_core::data::{CtorSpec, DataRegistry, DeclSpec, FieldSpec};
    use std::collections::BTreeMap;

    let mut specs = BTreeMap::new();
    specs.insert(
        "Box".to_owned(),
        DeclSpec {
            ctors: vec![CtorSpec {
                fields: vec![FieldSpec::Repr(Repr::Binary { width: 8 })],
            }],
        },
    );
    let reg = DataRegistry::build(&specs).expect("Box registry must build");
    let ctor = reg.ctor_ref("Box", 0).unwrap();

    // Construct Box(0xAA), match to extract + bit.not the field.
    let prog = Node::Match {
        scrutinee: Box::new(Node::Construct {
            ctor: ctor.clone(),
            args: vec![Node::Const(byte_n(0xAA))],
        }),
        alts: vec![Alt::Ctor {
            ctor,
            binders: vec!["b".to_owned()],
            body: Node::Op {
                prim: "bit.not".into(),
                args: vec![var("b")],
            },
        }],
        default: None,
    };
    let result = mycelium_mlir::compile_and_run(&prog);
    skip_if_toolchain_missing!(result, "non_recursive_construct_match_lowers");
    match result {
        Ok(v) => assert_eq!(
            v.payload(),
            &Payload::Bits({
                let n = !0xAAu8;
                (0..8).map(|i| (n >> i) & 1 == 1).collect()
            })
        ),
        other => panic!("non-recursive Construct/Match must lower to Ok, got {other:?}"),
    }
}

// ─── Category 4 (Ok): tail-position Fix (B1 — Match-driven countdown on the iterative tail loop) ───

#[test]
fn tail_position_fix_lowers() {
    // f = Fix(self, λn. Match n { 0 → 0xAA ; _ → App(self, 0) }) applied to 1: one tail step, base.
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(var("n")),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                body: Node::Const(byte_n(0xAA)),
            }],
            default: Some(Box::new(Node::App {
                func: Box::new(var("self")),
                arg: Box::new(Node::Const(byte_n(0))),
            })),
        }),
    };
    let prog = Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(1))),
    };
    let result = mycelium_mlir::compile_and_run(&prog);
    skip_if_toolchain_missing!(result, "tail_position_fix_lowers");
    match result {
        Ok(v) => assert_eq!(
            v.payload(),
            &Payload::Bits((0..8).map(|i| (0xAAu8 >> i) & 1 == 1).collect())
        ),
        other => panic!("tail-position Fix must lower to Ok, got {other:?}"),
    }
}

// ─── Category 5 (Ok): heap-trampoline FixGroup applied inside a pure-tail Fix arm (B2 — gate #19) ──
//
// This row's expected result changed with codegen#19 (B1/B2 promotion to `main`, merged
// 2026-08-04T13:19:26Z, merge commit 08ae6f4a4c3c58656c992c2fd7a0826ba3485538) — before that merge
// this shape was refused; after it, it lowers via the shared heap trampoline (M-850).

#[test]
fn fixgroup_in_tail_fix_arm_lowers() {
    // Mutual pair e/o applied as the BASE of an outer pure-tail Fix (mirrors
    // recursion_b2.rs::mutual_fixgroup_applied_as_base, trimmed to the essential shape):
    //   e(x) = Match x { 0 → 0xAA ; _ → o(0) }
    //   o(x) = Match x { 0 → bit.not(e(0)) ; _ → e(0) }
    //   outer = Fix(self, λn. Match n { 0 → FixGroup{e,o}(e(1)) ; _ → App(self, 0) })
    //   App(outer, 1)   // e(1) -> o(0) -> not(e(0)) = not(0xAA) = 0x55
    let even = (
        "e".to_string(),
        Box::new(Node::Lam {
            param: "x".into(),
            body: Box::new(Node::Match {
                scrutinee: Box::new(var("x")),
                alts: vec![Alt::Lit {
                    value: byte_n(0),
                    body: Node::Const(byte_n(0xAA)),
                }],
                default: Some(Box::new(Node::App {
                    func: Box::new(var("o")),
                    arg: Box::new(Node::Const(byte_n(0))),
                })),
            }),
        }),
    );
    let odd = (
        "o".to_string(),
        Box::new(Node::Lam {
            param: "x".into(),
            body: Box::new(Node::Match {
                scrutinee: Box::new(var("x")),
                alts: vec![Alt::Lit {
                    value: byte_n(0),
                    body: Node::Op {
                        prim: "bit.not".into(),
                        args: vec![Node::App {
                            func: Box::new(var("e")),
                            arg: Box::new(Node::Const(byte_n(0))),
                        }],
                    },
                }],
                default: Some(Box::new(Node::App {
                    func: Box::new(var("e")),
                    arg: Box::new(Node::Const(byte_n(0))),
                })),
            }),
        }),
    );
    let group = Node::FixGroup {
        defs: vec![even, odd],
        body: Box::new(Node::App {
            func: Box::new(var("e")),
            arg: Box::new(Node::Const(byte_n(1))),
        }),
    };
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(var("n")),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                body: group,
            }],
            default: Some(Box::new(Node::App {
                func: Box::new(var("self")),
                arg: Box::new(Node::Const(byte_n(0))),
            })),
        }),
    };
    let prog = Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(1))),
    };
    let result = mycelium_mlir::compile_and_run(&prog);
    skip_if_toolchain_missing!(result, "fixgroup_in_tail_fix_arm_lowers");
    match result {
        Ok(v) => assert_eq!(
            v.payload(),
            &Payload::Bits((0..8).map(|i| (!0xAAu8 >> i) & 1 == 1).collect()),
            "e(1) -> o(0) -> not(e(0)) = not(0xAA) = 0x55"
        ),
        other => panic!(
            "FixGroup applied inside a pure-tail Fix arm must lower to Ok post-#19, got {other:?}"
        ),
    }
}

// ─── Category 6 (refused): a closure-valued PROGRAM RESULT (general Lam/App outside the trampoline
// shape). NOTE — measured, not the stale claim: since M-851 (closure-ABI widening), an applied
// `App(Lam{..}, arg)` DOES lower (specialize-at-application/inlining; see
// closure_widening_differential.rs). The permanent refusal boundary is narrower than "any Lam/App":
// only a closure escaping as the program's *final result* (unapplied, or a partially-applied curry)
// is refused, because a closure is not printable by the read-back protocol (DN-15 §7.4). ──────────

#[test]
fn refused_closure_valued_program_result() {
    let bare_lam = Node::Lam {
        param: "x".into(),
        body: Box::new(var("x")),
    };
    match mycelium_mlir::compile_and_run(&bare_lam) {
        Err(AotError::UnsupportedNode(_)) => { /* expected */ }
        Err(AotError::ToolchainMissing(tool)) => {
            eprintln!("refused_closure_valued_program_result: skipping — {tool} not installed");
        }
        other => panic!(
            "a closure-valued program result must be UnsupportedNode (DN-15 §7.4), got {other:?}"
        ),
    }
}

// ─── Category 7 (refused): a Construct bound in a non-tail Fix trampoline's pre-call sequence —
// the corpus's proxy for "wide/heap general ADTs beyond the non-recursive stack-alloca fragment":
// the trampoline pre-call sequence is restricted to straight-line Binary{8} const/alias/op only
// (Wave-B2 residual), so any data construction there is an honest refuse naming "Construct". ──────

#[test]
fn refused_construct_in_trampoline_precall() {
    use mycelium_core::data::{CtorSpec, DataRegistry, DeclSpec, FieldSpec};
    use std::collections::BTreeMap;

    let mut specs = BTreeMap::new();
    specs.insert(
        "Box".to_owned(),
        DeclSpec {
            ctors: vec![CtorSpec {
                fields: vec![FieldSpec::Repr(Repr::Binary { width: 8 })],
            }],
        },
    );
    let reg = DataRegistry::build(&specs).unwrap();
    let ctor = reg.ctor_ref("Box", 0).unwrap();

    // f = Fix(self, λn. Match n { 0 → 0xAA ; _ → bit.not(let _ = Box(0) in App(self, 0)) })
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(var("n")),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                body: Node::Const(byte_n(0xAA)),
            }],
            default: Some(Box::new(Node::Op {
                prim: "bit.not".into(),
                args: vec![Node::Let {
                    id: "boxed".into(),
                    bound: Box::new(Node::Construct {
                        ctor,
                        args: vec![Node::Const(byte_n(0))],
                    }),
                    body: Box::new(Node::App {
                        func: Box::new(var("self")),
                        arg: Box::new(Node::Const(byte_n(0))),
                    }),
                }],
            })),
        }),
    };
    let prog = Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(1))),
    };
    match mycelium_mlir::emit_llvm_ir(&prog) {
        Err(AotError::UnsupportedNode(msg)) => {
            assert!(
                msg.contains("Construct"),
                "the trampoline pre-call refusal must name the offending Construct; got: {msg}"
            );
        }
        other => panic!(
            "a Construct in a non-tail Fix trampoline's pre-call must stay UnsupportedNode, got {other:?}"
        ),
    }
}

// ─── Category 8 (refused): Repr::Vsa reaching the generic bit/trit Node path ───────────────────────
// (VSA has its own dedicated `vsa_codegen` entry points — VsaProgram/vsa_compile_and_run — but a
// `Repr::Vsa` `Const` fed into the *generic* Node lowering this corpus targets is refused at
// `const_lane`, naming Vsa, exactly like `vsa_differential.rs`'s own
// `vsa_const_is_refused_by_the_generic_bit_trit_node_path`.)

#[test]
fn refused_vsa_repr_on_generic_node_path() {
    let vsa_val = Value::new(
        Repr::Vsa {
            model: "BSC".to_owned(),
            dim: 4,
            sparsity: SparsityClass::Dense,
        },
        Payload::Hypervector(vec![1.0, 0.0, 1.0, 0.0]),
        Meta::exact(Provenance::Root),
    )
    .unwrap();
    let node = Node::Const(vsa_val);
    // emit_llvm_ir refuses at const_lane (before any toolchain probe), so this holds without llc/clang.
    match mycelium_mlir::emit_llvm_ir(&node) {
        Err(AotError::UnsupportedRepr(msg)) => {
            assert!(msg.contains("Vsa"), "must name Vsa; got: {msg}");
        }
        other => {
            panic!("Vsa const must be UnsupportedRepr on the generic Node path, got {other:?}")
        }
    }
}

// ─── Category 9 (refused, proxy): wild/host-effect calls ───────────────────────────────────────────
// **Measured, not assumed**: `mycelium_core::Node` carries NO wild/host-effect-specific variant at
// all (verified — `Node`'s grammar is `Const|Var|Let|Op|Swap|Construct|Match|Lam|App|Fix|FixGroup`;
// there is no `HostCall`/`Wild` node, and grepping llvm.rs + trampoline.rs turns up zero 'wild' or
// 'HostCall' token, matching this package's own non-goals section). So there is no Core-IR shape to
// literally construct here at the codegen-API layer. This fixture is an explicit, documented PROXY:
// a `Node::Op` with a host-effect-shaped prim name that the backend has never heard of falls through
// to the same catch-all `_ => UnsupportedPrim` any unrecognized primitive hits — the same failure
// mode a lowered `wild { .. }` call would hit if it ever reached this backend as an Op. This is NOT
// a claim that `wild` lowers to an Op today (it does not reach mycelium-codegen at all yet); it is
// the closest honest measurement this crate's API can make of "an unrecognized effectful primitive
// is refused, never silently accepted."

#[test]
fn refused_wild_host_effect_proxy_unrecognized_prim() {
    let prog = Node::Op {
        prim: "host.time_mono_nanos".into(),
        args: vec![],
    };
    match mycelium_mlir::emit_llvm_ir(&prog) {
        Err(AotError::UnsupportedPrim(_)) => { /* expected — unrecognized primitive, never silent */
        }
        other => panic!(
            "an unrecognized host-effect-shaped primitive must be UnsupportedPrim (proxy for the \
             wild/host-effect boundary — see this fixture's doc comment), got {other:?}"
        ),
    }
}

// ─── Category 10 (Ok, dedicated Dense path): un-quantized F32 element-wise Dense (M-853) ───────────
// Distinct API from the generic AotError path above — `DenseProgram`/`DenseAotError` are Dense's own
// typed surface (RFC-0039 §5.1), asserted here to its own literal variant, not folded into AotError
// (they are genuinely different Rust types; forcing them through one enum would misrepresent the
// measured API shape).

#[test]
fn dense_unquantized_element_wise_lowers() {
    let prog = DenseProgram {
        op: DenseCgOp::Add,
        dim: 3,
        dtype: ScalarKind::F32,
        a: vec![1.0, 2.0, 3.0],
        b: Some(vec![0.5, 0.5, 0.5]),
        scale: None,
    };
    let result = dense_compile_and_run(&prog);
    if let Err(DenseAotError::ToolchainMissing(tool)) = &result {
        eprintln!("dense_unquantized_element_wise_lowers: skipping — {tool} not installed");
        return;
    }
    match result {
        Ok(DenseResult::Value(v)) => {
            assert_eq!(
                v.repr(),
                &Repr::Dense {
                    dim: 3,
                    dtype: ScalarKind::F32
                }
            );
        }
        other => panic!("un-quantized F32 Dense add must lower to Ok(Value), got {other:?}"),
    }
}

// ─── Category 11 (documented unknown — NOT asserted): quantized Dense refusal ──────────────────────
// `DenseAotError::QuantRefused` is declared (dense_codegen.rs) and documented as the refusal for a
// "quantized Dense value", but **measured**: nothing in this crate's current value model constructs
// one (grep of src/*.rs finds `QuantRefused` declared and referenced only in its own doc comment /
// Display impl — never constructed; the ADR-030 `QuantDesc` descriptor E20-1 would add does not yet
// exist). So there is currently no way to build a fixture that exercises this arm; asserting a
// refusal here would be unverifiable and asserting anything else would be worse. Left as an
// explicit "unknown / needs human" gap rather than false closure — re-visit once E20-1 lands a
// constructible quantized-Dense value.
//
// (Vsa quant/sparse refusal IS independently measured — see `vsa_differential.rs`'s
// `sbc_mapb_and_sparse_carrier_are_refused_never_silently` — that is SBC/MAP-B refusal in the
// *dedicated* VsaProgram path, not the "quantized Dense" arm this note is about.)

// ─── Category 12 (known-gap fixture, NOT fixed here — codegen#8, non-goal per PKG-WP9-AOT) ─────────
// `classify_arm`'s stray-self scan (the tail-Fix pre-tail check, llvm.rs ~line 1583-1617) does not
// apply the member-name shadowing guard `anf_refs_name` itself applies for `Rhs::FixGroup` (compare
// line ~1519-1523, which skips descending into a FixGroup member that shadows `self_name`, against
// line ~1597, which does not). This fixture pins TODAY's (possibly wrong) behavior on the exact
// pathological shape from codegen#8's issue body — a FixGroup member literally named `self` bound in
// a tail-Fix arm's pre-tail sequence — so the gap stays visible and tested, never silently certified
// as safe. Fixing codegen#8 is explicitly a non-goal of PKG-WP9-AOT (LOW severity; not required for
// V0) — left OPEN by design; this test only prevents silent regression/drift of the *current*
// behavior until someone does fix it.

#[test]
fn known_gap_codegen_8_fixgroup_self_name_shadow() {
    // self_name = "self"; the tail arm's pre-tail binds a FixGroup with a member ALSO named "self".
    // anf_refs_name's own shadowing rule (used elsewhere) would treat this as shadowed (skip
    // descending); classify_arm's stray-self scan at the FixGroup arm (line ~1597) does not apply
    // that guard, so it descends into the shadowing member's body looking for "self" anyway.
    let shadowing_group = Node::FixGroup {
        defs: vec![
            (
                // This member is literally named the same as the outer Fix's self-name.
                "self".to_string(),
                Box::new(Node::Lam {
                    param: "z".into(),
                    body: Box::new(var("z")),
                }),
            ),
            (
                "h".to_string(),
                Box::new(Node::Lam {
                    param: "y".into(),
                    body: Box::new(var("y")),
                }),
            ),
        ],
        body: Box::new(Node::Const(byte_n(0))),
    };
    // outer = Fix(self, λn. Match n { 0 → 0xAA ; _ → let _ = shadowing_group in App(self, 0) })
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(var("n")),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                body: Node::Const(byte_n(0xAA)),
            }],
            default: Some(Box::new(Node::Let {
                id: "grp".into(),
                bound: Box::new(shadowing_group),
                body: Box::new(Node::App {
                    func: Box::new(var("self")),
                    arg: Box::new(Node::Const(byte_n(0))),
                }),
            })),
        }),
    };
    let prog = Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(1))),
    };
    // Pin CURRENT (possibly-wrong per #8) behavior — do not "fix" this assertion without also
    // fixing classify_arm and closing codegen#8; the point of this fixture is to make any future
    // change to this behavior a visible, deliberate diff, never a silent one.
    //
    // MEASURED (not the guess this comment originally carried): this shape reproduces codegen#8's
    // false-refuse exactly. The outer Fix's tail step (`App(self, 0)`) is legitimately tail — but
    // the stray-self scan over the pre-tail bindings hits the `Rhs::FixGroup` arm at llvm.rs
    // ~line 1597 without the shadowing guard `anf_refs_name` applies at ~line 1519-1523, and (per
    // the ANF flattening naming an intermediate binding after the shadowing member itself) the scan
    // trips on a binding literally named `self` — producing exactly the "non-tail self-reference"
    // `UnsupportedNode` a genuinely-non-tail program would get, even though this program's only
    // outer-Fix self-reference IS the tail call. This is the false-refuse #8 describes, reproduced.
    let observed = mycelium_mlir::emit_llvm_ir(&prog);
    match &observed {
        Err(AotError::UnsupportedNode(msg)) => {
            assert!(
                msg.contains("non-tail self-reference"),
                "codegen#8: expected the false-refuse's exact message shape, got: {msg}"
            );
        }
        Ok(_) => panic!(
            "codegen#8 known-gap fixture: unexpectedly lowered to Ok — re-check whether #8 was \
             fixed (this would be real progress; investigate and update this assertion + link the \
             fixing PR before flipping it)"
        ),
        Err(AotError::ToolchainMissing(_)) => { /* emission is checked above `compile`, unaffected */
        }
        Err(other) => panic!("codegen#8 known-gap fixture: unexpected error variant {other:?}"),
    }
}
