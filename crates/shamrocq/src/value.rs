// Value encoding (32 bits)
//
//   bit 31 = 0  →  Immediate (no heap allocation)
//     0 | 00 | value:29                             Integer (sign-extended)
//     0 | 01 | tag:8 | 0:21                         Nullary ctor
//     0 | 10 | foreign:1 | arity:4 | addr:16 | 0:8  Function
//     0 | 11 | (reserved)
//
//   bit 31 = 1  →  Reference (heap pointer)
//     1 | tag:8 | offset:23     Ctor (tag 0..253)
//     1 | 0xFF  | offset:23     Closure (sentinel)
//     1 | 0xFE  | offset:23     Bytes (sentinel)
//
//   Offsets are word indices into the u32 arena buffer.

const REF_BIT: u32 = 1 << 31;

// Immediate sub-tags (bits 30:29)
const IMM_SUB_SHIFT: u32 = 29;
const IMM_SUB_MASK: u32 = 0b11 << IMM_SUB_SHIFT;
const IMM_INTEGER: u32 = 0b00 << IMM_SUB_SHIFT;
const IMM_NULLARY_CTOR: u32 = 0b01 << IMM_SUB_SHIFT;
const IMM_FUNCTION: u32 = 0b10 << IMM_SUB_SHIFT;

// Reference layout: 1 | tag:8 | offset:23
const REF_TAG_SHIFT: u32 = 23;
const REF_TAG_MASK: u32 = 0xFF;
const REF_OFFSET_MASK: u32 = 0x007F_FFFF;

// Sentinel tags for non-ctor references
const TAG_CLOSURE: u8 = 0xFF;
const TAG_BYTES: u8 = 0xFE;

// Shared tag accessor (works for both nullary and heap ctors)
const TAG_SHIFT: u32 = 21;
const TAG_MASK_8: u32 = 0xFF;

// Integer payload
const PAYLOAD_29: u32 = 0x1FFF_FFFF;

// Function layout (inside immediate): 0|10|foreign:1|arity:4|addr:16|0:8
const FN_FOREIGN_BIT: u32 = 1 << 28;
const FN_ARITY_SHIFT: u32 = 24;
const FN_ARITY_MASK: u32 = 0xF;
const FN_ADDR_SHIFT: u32 = 8;
const FN_ADDR_MASK: u32 = 0xFFFF;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Value(u32);

impl Value {
    // -- Immediate / Reference --

    pub const fn is_reference(self) -> bool {
        (self.0 & REF_BIT) != 0
    }

    pub const fn is_immediate(self) -> bool {
        !self.is_reference()
    }

    // -- Ctor (heap-allocated, arity >= 1): 1 | tag:8 | offset:23 --

    pub const fn ctor(tag: u8, word_offset: usize) -> Self {
        Value(REF_BIT | ((tag as u32) << REF_TAG_SHIFT) | (word_offset as u32))
    }

    pub const fn is_ctor(self) -> bool {
        self.is_reference() && self.ref_tag() < TAG_BYTES
    }

    /// Word offset into the arena buffer.
    pub const fn offset(self) -> usize {
        (self.0 & REF_OFFSET_MASK) as usize
    }

    const fn ref_tag(self) -> u8 {
        ((self.0 >> REF_TAG_SHIFT) & REF_TAG_MASK) as u8
    }

    // -- Nullary ctor (immediate): 0 | 01 | tag:8 | 0:21 --

    pub const fn nullary_ctor(tag: u8) -> Self {
        Value(IMM_NULLARY_CTOR | ((tag as u32) << TAG_SHIFT))
    }

    pub const fn is_nullary_ctor(self) -> bool {
        self.is_immediate() && (self.0 & IMM_SUB_MASK) == IMM_NULLARY_CTOR
    }

    /// Constructor tag — works for both nullary (immediate) and heap ctors.
    pub const fn tag(self) -> u8 {
        if self.is_reference() {
            self.ref_tag()
        } else {
            ((self.0 >> TAG_SHIFT) & TAG_MASK_8) as u8
        }
    }

