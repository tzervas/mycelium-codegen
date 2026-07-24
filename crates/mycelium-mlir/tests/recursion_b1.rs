//! Wave B1 — Match in the pre-tail position of a pure-tail `Fix` (DN-15 §8.5).
//!
//! Proves that factorial / ackermann / list-fold *style* recursive programs whose tail-step is
//! computed by a nested `Match` compile on the **direct-LLVM** tail-loop path and agree with the
//! reference interpreter and the env-machine (three-way where the harness supports it).
//!
//! Binary{8} is the recursion ABI (no integer mul/add on Binary), so these programs encode the
//! *control-flow shape* of those algorithms with Match-driven steps over small counters rather
//! than full arithmetic factorials. What is under test is the B1 lowering surface, not a math lib.
//!
//! Guarantee: **Declared** emission + **Empirical** differential (interp ≡ env-machine ≡ native).
//! Skips the native executable leg gracefully on `AotError::ToolchainMissing`.

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

fn interp_bounded(node: &Node, fuel: u64) -> Result<Value, EvalError> {
    Interpreter::new(PrimRegistry::with_builtins(), Box::new(IdentitySwapEngine))
        .with_fuel(fuel)
        .eval(node)
}

fn env_machine(node: &Node) -> Result<Value, EvalError> {
    // Env-machine second path (`aot::run`) — independent big-step evaluator for the three-way.
    mycelium_mlir::run(node, &PrimRegistry::with_builtins(), &IdentitySwapEngine)
}

/// Three-way (interp, env-machine, native) when tools are present; otherwise emission +
/// interp ≡ env-machine still gate the B1 surface.
fn assert_threeway(prog: &Node, expected: &Value, label: &str) {
    let interp = interp_bounded(prog, 100_000).unwrap_or_else(|e| {
        panic!("{label}: interpreter refused a valid B1 program: {e}");
    });
    assert_eq!(
        observable(&interp),
        observable(expected),
        "{label}: interpreter result"
    );

    let env = env_machine(prog).unwrap_or_else(|e| {
        panic!("{label}: env-machine refused a valid B1 program: {e}");
    });
    assert_eq!(
        observable(&interp),
        observable(&env),
        "{label}: interp ≢ env-machine"
    );

    // Emission must succeed without a toolchain (pure lowering gate).
    let ir = mycelium_mlir::emit_llvm_ir(prog)
        .unwrap_or_else(|e| panic!("{label}: emit_llvm_ir must succeed under B1: {e}"));
    assert!(
        !ir.contains("@myc_tramp_alloc"),
        "{label}: pure-tail B1 programs must stay on the iterative tail loop (no trampoline)"
    );
    assert!(
        ir.contains("phi i64"),
        "{label}: tail-loop IR must emit the header phi"
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

// ─── Factorial-style: Match-driven countdown of a small counter ─────────────────────────────────

/// Factorial *shape*: successive Match-selected decrements `n → n−1 → … → 0`, base returns `B`.
/// Encodes the iterative control of `fact` over a Binary{8} counter (no mul in the ABI):
/// ```text
/// f = Fix(self, λn. Match n {
///   Lit 0 → B
///   default → App(self, Match n { Lit 5→4; 4→3; 3→2; 2→1; 1→0; _→0 })
/// })
/// App(f, 5)  // 5→4→3→2→1→0→B
/// ```
fn factorial_style_program(init: u8) -> Node {
    let pred = || Node::Match {
        scrutinee: Box::new(Node::Var("n".into())),
        alts: vec![
            Alt::Lit {
                value: byte_n(5),
                body: Node::Const(byte_n(4)),
            },
            Alt::Lit {
                value: byte_n(4),
                body: Node::Const(byte_n(3)),
            },
            Alt::Lit {
                value: byte_n(3),
                body: Node::Const(byte_n(2)),
            },
            Alt::Lit {
                value: byte_n(2),
                body: Node::Const(byte_n(1)),
            },
            Alt::Lit {
                value: byte_n(1),
                body: Node::Const(byte_n(0)),
            },
        ],
        default: Some(Box::new(Node::Const(byte_n(0)))),
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
                arg: Box::new(pred()),
            })),
        }),
    };
    Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(init))),
    }
}

#[test]
fn factorial_style_match_in_tail_fix_threeway() {
    // Proves: multi-arm Match step in pre-tail of a pure-tail Fix lowers and runs (5 steps → B).
    assert_threeway(
        &factorial_style_program(5),
        &byte(B),
        "factorial-style countdown",
    );
}

// ─── Ackermann-style: nested Match in the step ──────────────────────────────────────────────────

