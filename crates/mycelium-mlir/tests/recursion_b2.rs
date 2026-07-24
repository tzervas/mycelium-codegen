//! Wave B2 — `FixGroup` inside pure-tail `Fix` arm bindings (direct-LLVM).
//!
//! Closes the B1 residual: a `FixGroup` bound in a tail-Fix arm's pre-tail sequence is suspended
//! (M-850) and, when applied, routes through the shared heap trampoline. The outer pure-tail `Fix`
//! stays on the iterative loop with B1 dedicated back-edge blocks (SSA block-params / phi
//! discipline). Mutual / grouped recursion therefore compiles natively without rewriting the
//! trampoline machinery.
//!
//! Guarantee: **Declared** emission + **Empirical** differential (interp ≡ env-machine ≡ native
//! where the harness supports it). Skips the native executable leg gracefully on
//! `AotError::ToolchainMissing`. Residual: non-straight-line pre-call bindings on the *trampoline*
//! path (nested FixGroup/Match before a non-tail recursive call) stay a tested honest refuse.

mod common;
use common::{byte, observable, B};

use mycelium_cert::{check, CheckVerdict, Evidence, RefinementRelation};
use mycelium_core::{Alt, GuaranteeStrength, Node, Payload, Repr, Value};
use mycelium_interp::{EvalError, IdentitySwapEngine, Interpreter, PrimRegistry};
use mycelium_mlir::AotError;
use mycelium_numerics::Certificate;

fn byte_n(n: u8) -> Value {
    let bits: Vec<bool> = (0..8).map(|i| (n >> i) & 1 == 1).collect();
    Value::new(
        Repr::Binary { width: 8 },
        Payload::Bits(bits),
        mycelium_core::Meta::exact(mycelium_core::Provenance::Root),
    )
    .expect("8-bit value")
}

fn bits(n: u8) -> Vec<bool> {
    (0..8).map(|i| (n >> i) & 1 == 1).collect()
}

fn interp_bounded(node: &Node, fuel: u64) -> Result<Value, EvalError> {
    Interpreter::new(PrimRegistry::with_builtins(), Box::new(IdentitySwapEngine))
        .with_fuel(fuel)
        .eval(node)
}

fn env_machine(node: &Node) -> Result<Value, EvalError> {
    mycelium_mlir::run(node, &PrimRegistry::with_builtins(), &IdentitySwapEngine)
}

/// Three-way (interp, env-machine, native) when tools are present; emission always gated.
/// `allow_trampoline`: applied FixGroup members nest the heap trampoline; unused suspension
/// keeps the outer pure-tail Fix on the iterative loop alone.
fn assert_threeway(prog: &Node, expected: &Value, label: &str, allow_trampoline: bool) {
    let interp = interp_bounded(prog, 100_000).unwrap_or_else(|e| {
        panic!("{label}: interpreter refused a valid B2 program: {e}");
    });
    assert_eq!(
        observable(&interp),
        observable(expected),
        "{label}: interpreter result"
    );

    let env = env_machine(prog).unwrap_or_else(|e| {
        panic!("{label}: env-machine refused a valid B2 program: {e}");
    });
    assert_eq!(
        observable(&interp),
        observable(&env),
        "{label}: interp ≢ env-machine"
    );

    let ir = mycelium_mlir::emit_llvm_ir(prog)
        .unwrap_or_else(|e| panic!("{label}: emit_llvm_ir must succeed under B2: {e}"));
    if !allow_trampoline {
        assert!(
            !ir.contains("@myc_tramp_alloc"),
            "{label}: unused FixGroup suspension must keep the outer Fix on the tail loop"
        );
    }
    assert!(
        ir.contains("phi i64"),
        "{label}: outer pure-tail Fix must emit the header phi (B1 SSA block-params)"
    );

    match mycelium_mlir::compile_and_run(prog) {
        Ok(native) => {
            assert_eq!(
                observable(&interp),
                observable(&native),
                "{label}: interp ≢ native"
            );
            assert_eq!(
                check(
                    &interp,
                    &native,
                    RefinementRelation::ObservationalEquiv,
                    Certificate::exact(),
                    &Evidence::Observational,
                ),
                CheckVerdict::Validated {
                    strength: GuaranteeStrength::Exact
                },
                "{label}: M-210 checker must validate interp↔native"
            );
        }
        Err(AotError::ToolchainMissing(_)) => { /* house idiom — emission already gated */ }
        Err(e) => panic!("{label}: native path unexpected error: {e}"),
    }
}

// ─── Suspended (unused) FixGroup in pre-tail ────────────────────────────────────────────────────

