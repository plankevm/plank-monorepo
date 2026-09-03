use sir_stack_scheduling_common::{RepresentativeSchedule, RepresentativeStackOp};

const MAX_EVM_SWAP_DEPTH: u32 = 16;
const MAX_EVM_DUP_DEPTH: u32 = 16;

pub struct ParsedSchedule {
    pub schedule: RepresentativeSchedule,
    pub error: Option<String>,
}

pub fn parse(source: &str) -> ParsedSchedule {
    let mut operations = Vec::new();
    for (index, token) in source.split_whitespace().enumerate() {
        match parse_operation(token) {
            Ok(operation) => operations.push(operation),
            Err(error) => {
                return ParsedSchedule {
                    schedule: RepresentativeSchedule(operations.into_boxed_slice()),
                    error: Some(format!(
                        "stack operation {} ('{token}') is invalid: {error}",
                        index + 1
                    )),
                };
            }
        }
    }
    ParsedSchedule { schedule: RepresentativeSchedule(operations.into_boxed_slice()), error: None }
}

fn parse_operation(token: &str) -> Result<RepresentativeStackOp, String> {
    if token == "pop" {
        return Ok(RepresentativeStackOp::Pop);
    }
    if let Some(raw) = token.strip_prefix("swap") {
        let depth = parse_u32(raw, "swap depth")?;
        if !(1..=MAX_EVM_SWAP_DEPTH).contains(&depth) {
            return Err(format!("swap depth must be between 1 and {MAX_EVM_SWAP_DEPTH}"));
        }
        return Ok(RepresentativeStackOp::Swap {
            depth: depth.try_into().expect("validated swap depth fits in u8"),
        });
    }
    if let Some(raw) = token.strip_prefix("dup") {
        let depth = parse_u32(raw, "dup depth")?;
        if !(1..=MAX_EVM_DUP_DEPTH).contains(&depth) {
            return Err(format!("dup depth must be between 1 and {MAX_EVM_DUP_DEPTH}"));
        }
        return Ok(RepresentativeStackOp::Dup {
            depth: u8::try_from(depth.checked_sub(1).expect("validated dup depth is nonzero"))
                .expect("validated dup depth fits in u8"),
        });
    }
    if let Some(raw) = token.strip_prefix("store") {
        return Ok(RepresentativeStackOp::Store { slot: parse_u32(raw, "spill slot")? });
    }
    if let Some(raw) = token.strip_prefix("load") {
        return Ok(RepresentativeStackOp::Load { slot: parse_u32(raw, "spill slot")? });
    }

    let (token, flipped) =
        token.strip_suffix('f').map_or((token, false), |without_suffix| (without_suffix, true));
    if let Some(raw) = token.strip_prefix("op") {
        let operation = parse_u32(raw, "operation ID")?;
        return Ok(if flipped {
            RepresentativeStackOp::Flipped { operation }
        } else {
            RepresentativeStackOp::Op { operation }
        });
    }

    Err("expected swapN, dupN, pop, opN, opNf, storeN, or loadN".to_owned())
}

fn parse_u32(raw: &str, name: &str) -> Result<u32, String> {
    raw.parse().map_err(|_| format!("{name} must be an unsigned integer"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_whitespace_separated_operations_and_spills() {
        let parsed = parse("dup3 swap2\nop4f store0 load0 pop");
        assert_eq!(parsed.error, None);
        assert_eq!(
            parsed.schedule,
            RepresentativeSchedule(Box::new([
                RepresentativeStackOp::Dup { depth: 2 },
                RepresentativeStackOp::Swap { depth: 2 },
                RepresentativeStackOp::Flipped { operation: 4 },
                RepresentativeStackOp::Store { slot: 0 },
                RepresentativeStackOp::Load { slot: 0 },
                RepresentativeStackOp::Pop,
            ]))
        );
    }

    #[test]
    fn retains_the_valid_prefix_when_parsing_fails() {
        let parsed = parse("dup1 nope op0");
        assert_eq!(
            parsed.schedule,
            RepresentativeSchedule(Box::new([RepresentativeStackOp::Dup { depth: 0 }]))
        );
        assert_eq!(
            parsed.error.as_deref(),
            Some(
                "stack operation 2 ('nope') is invalid: expected swapN, dupN, pop, opN, opNf, storeN, or loadN"
            )
        );
    }
}
