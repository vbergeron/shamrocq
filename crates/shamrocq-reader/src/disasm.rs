use std::collections::HashMap;

use shamrocq_bytecode::op;

use crate::header::{Global, parse_header};
use crate::scan::{build_labels, scan_code, FrameInfo};
use crate::util::{read_u8, read_u16le, read_i32le, escape_bytes};

pub struct Disassembly {
    pub version: u16,
    pub blob_len: usize,
    pub header_len: usize,
    pub code_len: usize,
    pub globals: Vec<Global>,
    pub tags: Vec<String>,
    pub items: Vec<Item>,
}

pub enum Item {
    FnLabel { addr: u16, name: String, comment: String },
    BranchLabel { addr: u16, name: String },
    Instr { addr: usize, mnemonic: &'static str, operands: String, annotation: String },
    MatchEntry { tag: u8, tag_name: Option<String>, arity: u8, target: u16 },
}

pub fn disassemble(blob: &[u8]) -> Result<Disassembly, String> {
    let (version, globals, tags, header_len) = parse_header(blob)?;
    let code = &blob[header_len..];

    let scan = scan_code(code)?;
    let (labels, frames) = build_labels(&globals, &scan);

    let mut sorted_labels: Vec<(u16, &str)> =
        labels.iter().map(|(k, v)| (*k, v.as_str())).collect();
    sorted_labels.sort_by_key(|(addr, _)| *addr);
    let mut next_label = 0usize;

    let mut n_captures: usize = 0;
    let mut n_params: usize = 0;
    let mut bind_depth: usize = 0;
    let mut bind_restore: HashMap<u16, usize> = HashMap::new();
    let mut branch_labels: HashMap<u16, String> = HashMap::new();

    let mut items = Vec::new();

    let mut pc = 0usize;
    while pc < code.len() {
        if let Some(&saved) = bind_restore.get(&(pc as u16)) {
            bind_depth = saved;
        }

        if let Some(bl) = branch_labels.remove(&(pc as u16)) {
            items.push(Item::BranchLabel { addr: pc as u16, name: bl });
        }

        while next_label < sorted_labels.len()
            && sorted_labels[next_label].0 as usize == pc
        {
            let (addr, name) = sorted_labels[next_label];
            let comment = label_comment(addr, &frames);
            items.push(Item::FnLabel { addr, name: name.to_string(), comment });
            if let Some(fi) = frames.get(&addr) {
                n_captures = fi.n_captures;
                n_params = fi.n_params;
            } else {
                n_captures = 0;
                n_params = 0;
            }
            bind_depth = 0;
            bind_restore.clear();
            next_label += 1;
        }

        let instr_pc = pc;
        let opcode = read_u8(code, pc)?;
        pc += 1;

        macro_rules! emit {
            ($mn:expr) => {
                items.push(Item::Instr {
                    addr: instr_pc, mnemonic: $mn,
                    operands: String::new(), annotation: String::new(),
                })
            };
            ($mn:expr, $ops:expr) => {
                items.push(Item::Instr {
                    addr: instr_pc, mnemonic: $mn,
                    operands: $ops, annotation: String::new(),
                })
            };
            ($mn:expr, $ops:expr, $ann:expr) => {
                items.push(Item::Instr {
                    addr: instr_pc, mnemonic: $mn,
                    operands: $ops, annotation: $ann,
                })
            };
        }

        match opcode {
            op::LOAD => {
                let idx = read_u8(code, pc)?;
                pc += 1;
                let ann = annotate_load(idx as usize, n_captures, n_params, bind_depth);
                emit!("LOAD", format!("{}", idx), ann);
            }
            op::LOAD2 => {
                let a = read_u8(code, pc)?;
                let b = read_u8(code, pc + 1)?;
                pc += 2;
                let aa = annotate_load(a as usize, n_captures, n_params, bind_depth);
                let ab = annotate_load(b as usize, n_captures, n_params, bind_depth);
                let ann = if aa.is_empty() && ab.is_empty() { String::new() } else { format!("{}, {}", aa, ab) };
                emit!("LOAD2", format!("{} {}", a, b), ann);
            }
            op::LOAD3 => {
                let a = read_u8(code, pc)?;
                let b = read_u8(code, pc + 1)?;
                let c = read_u8(code, pc + 2)?;
                pc += 3;
                let aa = annotate_load(a as usize, n_captures, n_params, bind_depth);
                let ab = annotate_load(b as usize, n_captures, n_params, bind_depth);
                let ac = annotate_load(c as usize, n_captures, n_params, bind_depth);
                let ann = if aa.is_empty() && ab.is_empty() && ac.is_empty() { String::new() } else { format!("{}, {}, {}", aa, ab, ac) };
                emit!("LOAD3", format!("{} {} {}", a, b, c), ann);
            }
            op::GLOBAL => {
                let idx = read_u16le(code, pc)?;
                pc += 2;
                let name = globals.get(idx as usize).map(|g| g.name.as_str()).unwrap_or("?");
                emit!("GLOBAL", format!("{}", idx), name.to_string());
            }
            op::DROP => {
                let n = read_u8(code, pc)?;
                pc += 1;
                bind_depth = bind_depth.saturating_sub(n as usize);
                emit!("DROP", format!("{}", n));
            }
            op::SLIDE1 => {
                bind_depth = bind_depth.saturating_sub(1);
                emit!("SLIDE1");
            }
            op::SLIDE => {
                let n = read_u8(code, pc)?;
                pc += 1;
                bind_depth = bind_depth.saturating_sub(n as usize);
                emit!("SLIDE", format!("{}", n));
            }
            op::PACK0 => {
                let tag = read_u8(code, pc)?;
                pc += 1;
                emit!("PACK0", fmt_tag_plain(tag, &tags));
            }
            op::PACK => {
                let tag = read_u8(code, pc)?;
                let arity = read_u8(code, pc + 1)?;
                pc += 2;
                emit!("PACK", format!("{} arity={}", fmt_tag_plain(tag, &tags), arity));
            }
            op::UNPACK => {
                let n = read_u8(code, pc)?;
                pc += 1;
                bind_depth += n as usize;
                emit!("UNPACK", format!("{}", n));
            }
            op::BIND => {
                let n = read_u8(code, pc)?;
                pc += 1;
                bind_depth += n as usize;
                emit!("BIND", format!("{}", n));
            }
            op::FOREIGN => {
                let idx = read_u16le(code, pc)?;
                let arity = read_u8(code, pc + 2)?;
                pc += 3;
                emit!("FOREIGN", format!("idx={} arity={}", idx, arity));
            }
            op::FUNCTION => {
                let addr = read_u16le(code, pc)?;
                let arity = read_u8(code, pc + 2)?;
                pc += 3;
                let lbl = label_at(addr, &labels);
                emit!("FUNCTION", format!("fn code+0x{:04X}{} arity={}", addr, lbl, arity));
            }
            op::CLOSURE => {
                let addr = read_u16le(code, pc)?;
                let arity = read_u8(code, pc + 2)?;
                let n_cap = read_u8(code, pc + 3)?;
                pc += 4;
                let lbl = label_at(addr, &labels);
                emit!("CLOSURE", format!("code+0x{:04X}{} arity={} bound={}", addr, lbl, arity, n_cap));
            }
            op::FIXPOINT => {
                let cap_idx = read_u8(code, pc)?;
                pc += 1;
                if cap_idx == 0xFF {
                    emit!("FIXPOINT", "(no self-capture)".into());
                } else {
                    emit!("FIXPOINT", format!("cap_idx={}", cap_idx));
                }
            }
            op::CALL1 => emit!("CALL1"),
            op::TAIL_CALL1 => emit!("TAIL_CALL1"),
            op::CALL_N => {
                let addr = read_u16le(code, pc)?;
                let n_args = read_u8(code, pc + 2)?;
                pc += 3;
                let lbl = label_at(addr, &labels);
                emit!("CALL_N", format!("code+0x{:04X}{} n_args={}", addr, lbl, n_args));
            }
            op::TAIL_CALL_N => {
                let addr = read_u16le(code, pc)?;
                let n_args = read_u8(code, pc + 2)?;
                pc += 3;
                let lbl = label_at(addr, &labels);
                emit!("TAIL_CALL_N", format!("code+0x{:04X}{} n_args={}", addr, lbl, n_args));
            }
            op::RET => emit!("RET"),
            op::MATCH2 => {
                let base_tag = read_u8(code, pc)?;
                pc += 1;
                let saved = bind_depth;
                emit!("MATCH2", format!("base_tag={}:", base_tag));
                for i in 0..2u8 {
                    let tag = base_tag + i;
                    let arity = read_u8(code, pc)?;
                    let offset = read_u16le(code, pc + 1)?;
                    pc += 3;
                    items.push(Item::MatchEntry {
                        tag, tag_name: tags.get(tag as usize).cloned(), arity, target: offset,
                    });
                    branch_labels.insert(offset, branch_name(tag, &tags));
                    bind_restore.insert(offset, saved);
                }
            }
            op::MATCH => {
                let base_tag = read_u8(code, pc)?;
                let n_entries = read_u8(code, pc + 1)? as usize;
                pc += 2;
                let saved = bind_depth;
                emit!("MATCH", format!("base_tag={} {} entries:", base_tag, n_entries));
                for i in 0..n_entries {
                    let tag = base_tag + i as u8;
                    let arity = read_u8(code, pc)?;
                    let offset = read_u16le(code, pc + 1)?;
                    pc += 3;
                    items.push(Item::MatchEntry {
                        tag, tag_name: tags.get(tag as usize).cloned(), arity, target: offset,
                    });
                    branch_labels.insert(offset, branch_name(tag, &tags));
                    bind_restore.insert(offset, saved);
                }
            }
            op::JMP => {
                let offset = read_u16le(code, pc)?;
                pc += 2;
                let lbl = label_at(offset, &labels);
                emit!("JMP", format!("code+0x{:04X}{}", offset, lbl));
            }
            op::ERROR => emit!("ERROR"),
            op::INT0 => emit!("INT0"),
            op::INT1 => emit!("INT1"),
            op::INT => {
                let value = read_i32le(code, pc)?;
                pc += 4;
                emit!("INT", format!("{}", value));
            }
            op::ADD => emit!("ADD"),
            op::SUB => emit!("SUB"),
            op::MUL => emit!("MUL"),
            op::DIV => emit!("DIV"),
            op::NEG => emit!("NEG"),
            op::EQ  => emit!("EQ"),
            op::LT  => emit!("LT"),
            op::BYTES => {
                let len = read_u8(code, pc)? as usize;
                pc += 1;
                if pc + len > code.len() {
                    return Err(format!("BYTES at {:04X}: data truncated (need {} bytes)", instr_pc, len));
                }
                let data = &code[pc..pc + len];
                pc += len;
                let display = escape_bytes(data);
                emit!("BYTES", format!("len={} {:?}", len, display));
            }
            op::BYTES_LEN    => emit!("BYTES_LEN"),
            op::BYTES_GET    => emit!("BYTES_GET"),
            op::BYTES_EQ     => emit!("BYTES_EQ"),
            op::BYTES_CONCAT => emit!("BYTES_CONCAT"),
            other => {
                return Err(format!("unknown opcode 0x{:02X} at code+0x{:04X}", other, instr_pc));
            }
        }
    }

    Ok(Disassembly {
        version,
        blob_len: blob.len(),
        header_len,
        code_len: code.len(),
        globals,
        tags,
        items,
    })
}

fn label_comment(addr: u16, frames: &HashMap<u16, FrameInfo>) -> String {
    match frames.get(&addr) {
        Some(fi) if fi.n_captures == 0 && fi.n_params == 1 => "function".into(),
        Some(fi) if fi.n_captures > 0 && fi.n_params == 1 => {
            format!("{} capture{}", fi.n_captures, if fi.n_captures > 1 { "s" } else { "" })
        }
        Some(fi) if fi.n_params > 1 => format!("direct, {} args", fi.n_params),
        _ => String::new(),
    }
}

fn annotate_load(idx: usize, n_captures: usize, n_params: usize, bind_depth: usize) -> String {
    if n_params == 0 && n_captures == 0 {
        return String::new();
    }
    if idx < n_captures {
        return format!("cap.{}", idx);
    }
    let after_cap = idx - n_captures;
    if after_cap < n_params {
        if n_params == 1 { return "arg".into(); }
        return format!("arg.{}", after_cap);
    }
    let let_idx = after_cap - n_params;
    if let_idx < bind_depth {
        return format!("let.{}", let_idx);
    }
    String::new()
}

fn branch_name(tag: u8, tags: &[String]) -> String {
    match tags.get(tag as usize) {
        Some(name) if !name.is_empty() => format!(".{}", name),
        _ => format!(".tag{}", tag),
    }
}

fn fmt_tag_plain(tag: u8, tags: &[String]) -> String {
    match tags.get(tag as usize) {
        Some(name) if !name.is_empty() => format!("tag={} ({})", tag, name),
        _ => format!("tag={}", tag),
    }
}

fn label_at(addr: u16, labels: &HashMap<u16, String>) -> String {
    match labels.get(&addr) {
        Some(name) => format!(" <{}>", name),
        None => String::new(),
    }
}
