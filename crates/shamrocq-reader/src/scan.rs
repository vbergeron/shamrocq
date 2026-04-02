use std::collections::HashMap;

use shamrocq_bytecode::op;

use crate::header::Global;

pub struct ClosureRef {
    pub pc: usize,
    pub target: u16,
    pub arity: u8,
    pub n_captures: u8,
}

pub struct DirectCallRef {
    pub target: u16,
    pub n_args: u8,
}

pub struct FrameInfo {
    pub n_captures: usize,
    pub n_params: usize,
}

pub struct ScanResult {
    pub closures: Vec<ClosureRef>,
    pub direct_calls: Vec<DirectCallRef>,
}

pub fn scan_code(code: &[u8]) -> Result<ScanResult, String> {
    let mut closures = Vec::new();
    let mut direct_calls = Vec::new();

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
            op::CALL_DYNAMIC => {}
            op::TAIL_CALL_DYNAMIC => {}
            op::CALL => {
                let target = u16::from_le_bytes([code[pc], code[pc + 1]]);
                let n_args = code[pc + 2];
                pc += 3;
                direct_calls.push(DirectCallRef { target, n_args });
            }
            op::TAIL_CALL => {
                let target = u16::from_le_bytes([code[pc], code[pc + 1]]);
                let n_args = code[pc + 2];
                pc += 3;
                direct_calls.push(DirectCallRef { target, n_args });
            }
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

    Ok(ScanResult { closures, direct_calls })
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

    // Identify globals whose stub is a FUNCTION instruction with arity >= 2.
    // The compiler emits flat (direct multi-arg) entry points for these, in
    // global-table order. We match them positionally against unlabeled
    // CALL/TAIL_CALL targets sorted by address.
    let mut flat_globals: Vec<(&str, u8)> = Vec::new();
    for g in globals {
        // The stub at g.offset should be a FUNCTION instruction for lambda globals.
        if let Some(cl) = scan.closures.iter().find(|c| c.pc as u16 == g.offset && c.n_captures == 0) {
            if cl.arity >= 2 {
                flat_globals.push((&g.name, cl.arity));
            }
        }
    }

    let mut unlabeled_targets: Vec<u16> = scan.direct_calls.iter()
        .map(|dc| dc.target)
        .filter(|t| !labels.contains_key(t))
        .collect();
    unlabeled_targets.sort();
    unlabeled_targets.dedup();

    for (i, &target) in unlabeled_targets.iter().enumerate() {
        let (name, n_params) = if i < flat_globals.len() {
            let (gname, arity) = flat_globals[i];
            (format!("{}$direct", gname), arity as usize)
        } else {
            let n_args = scan.direct_calls.iter()
                .find(|dc| dc.target == target)
                .map(|dc| dc.n_args as usize)
                .unwrap_or(0);
            (format!("direct@{:04X}", target), n_args)
        };
        frames.insert(target, FrameInfo { n_captures: 0, n_params });
        labels.insert(target, name);
    }

    (labels, frames)
}
