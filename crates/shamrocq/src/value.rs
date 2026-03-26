const KIND_IMM: u32 = 0b00 << 30;
const KIND_TUPLE: u32 = 0b01 << 30;
const KIND_CLOSURE: u32 = 0b10 << 30;

const TAG_SHIFT: u32 = 24;
const TAG_MASK: u32 = 0x3F;
const PAYLOAD_MASK: u32 = 0x00FF_FFFF;
const KIND_MASK: u32 = 0xC000_0000;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Value(u32);

impl Value {
    pub const fn immediate(tag: u8) -> Self {
        Value(KIND_IMM | ((tag as u32) << TAG_SHIFT))
    }

    pub const fn tuple(tag: u8, offset: usize) -> Self {
        Value(KIND_TUPLE | ((tag as u32) << TAG_SHIFT) | (offset as u32))
    }

    pub const fn closure(offset: usize) -> Self {
        Value(KIND_CLOSURE | (offset as u32))
    }

    pub const fn tag(self) -> u8 {
        ((self.0 >> TAG_SHIFT) & TAG_MASK) as u8
    }

    pub const fn offset(self) -> usize {
        (self.0 & PAYLOAD_MASK) as usize
    }

    pub const fn is_immediate(self) -> bool {
        self.0 & KIND_MASK == KIND_IMM
    }

    pub const fn is_tuple(self) -> bool {
        self.0 & KIND_MASK == KIND_TUPLE
    }

    pub const fn is_closure(self) -> bool {
        self.0 & KIND_MASK == KIND_CLOSURE
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn from_raw(raw: u32) -> Self {
        Value(raw)
    }
}

impl core::fmt::Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_immediate() {
            write!(f, "Imm(tag={})", self.tag())
        } else if self.is_tuple() {
            write!(f, "Tuple(tag={}, @{})", self.tag(), self.offset())
        } else {
            write!(f, "Closure(@{})", self.offset())
        }
    }
}

pub mod tags {
    pub const TRUE: u8 = 0;
    pub const FALSE: u8 = 1;
    pub const NIL: u8 = 2;
    pub const CONS: u8 = 3;
    pub const O: u8 = 4;
    pub const S: u8 = 5;
    pub const LEFT: u8 = 6;
    pub const RIGHT: u8 = 7;
    pub const PAIR: u8 = 8;
    pub const BUILD_ROOT: u8 = 9;
    pub const BUILD_EDGE: u8 = 10;
    pub const BUILD_HFOREST: u8 = 11;

    pub const NUM_BUILTIN: u8 = 12;

    pub const fn arity(tag: u8) -> u8 {
        match tag {
            TRUE | FALSE | NIL | O | LEFT | RIGHT => 0,
            S => 1,
            CONS | PAIR | BUILD_ROOT | BUILD_HFOREST => 2,
            BUILD_EDGE => 3,
            _ => 0,
        }
    }

    pub const fn name(tag: u8) -> &'static str {
        match tag {
            TRUE => "True",
            FALSE => "False",
            NIL => "Nil",
            CONS => "Cons",
            O => "O",
            S => "S",
            LEFT => "Left",
            RIGHT => "Right",
            PAIR => "Pair",
            BUILD_ROOT => "Build_root",
            BUILD_EDGE => "Build_edge",
            BUILD_HFOREST => "Build_hforest",
            _ => "?",
        }
    }
}
