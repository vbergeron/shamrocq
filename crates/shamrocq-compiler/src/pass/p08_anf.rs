use crate::ir::{PrimOp, RDefines, RExpr};
use super::ResolvedPass;

pub struct AnfNormalize;

impl ResolvedPass for AnfNormalize {
    fn name(&self) -> &'static str { "anf_normalize" }

    fn run(&self, defs: RDefines) -> RDefines {
        defs.map_bodies(|b| b.bottom_up(&anf_lift))
    }
}

fn anf_lift(e: RExpr) -> RExpr {
    match e {
        RExpr::App(func, arg) => lift_app(*func, *arg),
        RExpr::AppN(func, args) => lift_appn(*func, args),
        RExpr::Ctor(tag, fields) => lift_ctor_fields(tag, fields),
        RExpr::PrimOp(op, args) => lift_primop_args(op, args),
        other => other,
    }
}

fn lift_app(func: RExpr, arg: RExpr) -> RExpr {
    if arg.is_atomic() {
        RExpr::App(Box::new(func), Box::new(arg))
    } else {
        RExpr::Let(
            Box::new(arg),
            Box::new(RExpr::App(
                Box::new(func.shift(0, 1)),
                Box::new(RExpr::Local(0)),
            )),
        )
    }
}

fn lift_appn(func: RExpr, args: Vec<RExpr>) -> RExpr {
    let non_atomic: Vec<usize> = (0..args.len())
        .filter(|i| !args[*i].is_atomic())
        .collect();
    if non_atomic.is_empty() {
        return RExpr::AppN(Box::new(func), args);
    }
    let k = non_atomic.len();
    let mut new_args = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        if let Some(j) = non_atomic.iter().position(|&idx| idx == i) {
            new_args.push(RExpr::Local((k - 1 - j) as u8));
        } else {
            new_args.push(arg.shift(0, k));
        }
    }
    let mut result = RExpr::AppN(Box::new(func.shift(0, k)), new_args);
    for j in (0..k).rev() {
        let val = args[non_atomic[j]].shift(0, j);
        result = RExpr::Let(Box::new(val), Box::new(result));
    }
    result
}

fn lift_ctor_fields(tag: u8, fields: Vec<RExpr>) -> RExpr {
    let non_atomic: Vec<usize> = (0..fields.len())
        .filter(|i| !fields[*i].is_atomic())
        .collect();

    if non_atomic.is_empty() {
        return RExpr::Ctor(tag, fields);
    }

    let k = non_atomic.len();

    let mut ctor_fields = Vec::with_capacity(fields.len());
    for (i, field) in fields.iter().enumerate() {
        if let Some(j) = non_atomic.iter().position(|&idx| idx == i) {
            ctor_fields.push(RExpr::Local((k - 1 - j) as u8));
        } else {
            ctor_fields.push(field.shift(0, k));
        }
    }

    let mut result = RExpr::Ctor(tag, ctor_fields);

    for j in (0..k).rev() {
        let val = fields[non_atomic[j]].shift(0, j);
        result = RExpr::Let(Box::new(val), Box::new(result));
    }

    result
}

fn lift_primop_args(op: PrimOp, args: Vec<RExpr>) -> RExpr {
    let non_atomic: Vec<usize> = (0..args.len())
        .filter(|i| !args[*i].is_atomic())
        .collect();

    if non_atomic.is_empty() {
        return RExpr::PrimOp(op, args);
    }

    let k = non_atomic.len();

    let mut primop_args = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        if let Some(j) = non_atomic.iter().position(|&idx| idx == i) {
            primop_args.push(RExpr::Local((k - 1 - j) as u8));
        } else {
            primop_args.push(arg.shift(0, k));
        }
    }

    let mut result = RExpr::PrimOp(op, primop_args);

    for j in (0..k).rev() {
        let val = args[non_atomic[j]].shift(0, j);
        result = RExpr::Let(Box::new(val), Box::new(result));
    }

    result
}
