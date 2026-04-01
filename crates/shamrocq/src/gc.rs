use crate::arena::Arena;
use crate::value::Value;

const REF_BIT: u32 = 1 << 31;

pub struct GcStats {
    pub bytes_reclaimed: usize,
    pub live_bytes: usize,
}

impl<'a> Arena<'a> {
    /// Run a mark-compact GC cycle. `extra_roots` are additional Value slots
    /// (e.g. the globals array) that must be treated as GC roots and have their
    /// references updated after compaction.
    ///
    /// Returns stats about what was reclaimed.
    pub fn collect_garbage(&mut self, extra_roots: &mut [Value]) -> GcStats {
        let old_heap = self.heap_used();
        self.mark_phase(extra_roots);
        self.compact_phase(extra_roots);
        let new_heap = self.heap_used();
        GcStats {
            bytes_reclaimed: old_heap.saturating_sub(new_heap),
            live_bytes: new_heap,
        }
    }

    // -- Mark phase --

    fn mark_phase(&mut self, extra_roots: &[Value]) {
        // Mark from the operand stack
        let stack_bot = self.stack_bot_pos();
        let buf_len = self.buf_len();
        let mut pos = stack_bot;
        while pos < buf_len {
            let word = self.read_word(pos);
            if word & REF_BIT != 0 {
                let offset = Value::from_raw(word).offset();
                self.mark_object(offset);
            }
            pos += 4;
        }

        // Mark from extra roots (globals, etc.)
        for &val in extra_roots.iter() {
            if val.is_reference() {
                self.mark_object(val.offset());
            }
        }
    }

    fn mark_object(&mut self, offset: usize) {
        if offset >= self.heap_used() || self.gc_is_marked(offset) {
            return;
        }
        self.gc_set_mark(offset);

        if self.gc_is_opaque(offset) {
            return;
        }

        // Iterative DFS using a small on-stack worklist to avoid deep recursion.
        // For objects with <= 2 pointer fields (the common case: cons cells),
        // this handles them without the worklist.
        let size = self.gc_object_size(offset);
        let mut i = 1;
        while i < size {
            let field_word = self.read_word(offset + i * 4);
            if field_word & REF_BIT != 0 {
                let child_offset = Value::from_raw(field_word).offset();
                if child_offset < self.heap_used() && !self.gc_is_marked(child_offset) {
                    self.mark_recursive(child_offset);
                }
            }
            i += 1;
        }
    }

    fn mark_recursive(&mut self, offset: usize) {
        // Explicit worklist to avoid stack overflow on deep structures (long lists).
        // We use a fixed-size buffer; if it overflows we fall back to recursive marking
        // on the remainder (bounded by the buffer refills).
        const WORKLIST_CAP: usize = 128;
        let mut worklist = [0usize; WORKLIST_CAP];
        worklist[0] = offset;
        let mut wl_len = 1;

        while wl_len > 0 {
            wl_len -= 1;
            let obj = worklist[wl_len];

            if obj >= self.heap_used() || self.gc_is_marked(obj) {
                continue;
            }
            self.gc_set_mark(obj);

            if self.gc_is_opaque(obj) {
                continue;
            }

            let size = self.gc_object_size(obj);
            let mut i = 1;
            while i < size {
                let field_word = self.read_word(obj + i * 4);
                if field_word & REF_BIT != 0 {
                    let child = Value::from_raw(field_word).offset();
                    if child < self.heap_used() && !self.gc_is_marked(child) {
                        if wl_len < WORKLIST_CAP {
                            worklist[wl_len] = child;
                            wl_len += 1;
                        } else {
                            self.mark_recursive(child);
                        }
                    }
                }
                i += 1;
            }
        }
    }

    // -- Compact phase (Lisp-2 style: compute fwd, update ptrs, slide) --

    fn compact_phase(&mut self, extra_roots: &mut [Value]) {
        let old_heap_top = self.heap_used();

        // Pass 1: compute forwarding addresses for all marked objects.
        let new_top = self.compute_forwarding(old_heap_top);

        // Pass 2: update all references on the stack, in extra roots, and in
        // live heap objects.
        self.update_stack_refs();
        self.update_extra_roots(extra_roots);
        self.update_heap_refs(old_heap_top);

        // Pass 3: slide objects to their forwarding destinations.
        self.slide_objects(old_heap_top);

        self.set_heap_top(new_top);
    }

    fn compute_forwarding(&mut self, heap_top: usize) -> usize {
        let mut scan = 0usize;
        let mut dest = 0usize;
        while scan < heap_top {
            let size = self.gc_object_size(scan);
            let size_bytes = size * 4;
            if self.gc_is_marked(scan) {
                self.gc_set_forwarding(scan, dest / 4);
                dest += size_bytes;
            }
            scan += size_bytes;
        }
        dest
    }

    fn update_ref_word(&mut self, pos: usize) {
        let word = self.read_word(pos);
        if word & REF_BIT != 0 {
            let val = Value::from_raw(word);
            let old_offset = val.offset();
            if old_offset < self.heap_used() {
                let new_word_offset = self.gc_read_forwarding(old_offset);
                let new_byte_offset = new_word_offset * 4;
                if new_byte_offset != old_offset {
                    let tag_bits = word & !0x007F_FFFF;
                    let new_word = tag_bits | ((new_byte_offset >> 2) as u32);
                    self.write_word(pos, new_word);
                }
            }
        }
    }

    fn update_extra_roots(&mut self, roots: &mut [Value]) {
        for val in roots.iter_mut() {
            if val.is_reference() {
                let old_offset = val.offset();
                if old_offset < self.heap_used() {
                    let new_word_offset = self.gc_read_forwarding(old_offset);
                    let new_byte_offset = new_word_offset * 4;
                    if new_byte_offset != old_offset {
                        let tag_bits = val.raw() & !0x007F_FFFF;
                        *val = Value::from_raw(tag_bits | ((new_byte_offset >> 2) as u32));
                    }
                }
            }
        }
    }

    fn update_stack_refs(&mut self) {
        let stack_bot = self.stack_bot_pos();
        let buf_len = self.buf_len();
        let mut pos = stack_bot;
        while pos < buf_len {
            self.update_ref_word(pos);
            pos += 4;
        }
    }

    fn update_heap_refs(&mut self, heap_top: usize) {
        let mut scan = 0usize;
        while scan < heap_top {
            let size = self.gc_object_size(scan);
            let size_bytes = size * 4;
            if self.gc_is_marked(scan) && !self.gc_is_opaque(scan) {
                let mut i = 1;
                while i < size {
                    self.update_ref_word(scan + i * 4);
                    i += 1;
                }
            }
            scan += size_bytes;
        }
    }

    fn slide_objects(&mut self, heap_top: usize) {
        let mut scan = 0usize;
        while scan < heap_top {
            let header = self.read_word(scan);
            let size = (header & 0x1FFF) as usize;
            let size_bytes = size * 4;
            if header & (1 << 30) != 0 {
                let dest_word = ((header >> 13) & 0xFFFF) as usize;
                let dest = dest_word * 4;
                // Clear mark and forwarding in the header
                let clean_header = header & 0x2000_1FFF;
                self.write_word(scan, clean_header);
                if dest != scan {
                    // memmove: copy word by word (safe for overlapping regions
                    // since dest < scan always in compaction)
                    let mut j = 0;
                    while j < size_bytes {
                        let w = self.read_word(scan + j);
                        self.write_word(dest + j, w);
                        j += 4;
                    }
                } else {
                    // Already in place, just clean the header
                }
            }
            scan += size_bytes;
        }
    }
}
