//! Dead binding elimination pass (ResolvedPass).
//!
//! Removes `Let` bindings whose bound variable is never referenced:
//!
//!   let _ = val in body   =>   body   (when Local(0) unused in body)
//!
//! This is safe because the language has no side effects -- if the bound
//! value is never observed, computing it can be skipped entirely. Free
//! variables in `body` are shifted down to account for the removed binding.

use crate::ir::{RDefine, RExpr, RExprVisitor};
use super::ResolvedPass;
use super::p08_anf::{references_local, shift_down};

pub struct DeadBindingElim;

impl ResolvedPass for DeadBindingElim {
    fn name(&self) -> &'static str { "dead_binding_elim" }

    fn run(&self, defs: Vec<RDefine>) -> Vec<RDefine> {
        DeadBindingVisitor.visit_rprogram(defs)
    }
}

struct DeadBindingVisitor;

impl RExprVisitor for DeadBindingVisitor {
    fn visit_rexpr(&mut self, expr: RExpr) -> RExpr {
        match expr {
            RExpr::Let(val, body) => {
                let val = self.visit_rexpr(*val);
                let body = self.visit_rexpr(*body);
                if !references_local(&body, 0, 0) {
                    shift_down(&body, 0, 1)
                } else {
                    RExpr::Let(Box::new(val), Box::new(body))
                }
            }
            other => self.walk_rexpr(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RExpr;

    fn rdef(name: &str, body: RExpr) -> RDefine {
        RDefine { name: name.to_string(), global_idx: 0, body }
    }

    #[test]
    fn dead_let_removed() {
        // let _ = 42 in Global(0)  =>  Global(0)
        let input = rdef("f", RExpr::Let(
            Box::new(RExpr::Int(42)),
            Box::new(RExpr::Global(0)),
        ));
        let result = DeadBindingElim.run(vec![input]);
        assert_eq!(result[0].body, RExpr::Global(0));
    }

    #[test]
    fn live_let_kept() {
        // let x = 42 in x   (Local(0) is used)
        let input = rdef("f", RExpr::Let(
            Box::new(RExpr::Int(42)),
            Box::new(RExpr::Local(0)),
        ));
        let result = DeadBindingElim.run(vec![input]);
        assert_eq!(result[0].body, RExpr::Let(
            Box::new(RExpr::Int(42)),
            Box::new(RExpr::Local(0)),
        ));
    }

    #[test]
    fn dead_let_shifts_free_vars() {
        // let _ = 0 in Local(1)  =>  Local(0)
        // Local(1) in the body refers to a variable one level above the dead
        // binding; after removal it becomes Local(0).
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::Let(
                Box::new(RExpr::Int(0)),
                Box::new(RExpr::Local(1)),
            ),
        )));
        let result = DeadBindingElim.run(vec![input]);
        assert_eq!(result[0].body, RExpr::Lambda(Box::new(RExpr::Local(0))));
    }
}
