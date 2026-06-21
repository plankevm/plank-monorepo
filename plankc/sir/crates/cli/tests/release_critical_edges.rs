#![allow(unused_crate_dependencies)]

use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

const SIR_WITH_SHARED_REVERT_EDGE: &str = r#"
fn init:
    entry {
        size = runtime_length
        offset = runtime_start_offset
        ptr = malloc size
        codecopy ptr offset size
        return ptr size
    }

fn main:
    entry {
        c0 = callvalue
        => c0 ? @make_value : @revert_error
    }
    make_value {
        x = const 0x2a
        c1 = callvalue
        => c1 ? @use_value : @return_zero
    }
    return_zero {
        z = const 0x0
        return z z
    }
    use_value {
        y = add x x
        c2 = callvalue
        => c2 ? @return_value : @revert_error
    }
    return_value {
        size = const 0x20
        out = malloc size
        mstore256 out y
        return out size
    }
    revert_error {
        zero = const 0x0
        revert zero zero
    }
"#;

fn run_sir(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sir"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sir CLI");

    child
        .stdin
        .as_mut()
        .expect("sir CLI stdin")
        .write_all(input.as_bytes())
        .expect("write SIR to stdin");

    child.wait_with_output().expect("wait for sir CLI")
}

#[test]
fn release_splits_critical_edges_for_block_parameter_sir() {
    let debug = run_sir(&[], SIR_WITH_SHARED_REVERT_EDGE);
    assert!(
        debug.status.success(),
        "debug backend should accept fixture:\n{}",
        String::from_utf8_lossy(&debug.stderr)
    );

    let release = run_sir(&["--release"], SIR_WITH_SHARED_REVERT_EDGE);
    assert!(
        release.status.success(),
        "release backend should accept fixture after critical-edge splitting:\n{}",
        String::from_utf8_lossy(&release.stderr)
    );

    assert!(String::from_utf8_lossy(&release.stdout).starts_with("0x"));
}
