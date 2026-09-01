use std::io::Read;

use clap::Parser;
use sir_evm_lifter::{
    cfg::build_provisional_cfg, classify::classify, decode, icall::infer_internal_calls,
    lower::lower_to_sir, ownership::analyze_ownership, primitive_blocks::build_primitive_blocks,
    ssa::build_ssa, verify::verify,
};

#[derive(Debug, Parser)]
#[command(about = "Inspect the EVM bytecode-to-SIR lifting pipeline")]
struct Args {
    /// Hex bytecode. Reads from stdin when omitted.
    bytecode: Option<String>,

    /// Decoded EVM instructions and their program counters.
    #[arg(long)]
    decoded: bool,

    /// Naive blocks split at jumps, jump destinations, and terminators.
    #[arg(long)]
    primitive_blocks: bool,

    /// Heuristically inferred internal calls, returns, and call-aware blocks.
    #[arg(long)]
    internal_calls: bool,

    /// Provisional intrafunction control-flow edges.
    #[arg(long)]
    cfg: bool,

    /// Candidate functions, per-block function contexts, and data candidates.
    #[arg(long)]
    ownership: bool,

    /// Verified function arities, stack states, and return-destination pushes.
    #[arg(long)]
    verification: bool,

    /// Final reachable-code and unreachable-data section classification.
    #[arg(long)]
    classification: bool,

    /// Block-local SSA reconstructed from the verified EVM stacks.
    #[arg(long)]
    ssa: bool,

    /// Final legalized SIR program.
    #[arg(long)]
    sir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Decoded,
    PrimitiveBlocks,
    InternalCalls,
    Cfg,
    Ownership,
    Verification,
    Classification,
    Ssa,
    Sir,
}

impl Args {
    fn any_stage_selected(&self) -> bool {
        self.decoded
            || self.primitive_blocks
            || self.internal_calls
            || self.cfg
            || self.ownership
            || self.verification
            || self.classification
            || self.ssa
            || self.sir
    }

    fn displays(&self, stage: Stage) -> bool {
        if !self.any_stage_selected() {
            return true;
        }
        match stage {
            Stage::Decoded => self.decoded,
            Stage::PrimitiveBlocks => self.primitive_blocks,
            Stage::InternalCalls => self.internal_calls,
            Stage::Cfg => self.cfg,
            Stage::Ownership => self.ownership,
            Stage::Verification => self.verification,
            Stage::Classification => self.classification,
            Stage::Ssa => self.ssa,
            Stage::Sir => self.sir,
        }
    }

    fn last_stage(&self) -> Stage {
        if !self.any_stage_selected() {
            return Stage::Sir;
        }
        [
            (self.decoded, Stage::Decoded),
            (self.primitive_blocks, Stage::PrimitiveBlocks),
            (self.internal_calls, Stage::InternalCalls),
            (self.cfg, Stage::Cfg),
            (self.ownership, Stage::Ownership),
            (self.verification, Stage::Verification),
            (self.classification, Stage::Classification),
            (self.ssa, Stage::Ssa),
            (self.sir, Stage::Sir),
        ]
        .into_iter()
        .filter_map(|(selected, stage)| selected.then_some(stage))
        .max()
        .expect("at least one stage is selected")
    }
}

fn main() {
    let args = Args::parse();
    let input = match args.bytecode.as_deref() {
        Some(input) => input.to_owned(),
        None => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).expect("failed to read stdin");
            input
        }
    };
    let input = input.trim();
    let input = input.strip_prefix("0x").or_else(|| input.strip_prefix("0X")).unwrap_or(input);
    let bytecode = alloy_primitives::hex::decode(input).unwrap_or_else(|error| {
        eprintln!("invalid hex bytecode: {error}");
        std::process::exit(2);
    });

    let last_stage = args.last_stage();

    let decoded = decode(&bytecode).unwrap_or_else(|error| {
        eprintln!("failed to decode bytecode: {error}");
        std::process::exit(1);
    });
    if args.displays(Stage::Decoded) {
        render_stage("decoded", &decoded);
    }
    if last_stage == Stage::Decoded {
        return;
    }

    let primitive = build_primitive_blocks(&decoded);
    if args.displays(Stage::PrimitiveBlocks) {
        render_stage("primitive blocks", primitive.display(&decoded));
    }
    if last_stage == Stage::PrimitiveBlocks {
        return;
    }

    let inference = infer_internal_calls(&decoded, &primitive);
    if args.displays(Stage::InternalCalls) {
        render_stage("internal calls", inference.display(&decoded));
    }
    if last_stage == Stage::InternalCalls {
        return;
    }

    let cfg = build_provisional_cfg(&decoded, &inference);
    if args.displays(Stage::Cfg) {
        render_stage("provisional cfg", cfg.display(&decoded, &inference));
    }
    if last_stage == Stage::Cfg {
        return;
    }

    let ownership = analyze_ownership(&inference, &cfg).unwrap_or_else(|error| {
        eprintln!("ownership analysis failed: {error}");
        std::process::exit(1);
    });
    if args.displays(Stage::Ownership) {
        render_stage("ownership", ownership.display(&inference));
    }
    if last_stage == Stage::Ownership {
        return;
    }

    let verification = verify(&decoded, &inference, &cfg, &ownership).unwrap_or_else(|error| {
        eprintln!("verification failed: {error}");
        std::process::exit(1);
    });
    if args.displays(Stage::Verification) {
        render_stage("verification", verification.display(&ownership));
    }
    if last_stage == Stage::Verification {
        return;
    }

    let classified = classify(&decoded, &inference, &ownership);
    if args.displays(Stage::Classification) {
        render_stage("classification", classified.display(&decoded));
    }
    if last_stage == Stage::Classification {
        return;
    }

    let ssa =
        build_ssa(&decoded, &inference, &cfg, &ownership, &verification).unwrap_or_else(|error| {
            eprintln!("SSA construction failed: {error}");
            std::process::exit(1);
        });
    if args.displays(Stage::Ssa) {
        render_stage("SSA", &ssa);
    }
    if last_stage == Stage::Ssa {
        return;
    }

    let lifted =
        lower_to_sir(&decoded, &classified, &ssa, verification.postorder(), ownership.root())
            .unwrap_or_else(|error| {
                eprintln!("SIR lowering failed: {error}");
                std::process::exit(1);
            });
    if args.displays(Stage::Sir) {
        render_stage("SIR", &lifted.program);
    }
}

fn render_stage(name: &str, value: impl std::fmt::Display) {
    print!("== {name} ==\n{value}\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_stages_control_display_and_execution_limit() {
        let args = Args::try_parse_from(["evm-lifter", "--cfg", "--ownership", "0x00"]).unwrap();
        assert!(!args.displays(Stage::Decoded));
        assert!(args.displays(Stage::Cfg));
        assert!(args.displays(Stage::Ownership));
        assert_eq!(args.last_stage(), Stage::Ownership);
    }

    #[test]
    fn no_stage_flags_select_every_stage() {
        let args = Args::try_parse_from(["evm-lifter", "0x00"]).unwrap();
        assert!(args.displays(Stage::Decoded));
        assert!(args.displays(Stage::Sir));
        assert_eq!(args.last_stage(), Stage::Sir);
    }
}
