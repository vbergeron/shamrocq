#![no_std]

pub mod arena;
pub mod bytes;
pub mod gc;
pub mod stats;
pub mod value;
pub mod vm;

pub use stats::MemSnapshot;
#[cfg(feature = "stats")]
pub use stats::Stats;
pub use value::{tags, Value};
pub use vm::{ForeignFn, Program, Vm, VmError};