/// Outer pure-tail Fix binds an unused FixGroup then tail-calls to base. Proves suspension alone
/// does not force the trampoline and still yields the base result.
fn suspended_unused_fixgroup_in_arm() -> Node {
    let inner = Node::FixGroup {
        defs: vec![
            (
                "g".into(),
                Box::new(Node::Lam {
                    param: "x".into(),
                    body: Box::new(Node::Var("x".into())),
                }),
            ),
            (
                "h".into(),
                Box::new(Node::Lam {
                    param: "y".into(),
                    body: Box::new(Node::Var("y".into())),
                }),
            ),
        ],
        body: Box::new(Node::Const(byte_n(0))),
    };
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(Node::Var("n".into())),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                body: Node::Const(byte(B)),
            }],
            default: Some(Box::new(Node::Let {
                id: "grp".into(),
                bound: Box::new(inner),
                body: Box::new(Node::App {
                    func: Box::new(Node::Var("self".into())),
                    arg: Box::new(Node::Const(byte_n(0))),
                }),
            })),
        }),
    };
    Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(1))),
    }
}

#[test]
fn suspended_unused_fixgroup_in_arm_threeway() {
    assert_threeway(
        &suspended_unused_fixgroup_in_arm(),
        &byte(B),
        "suspended unused FixGroup in pure-tail arm",
        false,
    );
}

// ─── Mutual recursion via FixGroup applied inside a Fix arm (base) ──────────────────────────────

/// Canonical two-member mutual group used as a *base* of an outer pure-tail Fix:
/// ```text
/// e(n) = Match n { 0 → 0xAA ; _ → o(0) }
/// o(n) = Match n { 0 → bit.not(e(0)) ; _ → e(0) }
/// outer = Fix(self, λn. Match n {
///   0 → App(e, 1)     // base: e(1) → o(0) → not(e(0)) = not(0xAA) = 0x55
///   _ → App(self, 0)  // tail into base
/// })
/// App(outer, 1)
/// ```
fn mutual_fixgroup_applied_as_base() -> Node {
    let even = (
        "e".to_string(),
        Box::new(Node::Lam {
            param: "x".into(),
            body: Box::new(Node::Match {
                scrutinee: Box::new(Node::Var("x".into())),
                alts: vec![Alt::Lit {
                    value: byte_n(0),
                    body: Node::Const(byte_n(0xAA)),
                }],
                default: Some(Box::new(Node::App {
                    func: Box::new(Node::Var("o".into())),
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
                scrutinee: Box::new(Node::Var("x".into())),
                alts: vec![Alt::Lit {
                    value: byte_n(0),
                    body: Node::Op {
                        prim: "bit.not".into(),
                        args: vec![Node::App {
                            func: Box::new(Node::Var("e".into())),
                            arg: Box::new(Node::Const(byte_n(0))),
                        }],
                    },
                }],
                default: Some(Box::new(Node::App {
                    func: Box::new(Node::Var("e".into())),
                    arg: Box::new(Node::Const(byte_n(0))),
                })),
            }),
        }),
    );
    let group = Node::FixGroup {
        defs: vec![even, odd],
        body: Box::new(Node::App {
            func: Box::new(Node::Var("e".into())),
            arg: Box::new(Node::Const(byte_n(1))),
        }),
    };
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(Node::Var("n".into())),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                // Base arm: bind+apply the mutual group (nested trampoline).
                body: group,
            }],
            default: Some(Box::new(Node::App {
                func: Box::new(Node::Var("self".into())),
                arg: Box::new(Node::Const(byte_n(0))),
            })),
        }),
    };
    Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(1))),
    }
}

#[test]
fn mutual_fixgroup_applied_as_base_threeway() {
    // e(1) → o(0) → not(e(0)) = not(0xAA) = 0x55
    let expected = Value::new(
        Repr::Binary { width: 8 },
        Payload::Bits(bits(!0xAA)),
        mycelium_core::Meta::exact(mycelium_core::Provenance::Root),
    )
    .expect("8-bit");
    assert_threeway(
        &mutual_fixgroup_applied_as_base(),
        &expected,
        "mutual FixGroup applied as outer Fix base arm",
        true,
    );
}

// ─── FixGroup result feeds the pure-tail step ───────────────────────────────────────────────────

