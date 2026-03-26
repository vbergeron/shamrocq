#![no_std]

pub mod arena;
pub mod value;
pub mod vm;

pub static BYTECODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bytecode.bin"));

pub mod funcs {
    include!(concat!(env!("OUT_DIR"), "/funcs.rs"));
}

pub use value::{tags, Value};
pub use vm::{Program, Vm, VmError};