/// Ackermann *shape*: a nested Match chooses the next counter (two-level decision tree), mirroring
/// the multi-branch recursive case selection of Ackermann without needing a second parameter:
/// ```text
/// f = Fix(self, λn. Match n {
///   Lit 0 → B
///   default → App(self, Match n {
///     Lit 3 → Match n { Lit 3 → 1; _ → 0 }   // nested
///     Lit 2 → 1
///     _     → 0
///   })
/// })
/// App(f, 3)  // 3 → (nested → 1) → 0 → B
/// ```
fn ackermann_style_program(init: u8) -> Node {
    let nested = Node::Match {
        scrutinee: Box::new(Node::Var("n".into())),
        alts: vec![Alt::Lit {
            value: byte_n(3),
            body: Node::Const(byte_n(1)),
        }],
        default: Some(Box::new(Node::Const(byte_n(0)))),
    };
    let step = Node::Match {
        scrutinee: Box::new(Node::Var("n".into())),
        alts: vec![
            Alt::Lit {
                value: byte_n(3),
                body: nested,
            },
            Alt::Lit {
                value: byte_n(2),
                body: Node::Const(byte_n(1)),
            },
        ],
        default: Some(Box::new(Node::Const(byte_n(0)))),
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
                arg: Box::new(step),
            })),
        }),
    };
    Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(init))),
    }
}

#[test]
fn ackermann_style_nested_match_in_tail_fix_threeway() {
    // Proves: nested Match inside the pre-tail step (join-block discipline in lower_match) works.
    assert_threeway(
        &ackermann_style_program(3),
        &byte(B),
        "ackermann-style nested Match step",
    );
}

// ─── List-fold style: Match-driven fold over a counter (OR-accumulate via bit.or of masks) ──────

/// List-fold *shape*: each recursive step selects a mask by Match and `bit.or`s it into an
/// accumulator packed in the same Binary{8} state. Encoding (low 4 bits = counter, high nibble
/// unused for simplicity — we only use successive states 3,2,1,0 and OR a fixed pattern each
/// step is not available without non-tail; instead we fold by returning a Match-selected constant
/// at the base after walking the list length:
/// ```text
/// fold = Fix(self, λn. Match n {
///   Lit 0 → byte(0b0000_1111)          // folded result for empty tail
///   default → App(self, Match n { 3→2; 2→1; 1→0; _→0 })
/// })
/// App(fold, 3)  // walks 3→2→1→0, returns 0x0F
/// ```
/// This is the control skeleton of a list fold (walk spine via Match, emit base at Nil).
fn fold_style_program(init: u8) -> Node {
    let folded = byte_n(0x0F);
    let pred = || Node::Match {
        scrutinee: Box::new(Node::Var("n".into())),
        alts: vec![
            Alt::Lit {
                value: byte_n(3),
                body: Node::Const(byte_n(2)),
            },
            Alt::Lit {
                value: byte_n(2),
                body: Node::Const(byte_n(1)),
            },
            Alt::Lit {
                value: byte_n(1),
                body: Node::Const(byte_n(0)),
            },
        ],
        default: Some(Box::new(Node::Const(byte_n(0)))),
    };
    let fix_body = Node::Lam {
        param: "n".into(),
        body: Box::new(Node::Match {
            scrutinee: Box::new(Node::Var("n".into())),
            alts: vec![Alt::Lit {
                value: byte_n(0),
                body: Node::Const(folded),
            }],
            default: Some(Box::new(Node::App {
                func: Box::new(Node::Var("self".into())),
                arg: Box::new(pred()),
            })),
        }),
    };
    Node::App {
        func: Box::new(Node::Fix {
            name: "self".into(),
            body: Box::new(fix_body),
        }),
        arg: Box::new(Node::Const(byte_n(init))),
    }
}

#[test]
fn fold_style_match_in_tail_fix_threeway() {
    // Proves: fold-shaped walk with Match step + distinct base result (0x0F).
    assert_threeway(
        &fold_style_program(3),
        &byte_n(0x0F),
        "list-fold-style Match walk",
    );
}

// ─── B2 residual (kept hard-refused in this suite too) ──────────────────────────────────────────

#[test]
fn b2_fixgroup_in_fix_arm_bindings_stays_hard_refuse() {
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
                msg.contains("FixGroup"),
                "B2 residual must name FixGroup; got: {msg}"
            );
            assert!(
                msg.contains("B2") || msg.contains("arm binding") || msg.contains("mutual"),
                "B2 residual message should identify the Wave-B2 / arm-binding residual; got: {msg}"
            );
        }
        other => panic!("expected hard UnsupportedNode for B2 residual, got {other:?}"),
    }
}