/// Outer pure-tail Fix whose step is the result of applying a mutual FixGroup member:
/// ```text
/// e(n) = Match n { 0 → 0 ; _ → o(0) }   // e(1) → o(0) → e(0) → 0
/// o(n) = Match n { 0 → e(0) ; _ → e(0) }
/// outer = Fix(self, λn. Match n {
///   0 → B
///   _ → let step = FixGroup{e,o} in App(e,1) ; App(self, step)
/// })
/// App(outer, 1)  // 1 → e(1)=0 → B
/// ```
fn mutual_fixgroup_feeds_tail_step() -> Node {
    let even = (
        "e".to_string(),
        Box::new(Node::Lam {
            param: "x".into(),
            body: Box::new(Node::Match {
                scrutinee: Box::new(Node::Var("x".into())),
                alts: vec![Alt::Lit {
                    value: byte_n(0),
                    body: Node::Const(byte_n(0)),
                }],
                default: Some(Box::new(Node::App {
                    func: Box::new(Node::Var("o".into())),
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
                scrutinee: Box::new(Node::Var("x".into())),
                alts: vec![Alt::Lit {
                    value: byte_n(0),
                    body: Node::App {
                        func: Box::new(Node::Var("e".into())),
                        arg: Box::new(Node::Const(byte_n(0))),
                    },
                }],
                default: Some(Box::new(Node::App {
                    func: Box::new(Node::Var("e".into())),
                    arg: Box::new(Node::Const(byte_n(0))),
                })),
            }),
        }),
    );
    let step_via_group = Node::FixGroup {
        defs: vec![even, odd],
        body: Box::new(Node::App {
            func: Box::new(Node::Var("e".into())),
            arg: Box::new(Node::Const(byte_n(1))),
        }),
    };
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(Node::Var("n".into())),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                body: Node::Const(byte(B)),
            }],
            default: Some(Box::new(Node::App {
                func: Box::new(Node::Var("self".into())),
                arg: Box::new(step_via_group),
            })),
        }),
    };
    Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(1))),
    }
}

#[test]
fn mutual_fixgroup_feeds_tail_step_threeway() {
    assert_threeway(
        &mutual_fixgroup_feeds_tail_step(),
        &byte(B),
        "mutual FixGroup result feeds pure-tail step",
        true,
    );
}

// ─── Residual: FixGroup in trampoline (non-tail) pre-call stays refused ─────────────────────────

/// Non-tail single Fix (`bit.not(self(…))`) whose pre-call bindings include a nested FixGroup —
/// still outside the straight-line trampoline pre-call fragment (Wave-B2 residual on this path).
fn fixgroup_in_trampoline_precall_program() -> Node {
    let nested = Node::FixGroup {
        defs: vec![
            (
                "g".into(),
                Box::new(Node::Lam {
                    param: "x".into(),
                    body: Box::new(Node::Match {
                        scrutinee: Box::new(Node::Var("x".into())),
                        alts: vec![Alt::Lit {
                            value: byte_n(0),
                            body: Node::Const(byte_n(0)),
                        }],
                        default: Some(Box::new(Node::Const(byte_n(0)))),
                    }),
                }),
            ),
            (
                "h".into(),
                Box::new(Node::Lam {
                    param: "y".into(),
                    body: Box::new(Node::Match {
                        scrutinee: Box::new(Node::Var("y".into())),
                        alts: vec![Alt::Lit {
                            value: byte_n(0),
                            body: Node::Const(byte_n(0)),
                        }],
                        default: Some(Box::new(Node::Const(byte_n(0)))),
                    }),
                }),
            ),
        ],
        body: Box::new(Node::Const(byte_n(0))),
    };
    // f = λn. Match n { 0 → 0xAA ; _ → bit.not( let _ = nested in App(self, 0) ) }
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(Node::Var("n".into())),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                body: Node::Const(byte_n(0xAA)),
            }],
            default: Some(Box::new(Node::Op {
                prim: "bit.not".into(),
                args: vec![Node::Let {
                    id: "grp".into(),
                    bound: Box::new(nested),
                    body: Box::new(Node::App {
                        func: Box::new(Node::Var("self".into())),
                        arg: Box::new(Node::Const(byte_n(0))),
                    }),
                }],
            })),
        }),
    };
    Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(1))),
    }
}

#[test]
fn fixgroup_in_trampoline_precall_stays_honest_refuse() {
    let prog = fixgroup_in_trampoline_precall_program();
    match mycelium_mlir::emit_llvm_ir(&prog) {
        Err(AotError::UnsupportedNode(msg)) => {
            assert!(
                msg.contains("FixGroup")
                    || msg.contains("non-straight-line")
                    || msg.contains("trampoline")
                    || msg.contains("pre-call"),
                "residual refuse must name trampoline / FixGroup / pre-call; got: {msg}"
            );
        }
        other => panic!("FixGroup in trampoline pre-call must stay UnsupportedNode; got {other:?}"),
    }
}
