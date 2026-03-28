#![cfg(feature = "integration")]

mod common;

use common::{setup, peano, unpeano, print_stats};
use shamrocq::{tags, funcs, Program, Value, Vm};

#[test]
fn compose_negb_negb() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let negb = vm.global_value(funcs::NEGB);
    let composed = vm.call(funcs::COMPOSE, &[negb, negb]).unwrap();
    let result = vm.apply(composed, &[Value::ctor(tags::TRUE, 0)]).unwrap();
    assert_eq!(result.tag(), tags::TRUE, "compose(negb, negb)(True) = True");
    print_stats("compose(negb,negb)", &vm);
}

#[test]
fn flip_sub() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    // flip(sub)(2, 5) = sub(5, 2) = 3
    let sub_fn = vm.global_value(funcs::SUB);
    let flipped = vm.call(funcs::FLIP, &[sub_fn]).unwrap();
    let n2 = peano(&mut vm, 2);
    let n5 = peano(&mut vm, 5);
    let result = vm.apply(flipped, &[n2, n5]).unwrap();
    assert_eq!(unpeano(&vm, result), 3);
    print_stats("flip(sub)(2,5)", &vm);
}

#[test]
fn const_fn_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let t = Value::ctor(tags::TRUE, 0);
    let f = Value::ctor(tags::FALSE, 0);
    let always_true = vm.call(funcs::CONST_FN, &[t]).unwrap();
    let result = vm.apply(always_true, &[f]).unwrap();
    assert_eq!(result.tag(), tags::TRUE);
    print_stats("const_fn(True)(False)", &vm);
}

#[test]
fn twice_negb() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let negb = vm.global_value(funcs::NEGB);
    let double_neg = vm.call(funcs::TWICE, &[negb]).unwrap();
    let result = vm.apply(double_neg, &[Value::ctor(tags::FALSE, 0)]).unwrap();
    assert_eq!(result.tag(), tags::FALSE, "twice(negb)(False) = False");
    print_stats("twice(negb)", &vm);
}

#[test]
fn apply_n_successor() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    // negb applied 4 times to True: negb^4(True) = True
    let negb = vm.global_value(funcs::NEGB);
    let n4 = peano(&mut vm, 4);
    let t = Value::ctor(tags::TRUE, 0);
    let result = vm.call(funcs::APPLY_N, &[negb, n4, t]).unwrap();
    assert_eq!(result.tag(), tags::TRUE, "apply_n(negb, 4, True) = True");

    let n3 = peano(&mut vm, 3);
    let result = vm.call(funcs::APPLY_N, &[negb, n3, t]).unwrap();
    assert_eq!(result.tag(), tags::FALSE, "apply_n(negb, 3, True) = False");
    print_stats("apply_n(negb,n,True)", &vm);
}
