use std::collections::HashMap;

use shamrocq_bytecode::op;

use crate::header::Global;

pub struct ClosureRef {
    pub pc: usize,
    pub target: u16,
    pub arity: u8,
    pub n_captures: u8,
}

pub struct FrameInfo {
    pub n_captures: usize,
    pub n_params: usize,
}

pub struct ScanResult {
    pub closures: Vec<ClosureRef>,
}

pub fn scan_code(code: &[u8]) -> Result<ScanResult, String> {
    let mut closures = Vec::new();

    let mut pc = 0usize;
    while pc < code.len() {
        let instr_pc = pc;
        let opcode = code[pc];
        pc += 1;

        match opcode {
            op::LOAD => { pc += 1; }
            op::LOAD2 => { pc += 2; }
            op::LOAD3 => { pc += 3; }
            op::GLOBAL => { pc += 2; }
            op::DROP => { pc += 1; }
            op::SLIDE => { pc += 1; }
            op::PACK0 => { pc += 1; }
            op::PACK => { pc += 2; }
            op::UNPACK => { pc += 1; }
            op::BIND => { pc += 1; }
            op::FOREIGN => { pc += 3; }
            op::FUNCTION => {
                let target = u16::from_le_bytes([code[pc], code[pc + 1]]);
                let arity = code[pc + 2];
                pc += 3;
                closures.push(ClosureRef { pc: instr_pc, target, arity, n_captures: 0 });
            }
            op::CLOSURE => {
                let target = u16::from_le_bytes([code[pc], code[pc + 1]]);
                let arity = code[pc + 2];
                let n_captures = code[pc + 3];
                pc += 4;
                closures.push(ClosureRef { pc: instr_pc, target, arity, n_captures });
            }
            op::FIXPOINT => { pc += 1; }
            op::CALL1 => {}
            op::TAIL_CALL1 => {}
            op::CALL_N => { pc += 3; }
            op::TAIL_CALL_N => { pc += 3; }
            op::RET => {}
            op::MATCH2 => {
                pc += 1;
                pc += 2 * 3;
            }
            op::MATCH => {
                pc += 1;
                let n_entries = code[pc] as usize;
                pc += 1;
                pc += n_entries * 3;
            }
            op::JMP => { pc += 2; }
            op::ERROR => {}
            op::INT0 | op::INT1 => {}
            op::INT => { pc += 4; }
            op::ADD | op::SUB | op::MUL | op::DIV | op::NEG | op::EQ | op::LT | op::SLIDE1 => {}
            op::BYTES => {
                let len = code[pc] as usize;
                pc += 1 + len;
            }
            op::BYTES_LEN | op::BYTES_GET | op::BYTES_EQ | op::BYTES_CONCAT => {}
            other => {
                return Err(format!(
                    "unknown opcode 0x{:02X} at code+0x{:04X}",
                    other, instr_pc
                ));
            }
        }
    }

    Ok(ScanResult { closures })
}

pub fn build_labels(
    globals: &[Global],
    scan: &ScanResult,
) -> (HashMap<u16, String>, HashMap<u16, FrameInfo>) {
    let mut labels: HashMap<u16, String> = HashMap::new();
    let mut frames: HashMap<u16, FrameInfo> = HashMap::new();

    for g in globals {
        labels.insert(g.offset, g.name.clone());
    }

    let mut sorted_closures: Vec<&ClosureRef> = scan.closures.iter().collect();
    sorted_closures.sort_by_key(|c| c.pc);

    let mut child_counts: HashMap<u16, usize> = HashMap::new();

    for cl in &sorted_closures {
        if labels.contains_key(&cl.target) {
            continue;
        }
        let parent = labels
            .iter()
            .filter(|(&addr, _)| (addr as usize) <= cl.pc)
            .max_by_key(|(&addr, _)| addr);

        if let Some((&parent_addr, parent_label)) = parent {
            let n = child_counts.entry(parent_addr).or_insert(0);
            let child_label = format!("{}/{}", parent_label, n);
            *n += 1;
            frames.insert(
                cl.target,
                FrameInfo {
                    n_captures: cl.n_captures as usize,
                    n_params: (cl.arity as usize).saturating_sub(cl.n_captures as usize),
                },
            );
            labels.insert(cl.target, child_label);
        }
    }

    (labels, frames)
}
