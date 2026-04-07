//! Small-global inlining pass (ExprPass).
//!
//! Replaces references to small, non-recursive global definitions with their
//! body. "Small" means the body's AST has at most 5 nodes.
//!
//!   (define add (lambda (n m) (+ n m)))
//!   (define f (lambda (x) (add x 1)))
//!     =>
//!   (define f (lambda (x) ((lambda (n m) (+ n m)) x 1)))
//!
//! This trades code size for reduced call overhead. On memory-constrained
//! targets the code growth may outweigh the benefit when a small function
//! is used at many call sites.

use std::collections::HashMap;
use crate::ir::{Define, Expr, ExprRefVisitor, ExprVisitor};
use super::ExprPass;

pub struct InlineSmallGlobals;

impl ExprPass for InlineSmallGlobals {
    fn name(&self) -> &'static str { "inline_small_globals" }

    fn run(&self, defs: Vec<Define>) -> Vec<Define> {
        let candidates = find_candidates(&defs);
        if candidates.is_empty() {
            return defs;
        }
        InlineVisitor { candidates }.visit_program(defs)
    }
}

fn find_candidates(defs: &[Define]) -> HashMap<String, Expr> {
    let mut candidates = HashMap::new();
    for d in defs {
        if is_small(&d.body) && !references_self(&d.body, &d.name) {
            candidates.insert(d.name.clone(), d.body.clone());
        }
    }
    candidates
}

fn is_small(expr: &Expr) -> bool {
    let mut v = ExprSizeVisitor { size: 0 };
    v.visit_expr_ref(expr);
    v.size <= 5
}

struct ExprSizeVisitor {
    size: usize,
}

impl ExprRefVisitor for ExprSizeVisitor {
    fn visit_expr_ref(&mut self, expr: &Expr) {
        match expr {
            Expr::Lambdas(params, body) => {
                self.size += params.len();
                self.visit_expr_ref(body);
            }
            _ => {
                self.size += 1;
                self.walk_expr_ref(expr);
            }
        }
    }
}

fn references_self(expr: &Expr, name: &str) -> bool {
    let mut v = ReferencesVarVisitor { name, found: false };
    v.visit_expr_ref(expr);
    v.found
}

struct ReferencesVarVisitor<'a> {
    name: &'a str,
    found: bool,
}

impl ExprRefVisitor for ReferencesVarVisitor<'_> {
    fn visit_expr_ref(&mut self, expr: &Expr) {
        if self.found { return; }
        if let Expr::Var(v) = expr {
            if v == self.name { self.found = true; return; }
        }
        self.walk_expr_ref(expr);
    }
}

struct InlineVisitor {
    candidates: HashMap<String, Expr>,
}

impl ExprVisitor for InlineVisitor {
    fn visit_expr(&mut self, expr: Expr) -> Expr {
        match expr {
            Expr::Var(ref name) => {
                if let Some(body) = self.candidates.get(name) {
                    body.clone()
                } else {
                    expr
                }
            }
            other => self.walk_expr(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Expr;

    fn def(name: &str, body: Expr) -> Define {
        Define { name: name.to_string(), body }
    }

    #[test]
    fn inline_small_global() {
        // (define id (lambda (x) x))
        // (define f (lambda (y) (id y)))
        //   => f's body becomes (lambda (y) ((lambda (x) x) y))
        let defs = vec![
            def("id", Expr::Lambda("x".into(), Box::new(Expr::Var("x".into())))),
            def("f", Expr::Lambda("y".into(), Box::new(
                Expr::App(Box::new(Expr::Var("id".into())), Box::new(Expr::Var("y".into()))),
            ))),
        ];
        let result = InlineSmallGlobals.run(defs);
        let expected_f = Expr::Lambda("y".into(), Box::new(
            Expr::App(
                Box::new(Expr::Lambda("x".into(), Box::new(Expr::Var("x".into())))),
                Box::new(Expr::Var("y".into())),
            ),
        ));
        assert_eq!(result[1].body, expected_f);
    }

    #[test]
    fn skip_recursive_global() {
        let defs = vec![
            def("loop", Expr::Lambda("x".into(), Box::new(
                Expr::App(Box::new(Expr::Var("loop".into())), Box::new(Expr::Var("x".into()))),
            ))),
        ];
        let expected = defs.clone();
        let result = InlineSmallGlobals.run(defs);
        assert_eq!(result[0].body, expected[0].body);
    }

    #[test]
    fn skip_large_global() {
        let big_body = Expr::Lambda("a".into(), Box::new(
            Expr::Lambda("b".into(), Box::new(
                Expr::Lambda("c".into(), Box::new(
                    Expr::App(Box::new(Expr::Var("a".into())), Box::new(
                        Expr::App(Box::new(Expr::Var("b".into())), Box::new(Expr::Var("c".into()))),
                    )),
                )),
            )),
        ));
        let defs = vec![
            def("big", big_body.clone()),
            def("f", Expr::Var("big".into())),
        ];
        let result = InlineSmallGlobals.run(defs);
        assert_eq!(result[1].body, Expr::Var("big".into()));
    }
}
