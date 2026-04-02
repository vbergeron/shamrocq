use crate::util::{read_u8, read_u16le};

pub struct Global {
    pub name: String,
    pub offset: u16,
}

pub fn parse_header(blob: &[u8]) -> Result<(u16, Vec<Global>, Vec<String>, usize), String> {
    if blob.len() < 8 {
        return Err("blob too short for header".to_string());
    }
    if blob[0..4] != shamrocq_bytecode::MAGIC {
        return Err("bad magic: expected SMRQ header".to_string());
    }
    let version = read_u16le(blob, 4)?;
    let mut cursor = 6usize;

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

    let (tag_names, tag_end) = try_parse_tags(blob, cursor);
    cursor = tag_end;

    Ok((version, globals, tag_names, cursor))
}

fn try_parse_tags(blob: &[u8], start: usize) -> (Vec<String>, usize) {
    let parse = || -> Option<(Vec<String>, usize)> {
        if start + 2 > blob.len() { return None; }
        let n_tags = u16::from_le_bytes([blob[start], blob[start + 1]]) as usize;
        if n_tags > 256 { return None; }
        let mut cur = start + 2;
        let mut names = Vec::with_capacity(n_tags);
        for _ in 0..n_tags {
            if cur >= blob.len() { return None; }
            let len = blob[cur] as usize;
            cur += 1;
            if len == 0 || cur + len > blob.len() { return None; }
            let bytes = &blob[cur..cur + len];
            let name = std::str::from_utf8(bytes).ok()?;
            if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return None;
            }
            names.push(name.to_owned());
            cur += len;
        }
        Some((names, cur))
    };
    parse().unwrap_or_else(|| (Vec::new(), start))
}
