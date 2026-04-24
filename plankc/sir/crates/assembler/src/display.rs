use crate::{AsmReference, AsmSection, Assembler, MarkReference, op};
use std::fmt::Write;

impl Assembler {
    pub fn write_asm(&self, f: &mut impl Write) -> std::fmt::Result {
        for section in self.iter_sections() {
            match section {
                AsmSection::Mark(id) => {
                    writeln!(f, ".mark{id}:")?;
                }
                AsmSection::Data(span) => {
                    let data = &self.bytes[span.start..span.end];
                    write!(f, "  data 0x").unwrap();
                    for &byte in data {
                        write!(f, "{byte:02x}").unwrap();
                    }
                    writeln!(f, " ({} bytes)", data.len()).unwrap();
                }
                AsmSection::Ops(span) => {
                    let ops = &self.bytes[span.start..span.end];
                    fmt_ops(f, ops);
                }
                AsmSection::MarkRef(asm_ref) => {
                    fmt_mark_ref(f, asm_ref);
                }
            }
        }

        Ok(())
    }
}

impl std::fmt::Display for Assembler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_asm(f)
    }
}

fn fmt_mark_ref(f: &mut impl Write, asm_ref: AsmReference) {
    let ref_str = match asm_ref.mark_ref {
        MarkReference::Direct(id) => format!(".mark{id}"),
        MarkReference::Delta(span) => {
            format!("(.mark{} - .mark{})", span.end, span.start)
        }
    };
    let size_str = asm_ref.set_size.map(|s| format!(":{}", s as u8)).unwrap_or_default();
    if asm_ref.pushed {
        writeln!(f, "  PUSH {ref_str}{size_str}").unwrap();
    } else {
        writeln!(f, "  RAW {ref_str}{size_str}").unwrap();
    }
}

fn fmt_ops(output: &mut impl Write, ops: &[u8]) {
    let mut i = 0;
    while i < ops.len() {
        let opcode = ops[i];
        let name = op::name(opcode);
        if let Some(push_size) = op::push_size(opcode) {
            let imm_start = i + 1;
            let imm_end = (imm_start + push_size as usize).min(ops.len());
            let imm = &ops[imm_start..imm_end];
            write!(output, "  {name} 0x").unwrap();
            for &byte in imm {
                write!(output, "{byte:02x}").unwrap();
            }
            if imm_end - imm_start < push_size as usize {
                write!(output, " (truncated)").unwrap();
            }
            writeln!(output).unwrap();
            i = imm_end;
        } else {
            writeln!(output, "  {name}").unwrap();
            i += 1;
        }
    }
}