    // -- Integer (immediate): 0 | 00 | value:29 --

    pub const fn integer(n: i32) -> Self {
        Value(IMM_INTEGER | ((n as u32) & PAYLOAD_29))
    }

    pub const fn is_integer(self) -> bool {
        self.is_immediate() && (self.0 & IMM_SUB_MASK) == IMM_INTEGER
    }

    pub const fn integer_value(self) -> i32 {
        ((self.0 << 3) as i32) >> 3
    }

    // -- Closure (reference): 1 | 0xFF | offset:23 --

    pub const fn closure(word_offset: usize) -> Self {
        Value(REF_BIT | ((TAG_CLOSURE as u32) << REF_TAG_SHIFT) | (word_offset as u32))
    }

    pub const fn is_closure(self) -> bool {
        self.is_reference() && self.ref_tag() == TAG_CLOSURE
    }

    pub const fn closure_offset(self) -> usize {
        self.offset()
    }

    // -- Bytes (reference): 1 | 0xFE | offset:23 --

    pub const fn bytes(word_offset: usize) -> Self {
        Value(REF_BIT | ((TAG_BYTES as u32) << REF_TAG_SHIFT) | (word_offset as u32))
    }

    pub const fn is_bytes(self) -> bool {
        self.is_reference() && self.ref_tag() == TAG_BYTES
    }

    pub const fn bytes_offset(self) -> usize {
        self.offset()
    }

    // -- Function (immediate): 0 | 10 | foreign:1 | arity:4 | addr:16 | 0:8 --

    pub const fn function(code_addr: u16, arity: u8) -> Self {
        Value(
            IMM_FUNCTION
                | ((arity as u32 & FN_ARITY_MASK) << FN_ARITY_SHIFT)
                | ((code_addr as u32) << FN_ADDR_SHIFT),
        )
    }

    pub const fn foreign_fn(idx: u16, arity: u8) -> Self {
        Value(
            IMM_FUNCTION
                | FN_FOREIGN_BIT
                | ((arity as u32 & FN_ARITY_MASK) << FN_ARITY_SHIFT)
                | ((idx as u32) << FN_ADDR_SHIFT),
        )
    }

    pub const fn is_function(self) -> bool {
        self.is_immediate() && (self.0 & IMM_SUB_MASK) == IMM_FUNCTION
    }

    pub const fn is_foreign_fn(self) -> bool {
        self.is_function() && (self.0 & FN_FOREIGN_BIT) != 0
    }

    pub const fn is_callable(self) -> bool {
        self.is_function() || self.is_closure()
    }

    pub const fn fn_arity(self) -> u8 {
        ((self.0 >> FN_ARITY_SHIFT) & FN_ARITY_MASK) as u8
    }

    pub const fn fn_addr(self) -> u16 {
        ((self.0 >> FN_ADDR_SHIFT) & FN_ADDR_MASK) as u16
    }

    // -- Raw access --

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn from_raw(raw: u32) -> Self {
        Value(raw)
    }
}

impl core::fmt::Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_nullary_ctor() {
            write!(f, "Ctor(tag={})", self.tag())
        } else if self.is_ctor() {
            write!(f, "Ctor(tag={}, @{})", self.tag(), self.offset())
        } else if self.is_integer() {
            write!(f, "Int({})", self.integer_value())
        } else if self.is_bytes() {
            write!(f, "Bytes(@{})", self.bytes_offset())
        } else if self.is_closure() {
            write!(f, "Closure(@{})", self.closure_offset())
        } else if self.is_foreign_fn() {
            write!(f, "ForeignFn(idx={}, arity={})", self.fn_addr(), self.fn_arity())
        } else if self.is_function() {
            write!(f, "Fn(pc={}, arity={})", self.fn_addr(), self.fn_arity())
        } else {
            write!(f, "Unknown(0x{:08X})", self.0)
        }
    }
}

pub use shamrocq_bytecode::tags;
