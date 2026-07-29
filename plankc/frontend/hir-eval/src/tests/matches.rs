use super::*;

#[test]
fn test_comptime_match_selects_arm() {
    assert_lowers_to(
        r#"
        init {
            let x = match 2 {
                1 => 10,
                2 => 20,
                else => 30,
            };
            @evm_sstore(0, x);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = 20
            %2 : void = @evm_sstore(%0, %1)
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_comptime_match_evaluates_only_selected_arm() {
    assert_diagnostics(
        r#"
        init {
            let x = match 0 {
                0 => { let selected: bool = 0; 1 },
                1 => { let unselected: bool = 0; 2 },
                else => 3,
            };
            @evm_stop();
        }
        "#,
        &[r#"
        error: mismatched types
         --> main.plk:3:37
          |
        3 |         0 => { let selected: bool = 0; 1 },
          |                              ----   ^ expected `bool`, got `u256`
          |                              |
          |                              `bool` expected because of this
        "#],
    );
}

#[test]
fn test_comptime_match_selects_else() {
    assert_lowers_to(
        r#"
        init {
            let x = match 3 {
                1 => 10,
                2 => 20,
                else => 30,
            };
            @evm_sstore(0, x);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = 30
            %2 : void = @evm_sstore(%0, %1)
            %3 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_match_lowers_to_mir_match() {
    assert_lowers_to(
        r#"
        init {
            let selector = @evm_calldataload(0);
            let x = match selector {
                0x04 => 40,
                2 => 20,
                else => 99,
            };
            @evm_sstore(0, x);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = %1
            match %2 {
                0x4 => {
                    %3 : u256 = 40
                }
                0x2 => {
                    %3 : u256 = 20
                }
                else {
                    %3 : u256 = 99
                }
            }
            %4 : u256 = %3
            %5 : u256 = %4
            %6 : u256 = 0
            %7 : void = @evm_sstore(%6, %5)
            %8 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_match_all_arms_terminate() {
    assert_lowers_to(
        r#"
        const choose = fn(selector: u256) u256 {
            match selector {
                1 => { return 11; },
                else => @evm_invalid(),
            };
        };

        init {
            let result = choose(@evm_calldataload(0));
            @evm_sstore(0, result);
            @evm_stop();
        }
        "#,
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            match %1 {
                0x1 => {
                    %2 : u256 = 11
                    ret %2
                }
                else {
                    %3 : never = @evm_invalid()
                }
            }
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 0
            %1 : u256 = @evm_calldataload(%0)
            %2 : u256 = call @fn0(%1)
            %3 : u256 = %2
            %4 : u256 = 0
            %5 : void = @evm_sstore(%4, %3)
            %6 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_runtime_match_arm_type_mismatch() {
    assert_diagnostics(
        r#"
        init {
            let selector = @evm_calldataload(0);
            let x = match selector {
                1 => 334,
                else => false,
            };
            @evm_stop();
        }
        "#,
        &[r#"
        error: incompatible branch types
         --> main.plk:5:17
          |
        4 |         1 => 334,
          |              --- `u256` expected because of this
        5 |         else => false,
          |                 ^^^^^ expected `u256`, got `bool`
        "#],
    );
}

#[test]
fn test_additional_else_arm_body_is_typechecked() {
    assert_diagnostics(
        r#"
        init {
            let subject = @evm_calldataload(0);
            let x = match subject {
                else => 1,
                else other => {
                    let from_binding: bool = other;
                    let checked: bool = 0;
                    2
                },
            };
            @evm_stop();
        }
        "#,
        &[
            r#"
        error: multiple else arms in match
         --> main.plk:5:9
          |
        4 |           else => 1,
          |           --------- previous else arm
        5 | /         else other => {
        6 | |             let from_binding: bool = other;
        7 | |             let checked: bool = 0;
        8 | |             2
        9 | |         },
          | |_________^ duplicate else arm
          |
          = note: a match expression can have only one else arm
        "#,
            r#"
        error: mismatched types
         --> main.plk:7:33
          |
        7 |             let checked: bool = 0;
          |                          ----   ^ expected `bool`, got `u256`
          |                          |
          |                          `bool` expected because of this
        "#,
        ],
    );
}

#[test]
fn test_additional_else_arm_body_is_not_typechecked_for_comptime_subject() {
    assert_diagnostics(
        r#"
        init {
            let x = match 0 {
                else => 1,
                else => { let checked: bool = 0; 2 },
            };
            @evm_stop();
        }
        "#,
        &[r#"
        error: multiple else arms in match
         --> main.plk:4:9
          |
        3 |         else => 1,
          |         --------- previous else arm
        4 |         else => { let checked: bool = 0; 2 },
          |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ duplicate else arm
          |
          = note: a match expression can have only one else arm
        "#],
    );
}

#[test]
fn test_match_key_must_be_comptime() {
    assert_diagnostics(
        r#"
        init {
            let selector = @evm_calldataload(0);
            let x = match 5 {
                selector => 1,
                else => 0,
            };
            @evm_stop();
        }
        "#,
        &[r#"
        error: attempting to evaluate runtime expression in comptime context
         --> main.plk:4:9
          |
        4 |         selector => 1,
          |         ^^^^^^^^ runtime expression
          |
          = note: match arm key must be known at compile time
        "#],
    );
}

#[test]
fn test_duplicate_match_key() {
    assert_diagnostics(
        std_project(
            r#"
        init {
            let x = match @evm_calldataload(0) {
                0x04 => 1,
                2 + 2 => 2,
                else => 0,
            };
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: duplicate match arm key
         --> main.plk:4:9
          |
        3 |         0x04 => 1,
          |         ---- previous key here
        4 |         2 + 2 => 2,
          |         ^^^^^^ key `4` is used more than once
        "#],
    );
}

#[test]
fn test_match_key_must_be_u256() {
    assert_diagnostics(
        r#"
        init {
            let x = match @evm_calldataload(0) {
                true => 1,
                else => 0,
            };
            @evm_stop();
        }
        "#,
        &[r#"
        error: unsupported match value type
         --> main.plk:3:9
          |
        3 |         true => 1,
          |         ^^^^ expected `u256`, got `bool`
          |
          = note: match expressions currently support only `u256` subjects and arm keys
        "#],
    );
}

#[test]
fn test_match_subject_must_be_u256() {
    assert_diagnostics(
        r#"
        init {
            let x = match true {
                1 => 1,
                else => 0,
            };
            @evm_stop();
        }
        "#,
        &[r#"
        error: unsupported match value type
         --> main.plk:2:19
          |
        2 |     let x = match true {
          |                   ^^^^ expected `u256`, got `bool`
          |
          = note: match expressions currently support only `u256` subjects and arm keys
        "#],
    );
}

#[test]
fn test_runtime_match_subject_must_be_u256() {
    assert_diagnostics(
        std_project(
            r#"
        init {
            let subject = @evm_calldataload(0) == 0;
            let x = match subject {
                1 => 1,
                else => 0,
            };
            @evm_stop();
        }
        "#,
        ),
        &[r#"
        error: unsupported match value type
         --> main.plk:3:19
          |
        3 |     let x = match subject {
          |                   ^^^^^^^ expected `u256`, got `bool`
          |
          = note: match expressions currently support only `u256` subjects and arm keys
        "#],
    );
}
