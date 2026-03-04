use crate::{AsmReference, AsmSection, Assembler, MarkReference, op};
use std::fmt::Write;

impl Assembler {
    pub fn display_asm(&self) -> String {
        let mut output = String::new();

        for section in self.iter_sections() {
            match section {
                AsmSection::Mark(id) => {
                    writeln!(&mut output, ".mark{id}:").unwrap();
                }
                AsmSection::Data(span) => {
                    let data = &self.bytes[span.start..span.end];
                    write!(&mut output, "  data 0x").unwrap();
                    for &byte in data {
                        write!(&mut output, "{byte:02x}").unwrap();
                    }
                    writeln!(&mut output, " ({} bytes)", data.len()).unwrap();
                }
                AsmSection::Ops(span) => {
                    let ops = &self.bytes[span.start..span.end];
                    fmt_ops(&mut output, ops);
                }
                AsmSection::MarkRef(asm_ref) => {
                    fmt_mark_ref(&mut output, asm_ref);
                }
            }
        }

        output
    }
}

fn fmt_mark_ref(output: &mut String, asm_ref: AsmReference) {
    let ref_str = match asm_ref.mark_ref {
        MarkReference::Direct(id) => format!(".mark{id}"),
        MarkReference::Delta(span) => {
            format!("(.mark{} - .mark{})", span.end, span.start)
        }
    };
    let size_str = asm_ref.set_size.map(|s| format!(":{}", s as u8)).unwrap_or_default();
    if asm_ref.pushed {
        writeln!(output, "  PUSH {ref_str}{size_str}").unwrap();
    } else {
        writeln!(output, "  RAW {ref_str}{size_str}").unwrap();
    }
}

fn fmt_ops(output: &mut String, ops: &[u8]) {
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
