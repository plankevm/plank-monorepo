#![allow(unused_crate_dependencies)]

use plank_test_utils::dedent_preserve_indent;
use std::{fs, process::Command};
use tempfile::TempDir;

fn plank_bin() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_plank").into()
}

fn create_std_dir(dir: &TempDir) {
    let std_path = dir.path().join("stdlib");
    fs::create_dir_all(&std_path).unwrap();
    fs::write(
        std_path.join("math.plk"),
        dedent_preserve_indent(
            r#"
            const max = fn (a: u256, b: u256) u256 {
                if @evm_gt(a, b) { a } else { b }
            };
            "#,
        ),
    )
    .unwrap();
}

#[test]
fn test_auto_registers_std_from_plank_dir() {
    let plank_dir = TempDir::new().unwrap();
    create_std_dir(&plank_dir);

    let source_dir = TempDir::new().unwrap();
    let source_file = source_dir.path().join("main.plk");
    fs::write(
        &source_file,
        dedent_preserve_indent(
            r#"
            import std::math::max;

            init {
                let a = max(1, 2);
                @evm_stop();
            }
            "#,
        ),
    )
    .unwrap();

    let output = Command::new(plank_bin())
        .arg("build")
        .arg(&source_file)
        .env("PLANK_DIR", plank_dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_explicit_dep_overrides_plank_dir_std() {
    let plank_dir = TempDir::new().unwrap();
    create_std_dir(&plank_dir);

    let custom_std = TempDir::new().unwrap();
    fs::write(
        custom_std.path().join("math.plk"),
        dedent_preserve_indent(
            r#"
            const skibidi_max = fn (a: u256, b: u256) u256 {
                if @evm_gt(a, b) { a } else { b }
            };
            "#,
        ),
    )
    .unwrap();

    let source_dir = TempDir::new().unwrap();
    let source_file = source_dir.path().join("main.plk");
    fs::write(
        &source_file,
        dedent_preserve_indent(
            r#"
            import std::math::skibidi_max;

            init {
                let a = skibidi_max(1, 2);
                @evm_stop();
            }
            "#,
        ),
    )
    .unwrap();

    let output = Command::new(plank_bin())
        .arg("build")
        .arg(&source_file)
        .arg("--dep")
        .arg(format!("std={}", custom_std.path().display()))
        .env("PLANK_DIR", plank_dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn missing_std_fails_to_compile() {
    let source_dir = TempDir::new().unwrap();
    let source_file = source_dir.path().join("main.plk");
    fs::write(
        &source_file,
        r#"import std::math::max;

init {
    evm_stop();
}
"#,
    )
    .unwrap();

    let output = Command::new(plank_bin())
        .arg("build")
        .arg(&source_file)
        .env("PLANK_DIR", "/nonexistent")
        .env("HOME", "/nonexistent")
        .output()
        .unwrap();

    assert!(!output.status.success());
}
