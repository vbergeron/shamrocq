use std::collections::HashMap;
use std::path::PathBuf;

use clap::Parser;

/// Read and disassemble a shamrocq bytecode blob.
#[derive(Parser)]
#[command(name = "shamrocq-reader", version)]
struct Cli {
    /// Bytecode file to disassemble (e.g. bytecode.bin)
    file: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let blob = std::fs::read(&cli.file)
        .map_err(|e| format!("cannot read {}: {}", cli.file.display(), e))?;
    disassemble(&blob).map_err(|e| format!("disassembly error: {}", e))?;
    Ok(())
}

// ── opcodes ──────────────────────────────────────────────────────────────────

mod op {
    pub const CTOR0: u8 = 0x01;
    pub const CTOR: u8 = 0x02;
    pub const LOAD: u8 = 0x03;
    pub const GLOBAL: u8 = 0x04;
    pub const CLOSURE: u8 = 0x05;
    pub const CALL: u8 = 0x06;
    pub const TAIL_CALL: u8 = 0x07;
    pub const RET: u8 = 0x08;
    pub const MATCH: u8 = 0x09;
    pub const JMP: u8 = 0x0A;
    pub const BIND: u8 = 0x0B;
    pub const DROP: u8 = 0x0C;
    pub const ERROR: u8 = 0x0D;
    pub const SLIDE: u8 = 0x0E;
    pub const FIXPOINT: u8 = 0x0F;
    pub const INT_CONST: u8 = 0x10;
    pub const ADD: u8 = 0x11;
    pub const SUB: u8 = 0x12;
    pub const MUL: u8 = 0x13;
    pub const DIV: u8 = 0x14;
    pub const NEG: u8 = 0x15;
    pub const EQ: u8 = 0x16;
    pub const LT: u8 = 0x17;
    pub const BYTES_CONST: u8 = 0x18;
    pub const BYTES_LEN: u8 = 0x19;
    pub const BYTES_GET: u8 = 0x1A;
    pub const BYTES_EQ: u8 = 0x1B;
    pub const BYTES_CONCAT: u8 = 0x1C;
    pub const CALL_DIRECT: u8 = 0x1D;
    pub const TAIL_CALL_DIRECT: u8 = 0x1E;
    pub const FOREIGN_FN_CONST: u8 = 0x1F;
}

// ── header parsing ────────────────────────────────────────────────────────────

struct Global {
    name: String,
    /// Byte offset into the Code section.
    offset: u16,
}

fn parse_header(blob: &[u8]) -> Result<(Vec<Global>, usize), String> {
    let mut cursor = 0usize;

    let n_globals = read_u16le(blob, cursor)? as usize;
    cursor += 2;

    let mut globals = Vec::with_capacity(n_globals);
    for _ in 0..n_globals {
        let name_len = read_u8(blob, cursor)? as usize;
        cursor += 1;

        if cursor + name_len > blob.len() {
            return Err(format!(
                "header truncated: need {} name bytes at offset {}",
                name_len, cursor
            ));
        }
        let name = std::str::from_utf8(&blob[cursor..cursor + name_len])
            .map_err(|_| format!("non-UTF-8 name at offset {}", cursor))?
            .to_owned();
        cursor += name_len;

        let offset = read_u16le(blob, cursor)?;
        cursor += 2;

        globals.push(Global { name, offset });
    }

    Ok((globals, cursor))
}

// ── disassembly ───────────────────────────────────────────────────────────────

