use crate::color::C;
use crate::disasm::{Disassembly, Item};

pub fn print_disassembly(d: &Disassembly, c: &C) {
    println!("{}=== shamrocq bytecode ==={}", c.bld, c.rst);
    println!(
        "blob: {} bytes  header: {} bytes  code: {} bytes  version: {}  tags: {}",
        d.blob_len,
        d.header_len,
        d.code_len,
        d.version,
        if d.tags.is_empty() { "none".to_string() } else { format!("{} embedded", d.tags.len()) },
    );
    println!();

    println!("{}Global table{} ({} entries):", c.bld, c.rst, d.globals.len());
    for (i, g) in d.globals.iter().enumerate() {
        println!("  [{:>3}]  {:.<40}  code+0x{:04X}", i, g.name, g.offset);
    }
    println!();

    if !d.tags.is_empty() {
        println!("{}Tag table{} ({} entries):", c.bld, c.rst, d.tags.len());
        for (i, name) in d.tags.iter().enumerate() {
            println!("  [{:>3}]  {}", i, name);
        }
        println!();
    }

    println!("{}=== Code ==={}", c.bld, c.rst);

    for item in &d.items {
        match item {
            Item::FnLabel { addr, name, comment } => {
                println!();
                if comment.is_empty() {
                    println!("{}{:04X}{}  {}<{}>:{}", c.dim, addr, c.rst, c.cyn, name, c.rst);
                } else {
                    println!(
                        "{}{:04X}{}  {}<{}>:{} {}; {}{}",
                        c.dim, addr, c.rst, c.cyn, name, c.rst, c.dim, comment, c.rst
                    );
                }
            }
            Item::BranchLabel { addr, name } => {
                println!("  {}{:04X}{}  {}{}:{}", c.dim, addr, c.rst, c.ylw, name, c.rst);
            }
            Item::Instr { addr, mnemonic, operands, annotation } => {
                let ops = colorize_operands(operands, &d.tags, c);
                if annotation.is_empty() {
                    if ops.is_empty() {
                        println!("  {}{:04X}{}  {}{}{}", c.dim, addr, c.rst, c.bld, mnemonic, c.rst);
                    } else {
                        println!("  {}{:04X}{}  {}{:<13}{}{}", c.dim, addr, c.rst, c.bld, mnemonic, c.rst, ops);
                    }
                } else {
                    println!(
                        "  {}{:04X}{}  {}{:<13}{}{:<17}{}; {}{}",
                        c.dim, addr, c.rst, c.bld, mnemonic, c.rst,
                        ops, c.dim, annotation, c.rst
                    );
                }
            }
            Item::MatchEntry { tag, tag_name, arity, target } => {
                let tag_str = match tag_name {
                    Some(name) if !name.is_empty() => {
                        format!("tag={} {}({}){}", tag, c.grn, name, c.rst)
                    }
                    _ => format!("tag={}", tag),
                };
                println!("        {}|{} {} arity={} -> {:04X}", c.ylw, c.rst, tag_str, arity, target);
            }
        }
    }

    println!();
}

fn colorize_operands(ops: &str, tags: &[String], c: &C) -> String {
    if ops.is_empty() {
        return String::new();
    }
    let mut result = ops.to_string();
    for name in tags {
        if !name.is_empty() {
            let plain = format!("({})", name);
            let colored = format!("{}({}){}", c.grn, name, c.rst);
            result = result.replace(&plain, &colored);
        }
    }
    // Colorize label references: <label> -> color(<label>)
    // Build from scratch to avoid re-matching inserted escape sequences
    let mut out = String::new();
    let mut rest = result.as_str();
    while let Some(start) = rest.find('<') {
        if let Some(end) = rest[start..].find('>') {
            let label = &rest[start + 1..start + end];
            out.push_str(&rest[..start]);
            out.push_str(c.cyn);
            out.push('<');
            out.push_str(label);
            out.push('>');
            out.push_str(c.rst);
            rest = &rest[start + end + 1..];
        } else {
            break;
        }
    }
    out.push_str(rest);
    out
}
