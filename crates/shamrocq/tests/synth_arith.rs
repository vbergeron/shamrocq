mod common;

use common::{setup, peano, unpeano, print_stats};
use shamrocq::{funcs, Program, Vm};

#[test]
fn add_zero_right() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n3 = peano(&mut vm, 3);
    let n0 = peano(&mut vm, 0);
    let result = vm.call(funcs::ADD, &[n3, n0]).unwrap();
    assert_eq!(unpeano(&vm, result), 3);
    print_stats("add(3,0)", &vm);
}

#[test]
fn add_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n2 = peano(&mut vm, 2);
    let n3 = peano(&mut vm, 3);
    let result = vm.call(funcs::ADD, &[n2, n3]).unwrap();
    assert_eq!(unpeano(&vm, result), 5);
    print_stats("add(2,3)", &vm);
}

#[test]
fn mul_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n3 = peano(&mut vm, 3);
    let n4 = peano(&mut vm, 4);
    let result = vm.call(funcs::MUL, &[n3, n4]).unwrap();
    assert_eq!(unpeano(&vm, result), 12);
    print_stats("mul(3,4)", &vm);
}

#[test]
fn mul_by_zero() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n5 = peano(&mut vm, 5);
    let n0 = peano(&mut vm, 0);
    let result = vm.call(funcs::MUL, &[n0, n5]).unwrap();
    assert_eq!(unpeano(&vm, result), 0);
    print_stats("mul(0,5)", &vm);
}

#[test]
fn sub_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n5 = peano(&mut vm, 5);
    let n2 = peano(&mut vm, 2);
    let result = vm.call(funcs::SUB, &[n5, n2]).unwrap();
    assert_eq!(unpeano(&vm, result), 3);
    print_stats("sub(5,2)", &vm);
}

#[test]
fn sub_truncated() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n2 = peano(&mut vm, 2);
    let n5 = peano(&mut vm, 5);
    let result = vm.call(funcs::SUB, &[n2, n5]).unwrap();
    assert_eq!(unpeano(&vm, result), 0);
    print_stats("sub(2,5)", &vm);
}

#[test]
fn min_nat_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n3 = peano(&mut vm, 3);
    let n7 = peano(&mut vm, 7);
    let result = vm.call(funcs::MIN_NAT, &[n3, n7]).unwrap();
    assert_eq!(unpeano(&vm, result), 3);
    print_stats("min_nat(3,7)", &vm);
}

#[test]
fn max_nat_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n3 = peano(&mut vm, 3);
    let n7 = peano(&mut vm, 7);
    let result = vm.call(funcs::MAX_NAT, &[n3, n7]).unwrap();
    assert_eq!(unpeano(&vm, result), 7);
    print_stats("max_nat(3,7)", &vm);
}
