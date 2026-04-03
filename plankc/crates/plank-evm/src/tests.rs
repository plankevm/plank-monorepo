use alloy_primitives::{I256, U256, uint};
use revm::{
    ExecuteEvm, MainBuilder, MainContext,
    bytecode::{Bytecode, opcode},
    context::{Context, TxEnv},
    database::CacheDB,
    database_interface::EmptyDB,
    primitives::{Address, TxKind},
    state::AccountInfo,
};

const TARGET: Address = Address::new([0xCC; 20]);

fn eval_op(args: &[U256], op: u8) -> U256 {
    let mut bytecode = Vec::new();
    for arg in args.iter().rev() {
        bytecode.push(opcode::PUSH32);
        bytecode.extend_from_slice(&arg.to_be_bytes::<32>());
    }
    bytecode.extend_from_slice(&[
        op,
        opcode::PUSH0,
        opcode::MSTORE,
        opcode::MSIZE,
        opcode::PUSH0,
        opcode::RETURN,
    ]);

    let mut db = CacheDB::<EmptyDB>::default();
    db.insert_account_info(TARGET, AccountInfo::from_bytecode(Bytecode::new_raw(bytecode.into())));

    let result = Context::mainnet()
        .with_db(db)
        .build_mainnet()
        .transact(TxEnv::builder().kind(TxKind::Call(TARGET)).build().unwrap())
        .unwrap();
    assert!(result.result.is_success());
    let data = result.result.into_output().unwrap();
    assert_eq!(data.len(), 32, "expected 32-byte return");
    U256::from_be_slice(&data)
}

fn assert_binop(op: u8, our_fn: fn(U256, U256) -> U256, cases: &[(U256, U256)]) {
    for &(a, b) in cases {
        let expected = eval_op(&[a, b], op);
        let actual = our_fn(a, b);
        assert_eq!(actual, expected, "mismatch for op 0x{op:02x} with a={a:#x}, b={b:#x}");
    }
}

fn assert_binop_bool(op: u8, our_fn: fn(U256, U256) -> bool, cases: &[(U256, U256)]) {
    for &(a, b) in cases {
        let expected = eval_op(&[a, b], op);
        let actual = U256::from(our_fn(a, b));
        assert_eq!(actual, expected, "mismatch for op 0x{op:02x} with a={a:#x}, b={b:#x}");
    }
}

fn assert_triop(op: u8, our_fn: fn(U256, U256, U256) -> U256, cases: &[(U256, U256, U256)]) {
    for &(a, b, c) in cases {
        let expected = eval_op(&[a, b, c], op);
        let actual = our_fn(a, b, c);
        assert_eq!(
            actual, expected,
            "mismatch for op 0x{op:02x} with a={a:#x}, b={b:#x}, c={c:#x}"
        );
    }
}

fn assert_unop(op: u8, our_fn: fn(U256) -> U256, cases: &[U256]) {
    for &a in cases {
        let expected = eval_op(&[a], op);
        let actual = our_fn(a);
        assert_eq!(actual, expected, "mismatch for op 0x{op:02x} with a={a:#x}");
    }
}

fn assert_unop_bool(op: u8, our_fn: fn(U256) -> bool, cases: &[U256]) {
    for &a in cases {
        let expected = eval_op(&[a], op);
        let actual = U256::from(our_fn(a));
        assert_eq!(actual, expected, "mismatch for op 0x{op:02x} with a={a:#x}");
    }
}

const BINARY_CASES: &[(U256, U256)] = &[
    (uint!(0_U256), uint!(0_U256)),
    (uint!(0_U256), uint!(1_U256)),
    (uint!(1_U256), uint!(0_U256)),
    (uint!(7_U256), uint!(3_U256)),
    (uint!(3_U256), uint!(7_U256)),
    (U256::MAX, U256::MAX),
    (U256::MAX, uint!(1_U256)),
    (uint!(1_U256), U256::MAX),
    (I256::MIN.into_raw(), I256::MIN.into_raw()),
    (U256::MAX, I256::MIN.into_raw()),
    (
        uint!(0xfedcba9812345678cafebabe00000000deadbeef_U256),
        uint!(0x4444444433333333222222221111111100000000_U256),
    ),
];

#[test]
fn test_add() {
    assert_binop(opcode::ADD, crate::add, BINARY_CASES);
}

#[test]
fn test_mul() {
    assert_binop(opcode::MUL, crate::mul, BINARY_CASES);
}

#[test]
fn test_sub() {
    assert_binop(opcode::SUB, crate::sub, BINARY_CASES);
}

#[test]
fn test_div() {
    assert_binop(opcode::DIV, crate::div, BINARY_CASES);
}

#[test]
fn test_sdiv() {
    assert_binop(opcode::SDIV, crate::sdiv, BINARY_CASES);
}

