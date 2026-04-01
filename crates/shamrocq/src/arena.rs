use crate::value::Value;

#[derive(Debug, PartialEq, Eq)]
pub enum ArenaError {
    OutOfMemory,
}

// GC header layout: [0:1 | mark:1 | opaque:1 | fwd:16 | size:13]
const GC_MARK_BIT: u32 = 1 << 30;
const GC_OPAQUE_BIT: u32 = 1 << 29;
const GC_FWD_SHIFT: u32 = 13;
const GC_FWD_MASK: u32 = 0xFFFF;
const GC_SIZE_MASK: u32 = 0x1FFF;

pub struct Arena<'a> {
    buf: &'a mut [u8],
    heap_top: usize,
    stack_bot: usize,
}

impl<'a> Arena<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        let len = buf.len();
        Arena {
            buf,
            heap_top: 0,
            stack_bot: len,
        }
    }

    pub fn reset(&mut self) {
        self.heap_top = 0;
        self.stack_bot = self.buf.len();
    }

    fn align4(offset: usize) -> usize {
        (offset + 3) & !3
    }

    pub fn alloc(&mut self, words: usize) -> Result<usize, ArenaError> {
        let base = Self::align4(self.heap_top);
        let end = base + words * 4;
        if end > self.stack_bot {
            return Err(ArenaError::OutOfMemory);
        }
        self.heap_top = end;
        Ok(base)
    }

    // -- GC header --

    fn write_gc_header(&mut self, offset: usize, opaque: bool, size: usize) {
        let w = if opaque { GC_OPAQUE_BIT } else { 0 } | (size as u32 & GC_SIZE_MASK);
        self.write_word(offset, w);
    }

    pub fn gc_object_size(&self, offset: usize) -> usize {
        (self.read_word(offset) & GC_SIZE_MASK) as usize
    }

    pub fn gc_is_opaque(&self, offset: usize) -> bool {
        (self.read_word(offset) & GC_OPAQUE_BIT) != 0
    }

    pub fn gc_set_mark(&mut self, offset: usize) {
        let w = self.read_word(offset);
        self.write_word(offset, w | GC_MARK_BIT);
    }

    pub fn gc_is_marked(&self, offset: usize) -> bool {
        (self.read_word(offset) & GC_MARK_BIT) != 0
    }

    pub fn gc_clear_mark(&mut self, offset: usize) {
        let w = self.read_word(offset);
        self.write_word(offset, w & !GC_MARK_BIT);
    }

    pub fn gc_set_forwarding(&mut self, offset: usize, dest_word: usize) {
        let w = self.read_word(offset);
        let cleared = w & !(GC_FWD_MASK << GC_FWD_SHIFT);
        self.write_word(offset, cleared | (((dest_word as u32) & GC_FWD_MASK) << GC_FWD_SHIFT));
    }

    pub fn gc_read_forwarding(&self, offset: usize) -> usize {
        ((self.read_word(offset) >> GC_FWD_SHIFT) & GC_FWD_MASK) as usize
    }

    pub fn gc_clear_forwarding(&mut self, offset: usize) {
        let w = self.read_word(offset);
        self.write_word(offset, w & !(GC_FWD_MASK << GC_FWD_SHIFT));
    }

    // -- Ctor --
    // Heap layout: [gc_header] [field_0] [field_1] ... [field_{arity-1}]
    // gc_header: opaque=0, size = 1 + arity

    pub fn alloc_ctor(&mut self, tag: u8, fields: &[Value]) -> Result<Value, ArenaError> {
        let n = fields.len();
        let offset = self.alloc(1 + n)?;
        self.write_gc_header(offset, false, 1 + n);
        for (i, &f) in fields.iter().enumerate() {
            self.write_word(offset + (1 + i) * 4, f.raw());
        }
        Ok(Value::ctor(tag, offset))
    }

    pub fn alloc_ctor_from_stack(&mut self, tag: u8, arity: usize) -> Result<Value, ArenaError> {
        let offset = self.alloc(1 + arity)?;
        self.write_gc_header(offset, false, 1 + arity);
        for i in (0..arity).rev() {
            let field = self.stack_pop();
            self.write_word(offset + (1 + i) * 4, field.raw());
        }
        Ok(Value::ctor(tag, offset))
    }

    pub fn ctor_field(&self, val: Value, idx: usize) -> Value {
        let base = val.offset();
        Value::from_raw(self.read_word(base + (1 + idx) * 4))
    }

    pub fn ctor_arity(&self, val: Value) -> usize {
        self.gc_object_size(val.offset()) - 1
    }

    // -- Closure --
    // Heap layout: [gc_header] [closure_header] [bound_0] ... [bound_{n-1}]
    // gc_header: opaque=0, size = 2 + n_bound
    // closure_header: [code_addr:16 | arity:8 | n_bound:8]

    fn closure_header_offset(val: Value) -> usize {
        val.closure_offset() + 4
    }

    pub fn alloc_closure(
        &mut self,
        code_addr: u16,
        arity: u8,
        bound: &[Value],
    ) -> Result<Value, ArenaError> {
        let n = bound.len();
        let offset = self.alloc(2 + n)?;
        self.write_gc_header(offset, false, 2 + n);
        let header = ((code_addr as u32) << 16) | ((arity as u32) << 8) | (n as u32);
        self.write_word(offset + 4, header);
        for (i, &v) in bound.iter().enumerate() {
            self.write_word(offset + (2 + i) * 4, v.raw());
        }
        Ok(Value::closure(offset))
    }

    pub fn alloc_closure_from_stack(
        &mut self,
        code_addr: u16,
        arity: u8,
        n_bound: usize,
    ) -> Result<Value, ArenaError> {
        let offset = self.alloc(2 + n_bound)?;
        self.write_gc_header(offset, false, 2 + n_bound);
        let header = ((code_addr as u32) << 16) | ((arity as u32) << 8) | (n_bound as u32);
        self.write_word(offset + 4, header);
        for i in (0..n_bound).rev() {
            let val = self.stack_pop();
            self.write_word(offset + (2 + i) * 4, val.raw());
        }
        Ok(Value::closure(offset))
    }

    pub fn closure_code(&self, val: Value) -> u16 {
        let header = self.read_word(Self::closure_header_offset(val));
        (header >> 16) as u16
    }

    pub fn closure_arity(&self, val: Value) -> u8 {
        let header = self.read_word(Self::closure_header_offset(val));
        ((header >> 8) & 0xFF) as u8
    }

    pub fn closure_bound_count(&self, val: Value) -> usize {
        let header = self.read_word(Self::closure_header_offset(val));
        (header & 0xFF) as usize
    }

    pub fn closure_bound(&self, val: Value, idx: usize) -> Value {
        let base = val.closure_offset();
        Value::from_raw(self.read_word(base + (2 + idx) * 4))
    }

    pub fn closure_set_bound(&mut self, closure: Value, idx: usize, val: Value) {
        let base = closure.closure_offset();
        self.write_word(base + (2 + idx) * 4, val.raw());
    }

    pub fn extend_closure(
        &mut self,
        closure: Value,
        extra_arg: Value,
    ) -> Result<Value, ArenaError> {
        let old_bound = self.closure_bound_count(closure);
        let new_bound = old_bound + 1;
        let old_clo_header = self.read_word(Self::closure_header_offset(closure));
        let code_addr = (old_clo_header >> 16) as u16;
        let arity = ((old_clo_header >> 8) & 0xFF) as u8;
        let offset = self.alloc(2 + new_bound)?;
        self.write_gc_header(offset, false, 2 + new_bound);
        let header = ((code_addr as u32) << 16) | ((arity as u32) << 8) | (new_bound as u32);
        self.write_word(offset + 4, header);
        for i in 0..old_bound {
            let v = self.closure_bound(closure, i);
            self.write_word(offset + (2 + i) * 4, v.raw());
        }
        self.write_word(offset + (2 + old_bound) * 4, extra_arg.raw());
        Ok(Value::closure(offset))
    }

    // -- Bytes --
    // Heap layout: [gc_header] [len:32] [raw data...]
    // gc_header: opaque=1, size = 2 + ceil(len/4)

    pub fn alloc_bytes(&mut self, data: &[u8]) -> Result<Value, ArenaError> {
        let len = data.len();
        let data_words = (len + 3) / 4;
        let offset = self.alloc(2 + data_words)?;
        self.write_gc_header(offset, true, 2 + data_words);
        self.write_word(offset + 4, len as u32);
        self.buf[offset + 8..offset + 8 + len].copy_from_slice(data);
        Ok(Value::bytes(offset))
    }

    pub fn bytes_len(&self, val: Value) -> usize {
        self.read_word(val.bytes_offset() + 4) as usize
    }

    pub fn bytes_data(&self, val: Value) -> &[u8] {
        let offset = val.bytes_offset();
        let len = self.read_word(offset + 4) as usize;
        &self.buf[offset + 8..offset + 8 + len]
    }

    pub fn bytes_concat(&mut self, a: Value, b: Value) -> Result<Value, ArenaError> {
        let a_off = a.bytes_offset() + 8;
        let a_len = self.bytes_len(a);
        let b_off = b.bytes_offset() + 8;
        let b_len = self.bytes_len(b);
        let total = a_len + b_len;
        let data_words = (total + 3) / 4;
        let offset = self.alloc(2 + data_words)?;
        self.write_gc_header(offset, true, 2 + data_words);
        self.write_word(offset + 4, total as u32);
        self.buf.copy_within(a_off..a_off + a_len, offset + 8);
        self.buf.copy_within(b_off..b_off + b_len, offset + 8 + a_len);
        Ok(Value::bytes(offset))
    }

    // -- stack (grows downward) --

    pub fn stack_push(&mut self, val: Value) -> Result<(), ArenaError> {
        if self.stack_bot < self.heap_top + 4 {
            return Err(ArenaError::OutOfMemory);
        }
        self.stack_bot -= 4;
        self.write_word(self.stack_bot, val.raw());
        Ok(())
    }

    pub fn stack_pop(&mut self) -> Value {
        let val = Value::from_raw(self.read_word(self.stack_bot));
        self.stack_bot += 4;
        val
    }

    pub fn stack_peek(&self, depth: usize) -> Value {
        Value::from_raw(self.read_word(self.stack_bot + depth * 4))
    }

    pub fn stack_set(&mut self, depth: usize, val: Value) {
        self.write_word(self.stack_bot + depth * 4, val.raw());
    }

    pub fn stack_depth(&self) -> usize {
        (self.buf.len() - self.stack_bot) / 4
    }

    pub fn stack_truncate(&mut self, depth: usize) {
        self.stack_bot = self.buf.len() - depth * 4;
    }

    pub fn stack_bot_pos(&self) -> usize {
        self.stack_bot
    }

    pub fn set_stack_bot_pos(&mut self, pos: usize) {
        self.stack_bot = pos;
    }

    pub fn stack_read_at(&self, byte_pos: usize) -> Value {
        Value::from_raw(self.read_word(byte_pos))
    }

    pub fn stack_write_at(&mut self, byte_pos: usize, val: Value) {
        self.write_word(byte_pos, val.raw());
    }

    // -- raw word access (little-endian) --

    pub(crate) fn write_word(&mut self, offset: usize, val: u32) {
        let bytes = val.to_le_bytes();
        self.buf[offset..offset + 4].copy_from_slice(&bytes);
    }

    pub(crate) fn read_word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.buf[offset..offset + 4].try_into().unwrap())
    }

    pub fn heap_used(&self) -> usize {
        self.heap_top
    }

    pub fn set_heap_top(&mut self, pos: usize) {
        self.heap_top = pos;
    }

    pub fn stack_used(&self) -> usize {
        self.buf.len() - self.stack_bot
    }

    pub fn free(&self) -> usize {
        self.stack_bot - self.heap_top
    }

    pub fn buf_len(&self) -> usize {
        self.buf.len()
    }

    pub fn heap_data(&self) -> &[u8] {
        &self.buf[..self.heap_top]
    }

    pub fn stack_data(&self) -> &[u8] {
        &self.buf[self.stack_bot..]
    }
}
