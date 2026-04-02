pub fn read_u8(buf: &[u8], pos: usize) -> Result<u8, String> {
    buf.get(pos)
        .copied()
        .ok_or_else(|| format!("unexpected end of blob at byte {}", pos))
}

pub fn read_u16le(buf: &[u8], pos: usize) -> Result<u16, String> {
    if pos + 2 > buf.len() {
        return Err(format!("unexpected end of blob reading u16 at byte {}", pos));
    }
    Ok(u16::from_le_bytes([buf[pos], buf[pos + 1]]))
}

pub fn read_i32le(buf: &[u8], pos: usize) -> Result<i32, String> {
    if pos + 4 > buf.len() {
        return Err(format!("unexpected end of blob reading i32 at byte {}", pos));
    }
    Ok(i32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]))
}

pub fn read_u32le(buf: &[u8], pos: usize) -> Result<u32, String> {
    if pos + 4 > buf.len() {
        return Err(format!("unexpected end of blob reading u32 at byte {}", pos));
    }
    Ok(u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]))
}

pub fn escape_bytes(data: &[u8]) -> String {
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
