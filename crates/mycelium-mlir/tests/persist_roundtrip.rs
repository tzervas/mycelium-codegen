//! `CompiledArtifact::persist` round-trip (S-AOT-NATIVE-COMPILE; PKG-WP9-AOT).
//!
//! `CompiledArtifact.bin` lives inside a private [`TmpDir`](mycelium_mlir) whose `Drop` impl does
//! `remove_dir_all` — before `persist` existed there was **no public way** to extract a compiled
//! binary onto disk (`run()` executes in place and drops the artifact). This proves the extraction
//! actually works end-to-end: `compile()` → `persist(tmp)` → **execute the persisted file directly
//! as a subprocess** (not via `.run()`, which would hide an argv/permissions/relocation bug
//! `persist()` itself could introduce) → the read-back value matches what `.run()` reports for the
//! *same* compiled artifact.
//!
//! Skips gracefully on `AotError::ToolchainMissing` (the house idiom — no llc/clang on this
//! runner) rather than failing CI.

use std::process::Command;

use mycelium_core::{Meta, Node, Payload, Provenance, Repr, Value};
use mycelium_mlir::AotError;

fn byte(bits: [bool; 8]) -> Value {
    Value::new(
        Repr::Binary { width: 8 },
        Payload::Bits(bits.to_vec()),
        Meta::exact(Provenance::Root),
    )
    .unwrap()
}

/// `bit.not(A)` — a trivial one-op program, just enough to produce a non-trivial read-back value.
fn program() -> Node {
    Node::Op {
        prim: "bit.not".into(),
        args: vec![Node::Const(byte([
            true, false, true, true, false, false, true, false,
        ]))],
    }
}

#[test]
fn persist_then_direct_exec_matches_run() {
    let artifact = match mycelium_mlir::compile(&program()) {
        Ok(a) => a,
        Err(AotError::ToolchainMissing(tool)) => {
            eprintln!("skipping: {tool} not installed (house idiom — env skip)");
            return;
        }
        Err(e) => panic!("compile() must succeed for a trivial bit.not program: {e}"),
    };

    // The value `.run()` reports (execute-in-place, inside the still-live TmpDir).
    let run_value = artifact
        .run()
        .expect(".run() must succeed on a valid artifact");

    // Persist to an independent location, OUTSIDE the artifact's own TmpDir, and outlive it. No
    // `tempfile` dev-dependency exists in this crate (avoid adding one for a single test) — a
    // process-id + nanos unique dir under `std::env::temp_dir()` mirrors the house idiom
    // `llvm::unique_tmp_dir` already uses internally.
    let out_dir = std::env::temp_dir().join(format!(
        "myc-persist-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&out_dir).expect("create independent persist-dest dir");
    let dest = out_dir.join("persisted-kernel");
    let returned = artifact
        .persist(&dest)
        .expect("persist() must copy the compiled binary to dest");
    assert_eq!(
        returned, dest,
        "persist() must return the dest path on success"
    );
    assert!(dest.exists(), "the persisted file must exist on disk");

    // Drop the artifact NOW — its TmpDir guard runs `remove_dir_all` on the *original* location.
    // The persisted copy at `dest` (a different directory) must survive this.
    drop(artifact);
    assert!(
        dest.exists(),
        "the persisted copy must survive the source CompiledArtifact's Drop"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&dest).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o755,
            "persist() must set 0o755 on unix so the file is directly executable"
        );
    }

    // Execute the PERSISTED FILE DIRECTLY as a subprocess (not through the crate's own `.run()`,
    // which would hide an argv/permissions/relocation bug persist() could have introduced).
    let output = Command::new(&dest)
        .output()
        .unwrap_or_else(|e| panic!("direct exec of the persisted artifact must succeed: {e}"));
    assert!(
        output.status.success(),
        "the persisted artifact must exit successfully when run directly, got {}",
        output.status
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let line = stdout.lines().next().unwrap_or("");

    // Decode the same way CompiledArtifact::run() does for an 8-bit binary lane: one '0'/'1' char
    // per bit, LSB-first (matches emit_llvm_ir's read-back format for Binary{8}).
    let direct_bits: Vec<bool> = line.chars().map(|c| c == '1').collect();
    let direct_value = Value::new(
        Repr::Binary { width: 8 },
        Payload::Bits(direct_bits),
        Meta::exact(Provenance::Root),
    )
    .expect("well-formed 8-bit value from the persisted artifact's direct read-back");

    assert_eq!(
        (direct_value.repr(), direct_value.payload()),
        (run_value.repr(), run_value.payload()),
        "the persisted-and-directly-executed artifact must produce the same read-back value as \
         `.run()` reported for the same compiled program"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}
