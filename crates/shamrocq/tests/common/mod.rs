#![allow(dead_code)]

use shamrocq::{tags, Value, Vm, BYTECODE};

pub fn setup() -> (Vec<u8>, &'static [u8]) {
    let buf = vec![0u8; 65536];
    (buf, BYTECODE)
}

#[cfg(feature = "stats")]
pub fn print_stats(name: &str, vm: &Vm) {
    eprintln!("[{name}] {}", vm.mem_snapshot());
    eprintln!("{}", vm.stats);
}

#[cfg(not(feature = "stats"))]
pub fn print_stats(_name: &str, _vm: &Vm) {}

pub fn peano(vm: &mut Vm, n: u32) -> Value {
    let mut v = Value::immediate(tags::O);
    for _ in 0..n {
        v = vm.alloc_tuple(tags::S, &[v]).unwrap();
    }
    v
}

pub fn unpeano(vm: &Vm, mut v: Value) -> u32 {
    let mut n = 0;
    while v.tag() == tags::S {
        v = vm.tuple_field(v, 0);
        n += 1;
    }
    n
}

pub fn list_to_vec(vm: &Vm, mut v: Value) -> Vec<Value> {
    let mut out = Vec::new();
    while v.tag() == tags::CONS {
        out.push(vm.tuple_field(v, 0));
        v = vm.tuple_field(v, 1);
    }
    out
}

pub fn make_list(vm: &mut Vm, items: &[Value]) -> Value {
    let mut list = Value::immediate(tags::NIL);
    for &item in items.iter().rev() {
        list = vm.alloc_tuple(tags::CONS, &[item, list]).unwrap();
    }
    list
}
