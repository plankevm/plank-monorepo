use super::*;

#[test]
fn test_import_group_symbols_accessible() {
    assert_lowers_to(
        TestProject::root(
            r#"
            use m::other::{f, g as my_g};
            init {
                let x = f(1);
                let y = my_g(2, 3);
                @evm_stop();
            }
        "#,
        )
        .add_file(
            "other",
            r#"
            const f = fn(x: u256) u256 { return x; };
            const g = fn(a: u256, b: u256) u256 { return a; };
            "#,
        )
        .add_module("m", ""),
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        @fn1(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            ret %2
        }

        ; init
        @fn2() -> never {
            %0 : u256 = 1
            %1 : u256 = call @fn0(%0)
            %2 : u256 = 2
            %3 : u256 = 3
            %4 : u256 = call @fn1(%2, %3)
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_public_import_group_symbols_reexported() {
    assert_lowers_to(
        TestProject::root(
            r#"
            use m::facade::*;
            init {
                let x = f(1);
                let y = renamed(2, 3);
                @evm_stop();
            }
            "#,
        )
        .add_file("facade", "pub use m::leaf::{f, g as renamed};")
        .add_file(
            "leaf",
            r#"
            const f = fn(x: u256) u256 { return x; };
            const g = fn(a: u256, b: u256) u256 { return a; };
            "#,
        )
        .add_module("m", ""),
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        @fn1(%0: u256, %1: u256) -> u256 {
            %2 : u256 = %0
            ret %2
        }

        ; init
        @fn2() -> never {
            %0 : u256 = 1
            %1 : u256 = call @fn0(%0)
            %2 : u256 = 2
            %3 : u256 = 3
            %4 : u256 = call @fn1(%2, %3)
            %5 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_transitive_reexported_symbol_accessible() {
    assert_lowers_to(
        TestProject::root(
            r#"
            use m::prelude::renamed;
            init {
                let x = renamed(1);
                @evm_stop();
            }
            "#,
        )
        .add_file("prelude", "pub use m::middle::*;")
        .add_file("middle", "pub use m::other::f as renamed;")
        .add_file("other", "const f = fn(x: u256) u256 { return x; };")
        .add_module("m", ""),
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        ; init
        @fn1() -> never {
            %0 : u256 = 1
            %1 : u256 = call @fn0(%0)
            %2 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_empty_glob_reexport_cycle_exports_nothing() {
    assert_lowers_to(
        TestProject::root(
            r#"
            use m::a::*;
            init { @evm_stop(); }
            "#,
        )
        .add_file("a", "pub use m::b::*;")
        .add_file("b", "pub use m::a::*;")
        .add_module("m", ""),
        r#"
        ==== Functions ====
        ; init
        @fn0() -> never {
            %0 : never = @evm_stop()
        }
        "#,
    );
}

#[test]
fn test_mutual_reexports_with_concrete_definitions() {
    assert_lowers_to(
        TestProject::root(
            r#"
            use m::a::{x, y};
            init {
                let first = x(1);
                let second = y(2);
                @evm_stop();
            }
            "#,
        )
        .add_file(
            "a",
            r#"
            const x = fn(value: u256) u256 { return value; };
            pub use m::b::y;
            "#,
        )
        .add_file(
            "b",
            r#"
            const y = fn(value: u256) u256 { return value; };
            pub use m::a::x;
            "#,
        )
        .add_module("m", ""),
        r#"
        ==== Functions ====
        @fn0(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        @fn1(%0: u256) -> u256 {
            %1 : u256 = %0
            ret %1
        }

        ; init
        @fn2() -> never {
            %0 : u256 = 1
            %1 : u256 = call @fn0(%0)
            %2 : u256 = 2
            %3 : u256 = call @fn1(%2)
            %4 : never = @evm_stop()
        }
        "#,
    );
}