#[test]
fn test_mod() {
    assert_binop(opcode::MOD, crate::r#mod, BINARY_CASES);
}

#[test]
fn test_smod() {
    assert_binop(opcode::SMOD, crate::smod, BINARY_CASES);
}

const TERNARY_CASES: &[(U256, U256, U256)] = &[
    (uint!(0_U256), uint!(0_U256), uint!(0_U256)),
    (uint!(10_U256), uint!(20_U256), uint!(7_U256)),
    (U256::MAX, U256::MAX, uint!(0_U256)),
    (U256::MAX, U256::MAX, uint!(3_U256)),
    (U256::MAX, uint!(1_U256), uint!(2_U256)),
    (U256::MAX, U256::MAX, U256::MAX),
];

#[test]
fn test_addmod() {
    assert_triop(opcode::ADDMOD, crate::addmod, TERNARY_CASES);
}

#[test]
fn test_mulmod() {
    assert_triop(opcode::MULMOD, crate::mulmod, TERNARY_CASES);
}

#[test]
fn test_exp() {
    assert_binop(
        opcode::EXP,
        crate::exp,
        &[
            (uint!(0_U256), uint!(0_U256)),
            (uint!(2_U256), uint!(10_U256)),
            (uint!(2_U256), uint!(255_U256)),
            (uint!(2_U256), uint!(256_U256)),
            (U256::MAX, uint!(2_U256)),
            (uint!(3_U256), uint!(0_U256)),
        ],
    );
}

#[test]
fn test_signextend() {
    assert_binop(
        opcode::SIGNEXTEND,
        crate::signextend,
        &[
            (uint!(0_U256), uint!(0x7f_U256)),
            (uint!(0_U256), uint!(0x80_U256)),
            (uint!(0_U256), uint!(0xff_U256)),
            (uint!(1_U256), uint!(0x7fff_U256)),
            (uint!(1_U256), uint!(0x8000_U256)),
            (uint!(30_U256), U256::MAX),
            (uint!(31_U256), U256::MAX),
            (uint!(32_U256), U256::MAX),
            (U256::MAX, uint!(0x80_U256)),
        ],
    );
}

#[test]
fn test_lt() {
    assert_binop_bool(opcode::LT, crate::lt, BINARY_CASES);
}

#[test]
fn test_gt() {
    assert_binop_bool(opcode::GT, crate::gt, BINARY_CASES);
}

#[test]
fn test_slt() {
    assert_binop_bool(opcode::SLT, crate::slt, BINARY_CASES);
}

#[test]
fn test_sgt() {
    assert_binop_bool(opcode::SGT, crate::sgt, BINARY_CASES);
}

#[test]
fn test_eq() {
    assert_binop_bool(opcode::EQ, crate::eq, BINARY_CASES);
}

const UNARY_CASES: &[U256] = &[
    uint!(0_U256),
    uint!(1_U256),
    uint!(42_U256),
    U256::MAX,
    I256::MIN.into_raw(),
    uint!(0xfedcba9812345678cafebabe00000000deadbeef_U256),
];

#[test]
fn test_iszero() {
    assert_unop_bool(opcode::ISZERO, crate::iszero, UNARY_CASES);
}

#[test]
fn test_and() {
    assert_binop(opcode::AND, crate::and, BINARY_CASES);
}

#[test]
fn test_or() {
    assert_binop(opcode::OR, crate::or, BINARY_CASES);
}

#[test]
fn test_xor() {
    assert_binop(opcode::XOR, crate::xor, BINARY_CASES);
}

#[test]
fn test_not() {
    assert_unop(opcode::NOT, crate::not, UNARY_CASES);
}

#[test]
fn test_byte() {
    assert_binop(
        opcode::BYTE,
        crate::byte,
        &[
            (
                uint!(0_U256),
                uint!(0xab00000000000000000000000000000000000000000000000000000000000000_U256),
            ),
            (uint!(31_U256), uint!(0xab_U256)),
            (uint!(32_U256), U256::MAX),
            (uint!(33_U256), U256::MAX),
            (U256::MAX, U256::MAX),
            (uint!(15_U256), U256::MAX),
        ],
    );
}

const SHIFT_CASES: &[(U256, U256)] = &[
    (uint!(0_U256), uint!(0xff_U256)),
    (uint!(1_U256), uint!(0xff_U256)),
    (uint!(8_U256), uint!(0xabcd_U256)),
    (uint!(255_U256), U256::MAX),
    (uint!(256_U256), U256::MAX),
    (uint!(257_U256), U256::MAX),
    (U256::MAX, U256::MAX),
    (uint!(4_U256), I256::MIN.into_raw()),
];

#[test]
fn test_shl() {
    assert_binop(opcode::SHL, crate::shl, SHIFT_CASES);
}

#[test]
fn test_shr() {
    assert_binop(opcode::SHR, crate::shr, SHIFT_CASES);
}

#[test]
fn test_sar() {
    assert_binop(opcode::SAR, crate::sar, SHIFT_CASES);
}
