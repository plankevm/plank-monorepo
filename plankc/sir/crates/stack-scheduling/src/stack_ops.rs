use serde::{Deserialize, Serialize};
use sir_data::{OperationIdx, StaticAllocId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackOps {
    Swap(u8),
    Dup(u8),
    Pop,
    Flipped(#[serde(with = "index_serde")] OperationIdx),
    Op(#[serde(with = "index_serde")] OperationIdx),
    CallRetPush(#[serde(with = "index_serde")] OperationIdx),
    Exchange(u8, u8),
    Store(#[serde(with = "index_serde")] StaticAllocId),
    Load(#[serde(with = "index_serde")] StaticAllocId),
}

impl std::fmt::Display for StackOps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StackOps::Swap(depth) => write!(f, "Swap({depth})"),
            StackOps::Dup(depth) => write!(f, "Dup({depth})"),
            StackOps::Pop => write!(f, "Pop"),
            StackOps::Flipped(id) => write!(f, "flipped({id})"),
            StackOps::Op(id) => write!(f, "op({id})"),
            StackOps::CallRetPush(id) => write!(f, "call_ret_push({id})"),
            StackOps::Exchange(a, b) => write!(f, "Exchange({a}, {b})"),
            StackOps::Store(id) => write!(f, "store({id})"),
            StackOps::Load(id) => write!(f, "load({id})"),
        }
    }
}

impl StackOps {
    pub fn is_valid(self, config: ShuffleConfig) -> bool {
        match self {
            StackOps::Swap(depth) => depth > 0 && depth <= config.max_swap_depth,
            StackOps::Dup(depth) => depth <= config.max_dup_depth,
            StackOps::Exchange(n, m) => {
                n != m && n.checked_add(m).is_some_and(|sum| sum <= config.max_exchange_range)
            }
            StackOps::Flipped(_)
            | StackOps::Op(_)
            | StackOps::Pop
            | StackOps::Store(_)
            | StackOps::Load(_)
            | StackOps::CallRetPush(_) => true,
        }
    }

    pub const fn gas_cost(self, config: ShuffleConfig) -> u8 {
        match self {
            StackOps::Swap(_) | StackOps::Dup(_) | StackOps::Pop => 3,
            StackOps::Exchange(_, _) => config.exchange_cost,
            // Conservatively include memory expansion in the price of the first spill.
            StackOps::Store(_) => 9,
            StackOps::Load(_) => 6,
            StackOps::Flipped(_) | StackOps::Op(_) | StackOps::CallRetPush(_) => 0,
        }
    }
}

pub fn gas_cost(ops: &[StackOps], config: ShuffleConfig) -> u64 {
    ops.iter().map(|&operation| u64::from(operation.gas_cost(config))).sum()
}

pub struct ParsedStackOps {
    pub operations: Box<[StackOps]>,
    pub error: Option<String>,
}

pub fn parse_stack_ops(source: &str, config: ShuffleConfig) -> ParsedStackOps {
    let mut operations = Vec::new();
    for (index, token) in source.split_whitespace().enumerate() {
        match parse_stack_op(token, config) {
            Ok(operation) => operations.push(operation),
            Err(error) => {
                return ParsedStackOps {
                    operations: operations.into_boxed_slice(),
                    error: Some(format!(
                        "stack operation {} ('{token}') is invalid: {error}",
                        index + 1
                    )),
                };
            }
        }
    }
    ParsedStackOps { operations: operations.into_boxed_slice(), error: None }
}

fn parse_stack_op(token: &str, config: ShuffleConfig) -> Result<StackOps, String> {
    if token == "pop" {
        return Ok(StackOps::Pop);
    }
    if let Some(raw) = token.strip_prefix("swap") {
        let depth = parse_u32(raw, "swap depth")?;
        if depth == 0 || depth > u32::from(config.max_swap_depth) {
            return Err(format!("swap depth must be between 1 and {}", config.max_swap_depth));
        }
        return Ok(StackOps::Swap(depth.try_into().expect("validated swap depth fits in u8")));
    }
    if let Some(raw) = token.strip_prefix("dup") {
        let depth = parse_u32(raw, "dup depth")?;
        let maximum = u32::from(config.max_dup_depth) + 1;
        if depth == 0 || depth > maximum {
            return Err(format!("dup depth must be between 1 and {maximum}"));
        }
        return Ok(StackOps::Dup(u8::try_from(depth - 1).expect("validated dup depth fits in u8")));
    }
    if let Some(raw) = token.strip_prefix("store") {
        let raw = parse_u32(raw, "spill slot")?;
        return Ok(StackOps::Store(
            StaticAllocId::try_new(raw).ok_or("spill slot is out of range")?,
        ));
    }
    if let Some(raw) = token.strip_prefix("load") {
        let raw = parse_u32(raw, "spill slot")?;
        return Ok(StackOps::Load(
            StaticAllocId::try_new(raw).ok_or("spill slot is out of range")?,
        ));
    }

    let (token, flipped) =
        token.strip_suffix('f').map_or((token, false), |without_suffix| (without_suffix, true));
    if let Some(raw) = token.strip_prefix("op") {
        let raw = parse_u32(raw, "operation ID")?;
        let operation = OperationIdx::try_new(raw).ok_or("operation ID is out of range")?;
        return Ok(if flipped { StackOps::Flipped(operation) } else { StackOps::Op(operation) });
    }

    Err("expected swapN, dupN, pop, opN, opNf, storeN, or loadN".to_owned())
}

fn parse_u32(raw: &str, name: &str) -> Result<u32, String> {
    raw.parse().map_err(|_| format!("{name} must be an unsigned integer"))
}

mod index_serde {
    use plank_core::Idx;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<I: Idx, S: Serializer>(value: &I, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(value.get())
    }

    pub fn deserialize<'de, I: Idx, D: Deserializer<'de>>(deserializer: D) -> Result<I, D::Error> {
        let value = u32::deserialize(deserializer)?;
        I::try_from(value).map_err(|_| D::Error::custom("index is out of range"))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShuffleConfig {
    pub max_swap_depth: u8,
    pub max_dup_depth: u8,
    /// Given 0-indexed stack depths `m`, `n`, the `max_exchange_range` represents the constraints
    /// such that all valid `(m, n)` must satisfy `m + n <= max_exchange_range`
    pub max_exchange_range: u8,
    pub exchange_cost: u8,
}

impl ShuffleConfig {
    pub const PRE_AMSTERDAM: Self = Self::max_swap_no_exchange(16);

    pub const fn max_swap_no_exchange(max_swap_depth: u8) -> Self {
        Self {
            max_swap_depth,
            max_dup_depth: max_swap_depth.checked_sub(1).expect("dup depth underflow"),
            max_exchange_range: max_swap_depth,
            exchange_cost: 9,
        }
    }
}

impl Default for ShuffleConfig {
    fn default() -> Self {
        Self::PRE_AMSTERDAM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stack_operations_and_retains_an_invalid_prefix() {
        let parsed =
            parse_stack_ops("dup3 swap2\nop4f store0 load0 pop", ShuffleConfig::PRE_AMSTERDAM);
        assert_eq!(parsed.error, None);
        assert_eq!(
            parsed.operations.as_ref(),
            &[
                StackOps::Dup(2),
                StackOps::Swap(2),
                StackOps::Flipped(OperationIdx::new(4)),
                StackOps::Store(StaticAllocId::new(0)),
                StackOps::Load(StaticAllocId::new(0)),
                StackOps::Pop,
            ]
        );

        let parsed = parse_stack_ops("dup1 nope op0", ShuffleConfig::PRE_AMSTERDAM);
        assert_eq!(parsed.operations.as_ref(), &[StackOps::Dup(0)]);
        assert_eq!(
            parsed.error.as_deref(),
            Some(
                "stack operation 2 ('nope') is invalid: expected swapN, dupN, pop, opN, opNf, storeN, or loadN"
            )
        );
    }

    #[test]
    fn stack_operations_json_round_trip() {
        let operations: Box<[StackOps]> = Box::new([
            StackOps::Swap(1),
            StackOps::Dup(2),
            StackOps::Pop,
            StackOps::Flipped(OperationIdx::new(0)),
            StackOps::Op(OperationIdx::new(1)),
            StackOps::CallRetPush(OperationIdx::new(2)),
            StackOps::Exchange(3, 4),
            StackOps::Store(StaticAllocId::new(0)),
            StackOps::Load(StaticAllocId::new(1)),
        ]);
        let encoded = serde_json::to_string(&operations).unwrap();
        assert_eq!(
            encoded,
            r#"[{"swap":1},{"dup":2},"pop",{"flipped":0},{"op":1},{"call_ret_push":2},{"exchange":[3,4]},{"store":0},{"load":1}]"#
        );
        assert_eq!(serde_json::from_str::<Box<[StackOps]>>(&encoded).unwrap(), operations);
        assert_eq!(
            serde_json::from_str::<StackOps>(r#"{"op":4294967295}"#).unwrap_err().to_string(),
            "index is out of range"
        );
    }
}
