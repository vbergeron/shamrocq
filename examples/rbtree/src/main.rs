#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use panic_halt as _;

use shamrocq::{Program, Value, Vm, VmError};

shamrocq::include_program!(env!("OUT_DIR"));
shamrocq::static_heap!(HEAP, 40_000);

fn vm_exit_err(e: VmError) -> ! {
    let _ = hprintln!("VM error: {:?}", e);
    debug::exit(debug::EXIT_FAILURE);
    loop {}
}

#[entry]
fn main() -> ! {
    let buf = HEAP();
    let prog = Program::from_blob_or_exit(BYTECODE, vm_exit_err);
    let mut vm = Vm::new(buf);
    unsafe { enable_dwt_cyccnt(); }
    vm.set_cycle_reader(read_dwt_cyccnt);
    vm.load(&prog).unwrap_or_else(|e| vm_exit_err(e));

    let n = 50;

    let tree = vm.call_or_exit(funcs::BUILD_TREE, &[Value::integer(n)], vm_exit_err);

    let d = vm.call_or_exit(funcs::DEPTH, &[tree], vm_exit_err);
    let _ = hprintln!("build_tree({}) depth = {}", n, d.integer_value());

    let s = vm.call_or_exit(funcs::SIZE, &[tree], vm_exit_err);
    let _ = hprintln!("build_tree({}) size  = {}", n, s.integer_value());

    let ok = vm.call_or_exit(funcs::BUILD_AND_CHECK, &[Value::integer(n)], vm_exit_err);
    let _ = hprintln!(
        "build_and_check({}) = {}",
        n,
        if ok.tag() == ctors::TRUE { "true" } else { "false" }
    );
    let _ = hprintln!("{}", vm.combined_stats());

    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}

unsafe fn enable_dwt_cyccnt() {
    const DEMCR: *mut u32 = 0xE000_EDFC as *mut u32;
    const DWT_CTRL: *mut u32 = 0xE000_1000 as *mut u32;
    core::ptr::write_volatile(DEMCR, core::ptr::read_volatile(DEMCR) | (1 << 24));
    core::ptr::write_volatile(DWT_CTRL, core::ptr::read_volatile(DWT_CTRL) | 1);
}

fn read_dwt_cyccnt() -> u32 {
    const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;
    unsafe { core::ptr::read_volatile(DWT_CYCCNT) }
}
