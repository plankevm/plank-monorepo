# Lint Rules

## Simple Rules

|Category|Detection Pattern|Fix|
|--------|----|-----------|
|Diagnostics|Use of `session.emit_diagnostic(diagnostic)` for concrete diagnostic methods|Use `diagnostic.emit(session)` instead, its more concise and chains nicely with the other diagnostic builder methods|

## Test Strings

Strings that represent _potentially_ multi-line content such as plank source inputs, rendered
diagnostics, rendered IR and more should be formatted consistently according to
the following style:

- always multi-line
- never inline `\n` characters
- always raw Rust strings `r#" ... "#` for consistency
- always start the string's content on the line **after** the opening `r#"`
- maintain consistent indentation relative to the snippet itself and its
  surroundings (typically requires `dedent_preserve_indent`)

**Examples**
Good:
```rust
assert_lowers_to(
    r#"
    init {
        let buf = malloc_uninit(0x20);
        mstore32(buf, 0x05);
        evm_return(buf, 0x20);
    }
    "#,
    // ...
);
```

Good (technically one-line but is source code and should therefore be multi-line):
```rust
assert_lowers_to(
    r#"
    const x = 
    "#,
    // ...
);
```
Bad (first line on the same line as opening `r#"`):
```rust
assert_lowers_to(
    r#"init {
        let buf = malloc_uninit(0x20);
        mstore32(buf, 0x05);
        evm_return(buf, 0x20);
    }
    "#,
    // ...
);
```

Bad (last line on the same line as closing `"#`):
```rust
assert_lowers_to(
    r#"
    init {
        let buf = malloc_uninit(0x20);
    }"#,
    // ...
);
```

Bad (inconsistent indentation relative to the surroundings):
```rust
assert_lowers_to(
    r#"
        init {
            let buf = malloc_uninit(0x20);
        }
    "#,
    // ...
);
```

Bad (inconsistent indentation relative to itself):
```rust
assert_lowers_to(
    r#"
    init {
        let buf = malloc_uninit(0x20);
        }
    "#,
    // ...
);
```