fn disassemble(blob: &[u8]) -> Result<(), String> {
    let (globals, header_len) = parse_header(blob)?;
    let code = &blob[header_len..];

    println!("=== shamrocq bytecode ===");
    println!(
        "blob: {} bytes  header: {} bytes  code: {} bytes",
        blob.len(),
        header_len,
        code.len()
    );
    println!();

    // Global table
    println!("Global table ({} entries):", globals.len());
    for (i, g) in globals.iter().enumerate() {
        println!("  [{:>3}]  {:.<40}  code+0x{:04X}", i, g.name, g.offset);
    }
    println!();

    // Build address -> name map for labelling jump targets / closure addrs.
    let label_map: HashMap<u16, &str> = globals
        .iter()
        .map(|g| (g.offset, g.name.as_str()))
        .collect();

    // Collect function boundaries: sort globals by code offset so we can
    // print a label whenever we cross a function entry point.
    let mut entries: Vec<(u16, &str)> = globals.iter().map(|g| (g.offset, g.name.as_str())).collect();
    entries.sort_by_key(|(off, _)| *off);

    println!("=== Code ===");
    println!();

    let mut pc = 0usize;
    let mut next_entry = 0usize; // index into `entries`

    while pc < code.len() {
        // Print function label if we're at a known entry point.
        while next_entry < entries.len() && entries[next_entry].0 as usize == pc {
            println!("{:04X}  <{}>:", pc, entries[next_entry].1);
            next_entry += 1;
        }

        let instr_pc = pc;
        let opcode = read_u8(code, pc)?;
        pc += 1;

        match opcode {
            op::CTOR0 => {
                let tag = read_u8(code, pc)?;
                pc += 1;
                println!("  {:04X}  CTOR0        tag={}", instr_pc, tag);
            }
            op::CTOR => {
                let tag = read_u8(code, pc)?;
                let arity = read_u8(code, pc + 1)?;
                pc += 2;
                println!("  {:04X}  CTOR         tag={} arity={}", instr_pc, tag, arity);
            }
            op::LOAD => {
                let idx = read_u8(code, pc)?;
                pc += 1;
                println!("  {:04X}  LOAD         {}", instr_pc, idx);
            }
            op::GLOBAL => {
                let idx = read_u16le(code, pc)?;
                pc += 2;
                // Resolve to name if possible
                let name = globals.get(idx as usize).map(|g| g.name.as_str()).unwrap_or("?");
                println!("  {:04X}  GLOBAL       {} ({})", instr_pc, idx, name);
            }
            op::CLOSURE => {
                let code_addr = read_u16le(code, pc)?;
                let n_captures = read_u8(code, pc + 2)?;
                pc += 3;
                let label = label_map.get(&code_addr).copied().unwrap_or("");
                if n_captures == 0 {
                    println!(
                        "  {:04X}  CLOSURE      bare_fn code+0x{:04X}{}",
                        instr_pc,
                        code_addr,
                        fmt_label(label)
                    );
                } else {
                    println!(
                        "  {:04X}  CLOSURE      code+0x{:04X}{} captures={}",
                        instr_pc,
                        code_addr,
                        fmt_label(label),
                        n_captures
                    );
                }
            }
            op::CALL => {
                println!("  {:04X}  CALL", instr_pc);
            }
            op::TAIL_CALL => {
                println!("  {:04X}  TAIL_CALL", instr_pc);
            }
            op::RET => {
                println!("  {:04X}  RET", instr_pc);
                // blank line after returns for readability
                println!();
            }
            op::MATCH => {
                let n_cases = read_u8(code, pc)? as usize;
                pc += 1;
                println!("  {:04X}  MATCH        {} cases:", instr_pc, n_cases);
                for i in 0..n_cases {
                    let tag = read_u8(code, pc)?;
                    let arity = read_u8(code, pc + 1)?;
                    let offset = read_u16le(code, pc + 2)?;
                    pc += 4;
                    let label = label_map.get(&offset).copied().unwrap_or("");
                    println!(
                        "             [{:>2}] tag={} arity={} -> code+0x{:04X}{}",
                        i,
                        tag,
                        arity,
                        offset,
                        fmt_label(label)
                    );
                }
            }
            op::JMP => {
                let offset = read_u16le(code, pc)?;
                pc += 2;
                let label = label_map.get(&offset).copied().unwrap_or("");
                println!(
                    "  {:04X}  JMP          code+0x{:04X}{}",
                    instr_pc,
                    offset,
                    fmt_label(label)
                );
            }
            op::BIND => {
                let n = read_u8(code, pc)?;
                pc += 1;
                println!("  {:04X}  BIND         {}", instr_pc, n);
            }
            op::DROP => {
                let n = read_u8(code, pc)?;
                pc += 1;
                println!("  {:04X}  DROP         {}", instr_pc, n);
            }
            op::ERROR => {
                println!("  {:04X}  ERROR", instr_pc);
            }
            op::SLIDE => {
                let n = read_u8(code, pc)?;
                pc += 1;
                println!("  {:04X}  SLIDE        {}", instr_pc, n);
            }
            op::FIXPOINT => {
                let cap_idx = read_u8(code, pc)?;
                pc += 1;
                if cap_idx == 0xFF {
                    println!("  {:04X}  FIXPOINT     (no self-capture)", instr_pc);
                } else {
                    println!("  {:04X}  FIXPOINT     cap_idx={}", instr_pc, cap_idx);
                }
            }
            op::INT_CONST => {
                let value = read_i32le(code, pc)?;
                pc += 4;
                println!("  {:04X}  INT_CONST    {}", instr_pc, value);
            }
            op::ADD => println!("  {:04X}  ADD", instr_pc),
            op::SUB => println!("  {:04X}  SUB", instr_pc),
            op::MUL => println!("  {:04X}  MUL", instr_pc),
            op::DIV => println!("  {:04X}  DIV", instr_pc),
            op::NEG => println!("  {:04X}  NEG", instr_pc),
            op::EQ  => println!("  {:04X}  EQ", instr_pc),
            op::LT  => println!("  {:04X}  LT", instr_pc),
            op::BYTES_CONST => {
                let len = read_u8(code, pc)? as usize;
                pc += 1;
                if pc + len > code.len() {
                    return Err(format!(
                        "BYTES_CONST at {:04X}: data truncated (need {} bytes)",
                        instr_pc, len
                    ));
                }
                let data = &code[pc..pc + len];
                pc += len;
                // Print as escaped ASCII
                let display = escape_bytes(data);
                println!("  {:04X}  BYTES_CONST  len={} {:?}", instr_pc, len, display);
            }
            op::BYTES_LEN    => println!("  {:04X}  BYTES_LEN", instr_pc),
            op::BYTES_GET    => println!("  {:04X}  BYTES_GET", instr_pc),
            op::BYTES_EQ     => println!("  {:04X}  BYTES_EQ", instr_pc),
            op::BYTES_CONCAT => println!("  {:04X}  BYTES_CONCAT", instr_pc),
            op::CALL_DIRECT => {
                let code_addr = read_u16le(code, pc)?;
                let n_args = read_u8(code, pc + 2)?;
                pc += 3;
                let label = label_map.get(&code_addr).copied().unwrap_or("");
                println!(
                    "  {:04X}  CALL_DIRECT  code+0x{:04X}{} args={}",
                    instr_pc,
                    code_addr,
                    fmt_label(label),
                    n_args
                );
            }
            op::TAIL_CALL_DIRECT => {
                let code_addr = read_u16le(code, pc)?;
                let n_args = read_u8(code, pc + 2)?;
                pc += 3;
                let label = label_map.get(&code_addr).copied().unwrap_or("");
                println!(
                    "  {:04X}  TAIL_CALL_DIRECT  code+0x{:04X}{} args={}",
                    instr_pc,
                    code_addr,
                    fmt_label(label),
                    n_args
                );
            }
            op::FOREIGN_FN_CONST => {
                let idx = read_u16le(code, pc)?;
                pc += 2;
                println!("  {:04X}  FOREIGN_FN_CONST  idx={}", instr_pc, idx);
            }
            other => {
                return Err(format!(
                    "unknown opcode 0x{:02X} at code+0x{:04X}",
                    other, instr_pc
                ));
            }
        }
    }

    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn read_u8(buf: &[u8], pos: usize) -> Result<u8, String> {
    buf.get(pos)
        .copied()
        .ok_or_else(|| format!("unexpected end of blob at byte {}", pos))
}

fn read_u16le(buf: &[u8], pos: usize) -> Result<u16, String> {
    if pos + 2 > buf.len() {
        return Err(format!("unexpected end of blob reading u16 at byte {}", pos));
    }
    Ok(u16::from_le_bytes([buf[pos], buf[pos + 1]]))
}

fn read_i32le(buf: &[u8], pos: usize) -> Result<i32, String> {
    if pos + 4 > buf.len() {
        return Err(format!("unexpected end of blob reading i32 at byte {}", pos));
    }
    Ok(i32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]))
}

/// Format a label suffix like ` <name>` when non-empty.
fn fmt_label(label: &str) -> String {
    if label.is_empty() {
        String::new()
    } else {
        format!(" <{}>", label)
    }
}

/// Produce a human-readable byte string (printable ASCII; otherwise hex escapes).
fn escape_bytes(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() + 2);
    for &b in data {
        if b.is_ascii_graphic() || b == b' ' {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\x{:02X}", b));
        }
    }
    out
}
