use crate::value::Value;

mod bytes {
    /// Cast `&mut [u8]` to `&mut [u32]`, truncating to the nearest whole word.
    ///
    /// # Safety
    /// `slice` must be 4-byte aligned.
    pub(super) unsafe fn as_words_mut(slice: &mut [u8]) -> &mut [u32] {
        core::slice::from_raw_parts_mut(slice.as_mut_ptr() as *mut u32, slice.len() / 4)
    }

    pub(super) fn as_bytes(words: &[u32]) -> &[u8] {
        // Safety: u8 has alignment 1; byte length is words.len() * 4.
        unsafe { core::slice::from_raw_parts(words.as_ptr() as *const u8, words.len() * 4) }
    }

    pub(super) fn as_bytes_mut(words: &mut [u32]) -> &mut [u8] {
        // Safety: u8 has alignment 1; byte length is words.len() * 4.
        unsafe { core::slice::from_raw_parts_mut(words.as_mut_ptr() as *mut u8, words.len() * 4) }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArenaError {
    OutOfMemory,
}

pub struct Arena<'a> {
    buf: &'a mut [u32],
    heap_top: usize,  // byte offset
    stack_bot: usize, // byte offset
}

impl<'a> Arena<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        // Safety: heap allocations and static buffers declared with
        // #[repr(align(4))] are 4-byte aligned.
        let buf32 = unsafe { bytes::as_words_mut(buf) };
        let words = buf32.len();
        Arena {
            buf: buf32,
            heap_top: 0,
            stack_bot: words * 4,
        }
    }

    pub fn reset(&mut self) {
        self.heap_top = 0;
        self.stack_bot = self.buf.len() * 4;
    }

    pub fn alloc(&mut self, words: usize) -> Result<usize, ArenaError> {
        let base = self.heap_top;
        let end = base + words * 4;
        if end > self.stack_bot {
            return Err(ArenaError::OutOfMemory);
        }
        self.heap_top = end;
        Ok(base)
    }

    // -- Ctor --

    pub fn alloc_ctor(&mut self, tag: u8, fields: &[Value]) -> Result<Value, ArenaError> {
        let offset = self.alloc(fields.len())?;
        for (i, &f) in fields.iter().enumerate() {
            self.write_word(offset + i * 4, f.raw());
        }
        Ok(Value::ctor(tag, offset))
    }

    pub fn alloc_ctor_from_stack(&mut self, tag: u8, arity: usize) -> Result<Value, ArenaError> {
        let offset = self.alloc(arity)?;
        for i in (0..arity).rev() {
            let field = self.stack_pop();
            self.write_word(offset + i * 4, field.raw());
        }
        Ok(Value::ctor(tag, offset))
    }

    pub fn ctor_field(&self, val: Value, idx: usize) -> Value {
        let base = val.offset();
        Value::from_raw(self.read_word(base + idx * 4))
    }

    // -- Closure: header = [code_addr:16 | arity:8 | n_cap:8] --

    pub fn alloc_closure(
        &mut self,
        code_addr: u16,
        arity: u8,
        captures: &[Value],
    ) -> Result<Value, ArenaError> {
        let n = captures.len();
        let offset = self.alloc(1 + n)?;
        let header = ((code_addr as u32) << 16) | ((arity as u32) << 8) | (n as u32);
        self.write_word(offset, header);
        for (i, &c) in captures.iter().enumerate() {
            self.write_word(offset + (1 + i) * 4, c.raw());
        }
        Ok(Value::closure(offset))
    }

    pub fn alloc_closure_from_stack(
        &mut self,
        code_addr: u16,
        arity: u8,
        n_cap: usize,
    ) -> Result<Value, ArenaError> {
        let offset = self.alloc(1 + n_cap)?;
        let header = ((code_addr as u32) << 16) | ((arity as u32) << 8) | (n_cap as u32);
        self.write_word(offset, header);
        for i in (0..n_cap).rev() {
            let cap = self.stack_pop();
            self.write_word(offset + (1 + i) * 4, cap.raw());
        }
        Ok(Value::closure(offset))
    }

    pub fn closure_code(&self, val: Value) -> u16 {
        let header = self.read_word(val.closure_offset());
        (header >> 16) as u16
    }

    pub fn closure_arity(&self, val: Value) -> u8 {
        let header = self.read_word(val.closure_offset());
        ((header >> 8) & 0xFF) as u8
    }

    pub fn closure_capture_count(&self, val: Value) -> usize {
        let header = self.read_word(val.closure_offset());
        (header & 0xFF) as usize
    }

    pub fn closure_capture(&self, val: Value, idx: usize) -> Value {
        let base = val.closure_offset();
        Value::from_raw(self.read_word(base + (1 + idx) * 4))
    }

    pub fn closure_set_capture(&mut self, closure: Value, idx: usize, val: Value) {
        let base = closure.closure_offset();
        self.write_word(base + (1 + idx) * 4, val.raw());
    }

    // -- Application: header = [arity:4 | applied:4 | callee_kind:3 | callee_payload:21] --

    pub fn alloc_application(
        &mut self,
        callee: Value,
        arity: u8,
        args: &[Value],
    ) -> Result<Value, ArenaError> {
        let applied = args.len();
        let offset = self.alloc(1 + applied)?;
        let kind_bits = (callee.raw() >> 29) & 0x7;
        let callee_bits = (kind_bits << 21) | (callee.raw() & PAYLOAD_21_RAW);
        let header = ((arity as u32 & 0xF) << 28)
            | ((applied as u32 & 0xF) << 24)
            | callee_bits;
        self.write_word(offset, header);
        for (i, &a) in args.iter().enumerate() {
            self.write_word(offset + (1 + i) * 4, a.raw());
        }
        Ok(Value::application(offset))
    }

    pub fn application_arity(&self, val: Value) -> u8 {
        let header = self.read_word(val.application_offset());
        ((header >> 28) & 0xF) as u8
    }

    pub fn application_applied(&self, val: Value) -> u8 {
        let header = self.read_word(val.application_offset());
        ((header >> 24) & 0xF) as u8
    }

    pub fn application_callee(&self, val: Value) -> Value {
        let header = self.read_word(val.application_offset());
        let callee_bits = header & 0x00FF_FFFF;
        // Reconstruct the callee Value: kind bits are in 23:21, shift to 31:29
        let kind = (callee_bits >> 21) & 0x7;
        let payload = callee_bits & PAYLOAD_21_RAW;
        Value::from_raw((kind << 29) | payload)
    }

    pub fn application_arg(&self, val: Value, idx: usize) -> Value {
        let base = val.application_offset();
        Value::from_raw(self.read_word(base + (1 + idx) * 4))
    }

    pub fn extend_application(
        &mut self,
        app: Value,
        extra_arg: Value,
    ) -> Result<Value, ArenaError> {
        let old_applied = self.application_applied(app) as usize;
        let arity = self.application_arity(app);
        let new_applied = old_applied + 1;
        let offset = self.alloc(1 + new_applied)?;
        let old_header = self.read_word(app.application_offset());
        let callee_bits = old_header & 0x00FF_FFFF;
        let header = ((arity as u32 & 0xF) << 28)
            | ((new_applied as u32 & 0xF) << 24)
            | callee_bits;
        self.write_word(offset, header);
        for i in 0..old_applied {
            let arg = self.application_arg(app, i);
            self.write_word(offset + (1 + i) * 4, arg.raw());
        }
        self.write_word(offset + (1 + old_applied) * 4, extra_arg.raw());
        Ok(Value::application(offset))
    }

    // -- Bytes --

    pub fn alloc_bytes(&mut self, data: &[u8]) -> Result<Value, ArenaError> {
        let len = data.len();
        let words = (len + 3) / 4;
        let byte_offset = self.alloc(words)?;
        let word_off = byte_offset >> 2;
        bytes::as_bytes_mut(&mut self.buf[word_off..word_off + words])[..len]
            .copy_from_slice(data);
        Ok(Value::bytes(len as u8, byte_offset))
    }

    pub fn bytes_data(&self, val: Value) -> &[u8] {
        let word_off = val.bytes_offset() >> 2;
        let len = val.bytes_len();
        &bytes::as_bytes(&self.buf[word_off..])[..len]
    }

    pub fn bytes_concat(&mut self, a: Value, b: Value) -> Result<Value, ArenaError> {
        let a_len = a.bytes_len();
        let b_len = b.bytes_len();
        let total = a_len + b_len;
        let words = (total + 3) / 4;
        let byte_offset = self.alloc(words)?;
        // The bump allocator guarantees the destination is above both sources,
        // so split_at_mut cleanly separates them.
        let dst_word_off = byte_offset >> 2;
        let (sources, dest_words) = self.buf.split_at_mut(dst_word_off);
        let src = bytes::as_bytes(sources);
        let dst = bytes::as_bytes_mut(dest_words);
        dst[..a_len].copy_from_slice(&src[a.bytes_offset()..][..a_len]);
        dst[a_len..total].copy_from_slice(&src[b.bytes_offset()..][..b_len]);
        Ok(Value::bytes(total as u8, byte_offset))
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
        (self.buf.len() * 4 - self.stack_bot) / 4
    }

    pub fn stack_truncate(&mut self, depth: usize) {
        self.stack_bot = self.buf.len() * 4 - depth * 4;
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

    // -- raw word access --

    #[inline]
    fn write_word(&mut self, byte_offset: usize, val: u32) {
        self.buf[byte_offset >> 2] = val;
    }

    #[inline]
    fn read_word(&self, byte_offset: usize) -> u32 {
        self.buf[byte_offset >> 2]
    }

    pub fn heap_used(&self) -> usize {
        self.heap_top
    }

    pub fn stack_used(&self) -> usize {
        self.buf.len() * 4 - self.stack_bot
    }

    pub fn free(&self) -> usize {
        self.stack_bot - self.heap_top
    }

    pub fn buf_len(&self) -> usize {
        self.buf.len() * 4
    }

    pub fn heap_data(&self) -> &[u8] {
        &bytes::as_bytes(&self.buf[..self.heap_top >> 2])
    }

    pub fn stack_data(&self) -> &[u8] {
        bytes::as_bytes(&self.buf[self.stack_bot >> 2..])
    }
}

const PAYLOAD_21_RAW: u32 = 0x001F_FFFF;
