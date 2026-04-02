use crate::color::C;
use crate::util::{read_u16le, read_u32le};

pub fn display_dump(blob: &[u8], c: &C) -> Result<(), String> {
    if blob.len() < 4 + 2 + 4 + 4 + 4 + 2 {
        return Err("dump too short for header".into());
    }
    let version = read_u16le(blob, 4)?;
    let buf_len = read_u32le(blob, 6)? as usize;
    let heap_top = read_u32le(blob, 10)? as usize;
    let stack_bot = read_u32le(blob, 14)? as usize;
    let n_globals = read_u16le(blob, 18)? as usize;
    let mut pos = 20;

    if pos + n_globals * 4 > blob.len() {
        return Err("dump truncated in globals section".into());
    }
    let mut globals = Vec::with_capacity(n_globals);
    for _ in 0..n_globals {
        globals.push(read_u32le(blob, pos)?);
        pos += 4;
    }

    let stack_len = buf_len - stack_bot;
    if pos + heap_top + stack_len > blob.len() {
        return Err("dump truncated in heap/stack data".into());
    }
    let heap = &blob[pos..pos + heap_top];
    pos += heap_top;
    let stack = &blob[pos..pos + stack_len];

    println!("{}=== VM dump ==={}", c.bld, c.rst);
    println!(
        "version: {}  buf: {} bytes  heap: {} bytes  stack: {} bytes  free: {} bytes",
        version, buf_len, heap_top, stack_len, stack_bot - heap_top,
    );
    println!();

    println!("{}Globals{} ({} entries):", c.bld, c.rst, n_globals);
    for (i, &raw) in globals.iter().enumerate() {
        println!("  [{:>2}]  {}", i, fmt_value(raw, c));
    }
    println!();

    println!("{}Heap{} (0x0000..0x{:04X}, {} bytes):", c.bld, c.rst, heap_top, heap_top);
    for (i, chunk) in heap.chunks(4).enumerate() {
        if chunk.len() == 4 {
            let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            println!("  {}{:04X}{}  {:08X}  {}", c.dim, i * 4, c.rst, word, fmt_value(word, c));
        }
    }
    println!();

    println!(
        "{}Stack{} (0x{:04X}..0x{:04X}, {} bytes):",
        c.bld, c.rst, stack_bot, buf_len, stack_len,
    );
    for (i, chunk) in stack.chunks(4).enumerate() {
        if chunk.len() == 4 {
            let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            println!(
                "  {}{:04X}{}  {:08X}  {}",
                c.dim, stack_bot + i * 4, c.rst, word, fmt_value(word, c),
            );
        }
    }
    println!();

    Ok(())
}

fn fmt_value(raw: u32, c: &C) -> String {
    let kind = (raw >> 29) & 0x7;
    match kind {
        0b000 => {
            let tag = ((raw >> 21) & 0xFF) as u8;
            let offset = ((raw & 0x001F_FFFF) as usize) << 2;
            if offset == 0 && tag == 0 {
                format!("{}Ctor(tag=0, @0){}", c.dim, c.rst)
            } else {
                format!("{}Ctor{}(tag={}, @{})", c.grn, c.rst, tag, offset)
            }
        }
        0b001 => {
            let v = ((raw << 3) as i32) >> 3;
            format!("{}Int{}({})", c.ylw, c.rst, v)
        }
        0b010 => {
            let len = ((raw >> 21) & 0xFF) as u8;
            let offset = ((raw & 0x001F_FFFF) as usize) << 2;
            format!("{}Bytes{}(len={}, @{})", c.cyn, c.rst, len, offset)
        }
        0b110 => {
            let offset = ((raw & 0x001F_FFFF) as usize) << 2;
            format!("Closure(@{})", offset)
        }
        0b111 => {
            let foreign = (raw >> 20) & 1;
            let arity = ((raw >> 16) & 0xF) as u8;
            let addr = (raw & 0xFFFF) as u16;
            if foreign != 0 {
                format!("ForeignFn(idx={}, arity={})", addr, arity)
            } else {
                format!("Fn(pc={}, arity={})", addr, arity)
            }
        }
        _ => format!("{}{:08X}{}", c.dim, raw, c.rst),
    }
}
