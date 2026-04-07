#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimOp {
    Add, Sub, Mul, Div, Neg, Eq, Lt,
    BytesLen, BytesGet, BytesEq, BytesCat,
}

/// High-level IR after desugaring, before variable resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Var(String),
    Int(i32),
    Bytes(Vec<u8>),
    /// Nullary or N-ary constructor application via quasiquote: `(Tag field...)
    Ctor(String, Vec<Expr>),
    PrimOp(PrimOp, Vec<Expr>),
    Lambda(String, Box<Expr>),
    Lambdas(Vec<String>, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    AppN(Box<Expr>, Vec<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    Letrec(String, Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<MatchCase>),
    /// Nat eliminator: CaseNat(zero_case, succ_case, scrutinee)
    CaseNat(Box<Expr>, Box<Expr>, Box<Expr>),
    Error,
    /// A host-provided foreign function, identified by its registration index.
    Foreign(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchCase {
    pub tag: String,
    pub bindings: Vec<String>,
    pub body: Expr,
}

/// Top-level definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Define {
    pub name: String,
    pub body: Expr,
}

/// Resolved IR: variables are indices, constructors are tag IDs.
#[derive(Debug, Clone, PartialEq)]
pub enum RExpr {
    /// Local variable, de Bruijn index (0 = innermost)
    Local(u8),
    /// Global variable by slot index
    Global(u16),
    Int(i32),
    Bytes(Vec<u8>),
    /// Constructor: tag id + resolved field exprs
    Ctor(u8, Vec<RExpr>),
    PrimOp(PrimOp, Vec<RExpr>),
    Lambda(Box<RExpr>),
    Lambdas(u8, Box<RExpr>),
    App(Box<RExpr>, Box<RExpr>),
    AppN(Box<RExpr>, Vec<RExpr>),
    Let(Box<RExpr>, Box<RExpr>),
    Letrec(Box<RExpr>, Box<RExpr>),
    Match(Box<RExpr>, Vec<RMatchCase>),
    /// Nat eliminator: CaseNat(zero_case, succ_case, scrutinee)
    CaseNat(Box<RExpr>, Box<RExpr>, Box<RExpr>),
    Error,
    /// A host-provided foreign function, identified by its registration index.
    Foreign(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RMatchCase {
    pub tag: u8,
    pub arity: u8,
    pub body: RExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RDefine {
    pub name: String,
    pub global_idx: u16,
    pub body: RExpr,
}

/// By-reference visitor for read-only walks over `Expr`.
///
/// Override `visit_expr_ref` to inspect specific variants, delegating the rest
/// to `walk_expr_ref` which recurses into all children.
pub trait ExprRefVisitor {
    fn visit_expr_ref(&mut self, expr: &Expr) {
        self.walk_expr_ref(expr);
    }

    fn walk_expr_ref(&mut self, expr: &Expr) {
        match expr {
            Expr::Var(_) | Expr::Int(_) | Expr::Bytes(_) | Expr::Error | Expr::Foreign(_) => {}
            Expr::Ctor(_, fields) => {
                for f in fields { self.visit_expr_ref(f); }
            }
            Expr::PrimOp(_, args) => {
                for a in args { self.visit_expr_ref(a); }
            }
            Expr::Lambda(_, body) => self.visit_expr_ref(body),
            Expr::Lambdas(_, body) => self.visit_expr_ref(body),
            Expr::App(f, a) => {
                self.visit_expr_ref(f);
                self.visit_expr_ref(a);
            }
            Expr::AppN(f, args) => {
                self.visit_expr_ref(f);
                for a in args { self.visit_expr_ref(a); }
            }
            Expr::If(c, t, e) => {
                self.visit_expr_ref(c);
                self.visit_expr_ref(t);
                self.visit_expr_ref(e);
            }
            Expr::Let(_, val, body) | Expr::Letrec(_, val, body) => {
                self.visit_expr_ref(val);
                self.visit_expr_ref(body);
            }
            Expr::Match(scrut, cases) => {
                self.visit_expr_ref(scrut);
                for c in cases { self.visit_expr_ref(&c.body); }
            }
            Expr::CaseNat(zc, sc, scrut) => {
                self.visit_expr_ref(zc);
                self.visit_expr_ref(sc);
                self.visit_expr_ref(scrut);
            }
        }
    }
}

/// Owned transformation visitor for `Expr`.
///
/// Override `visit_expr` to handle specific variants, delegating the rest to
/// `walk_expr` which performs the default recursive descent.
pub trait ExprVisitor {
    fn visit_expr(&mut self, expr: Expr) -> Expr {
        self.walk_expr(expr)
    }

    fn walk_expr(&mut self, expr: Expr) -> Expr {
        match expr {
            Expr::Ctor(tag, fields) => {
                Expr::Ctor(tag, fields.into_iter().map(|f| self.visit_expr(f)).collect())
            }
            Expr::PrimOp(op, args) => {
                Expr::PrimOp(op, args.into_iter().map(|a| self.visit_expr(a)).collect())
            }
            Expr::Lambda(p, body) => Expr::Lambda(p, Box::new(self.visit_expr(*body))),
            Expr::Lambdas(ps, body) => Expr::Lambdas(ps, Box::new(self.visit_expr(*body))),
            Expr::App(f, a) => {
                Expr::App(Box::new(self.visit_expr(*f)), Box::new(self.visit_expr(*a)))
            }
            Expr::AppN(f, args) => Expr::AppN(
                Box::new(self.visit_expr(*f)),
                args.into_iter().map(|a| self.visit_expr(a)).collect(),
            ),
            Expr::If(c, t, e) => Expr::If(
                Box::new(self.visit_expr(*c)),
                Box::new(self.visit_expr(*t)),
                Box::new(self.visit_expr(*e)),
            ),
            Expr::Let(name, val, body) => Expr::Let(
                name,
                Box::new(self.visit_expr(*val)),
                Box::new(self.visit_expr(*body)),
            ),
            Expr::Letrec(name, val, body) => Expr::Letrec(
                name,
                Box::new(self.visit_expr(*val)),
                Box::new(self.visit_expr(*body)),
            ),
            Expr::Match(scrut, cases) => Expr::Match(
                Box::new(self.visit_expr(*scrut)),
                cases
                    .into_iter()
                    .map(|c| MatchCase {
                        tag: c.tag,
                        bindings: c.bindings,
                        body: self.visit_expr(c.body),
                    })
                    .collect(),
            ),
            Expr::CaseNat(zc, sc, scrut) => Expr::CaseNat(
                Box::new(self.visit_expr(*zc)),
                Box::new(self.visit_expr(*sc)),
                Box::new(self.visit_expr(*scrut)),
            ),
            other => other,
        }
    }

    fn visit_define(&mut self, d: Define) -> Define {
        Define {
            name: d.name,
            body: self.visit_expr(d.body),
        }
    }

    fn visit_program(&mut self, defs: Vec<Define>) -> Vec<Define> {
        defs.into_iter().map(|d| self.visit_define(d)).collect()
    }
}

/// Owned transformation visitor for `RExpr`.
///
/// Override `visit_rexpr` to handle specific variants, delegating the rest to
/// `walk_rexpr` which performs the default recursive descent.
pub trait RExprVisitor {
    fn visit_rexpr(&mut self, expr: RExpr) -> RExpr {
        self.walk_rexpr(expr)
    }

    fn walk_rexpr(&mut self, expr: RExpr) -> RExpr {
        match expr {
            RExpr::Ctor(tag, fields) => {
                RExpr::Ctor(tag, fields.into_iter().map(|f| self.visit_rexpr(f)).collect())
            }
            RExpr::PrimOp(op, args) => {
                RExpr::PrimOp(op, args.into_iter().map(|a| self.visit_rexpr(a)).collect())
            }
            RExpr::Lambda(body) => RExpr::Lambda(Box::new(self.visit_rexpr(*body))),
            RExpr::Lambdas(n, body) => RExpr::Lambdas(n, Box::new(self.visit_rexpr(*body))),
            RExpr::App(f, a) => {
                RExpr::App(Box::new(self.visit_rexpr(*f)), Box::new(self.visit_rexpr(*a)))
            }
            RExpr::AppN(f, args) => RExpr::AppN(
                Box::new(self.visit_rexpr(*f)),
                args.into_iter().map(|a| self.visit_rexpr(a)).collect(),
            ),
            RExpr::Let(val, body) => RExpr::Let(
                Box::new(self.visit_rexpr(*val)),
                Box::new(self.visit_rexpr(*body)),
            ),
            RExpr::Letrec(val, body) => RExpr::Letrec(
                Box::new(self.visit_rexpr(*val)),
                Box::new(self.visit_rexpr(*body)),
            ),
            RExpr::Match(scrut, cases) => RExpr::Match(
                Box::new(self.visit_rexpr(*scrut)),
                cases
                    .into_iter()
                    .map(|c| RMatchCase {
                        tag: c.tag,
                        arity: c.arity,
                        body: self.visit_rexpr(c.body),
                    })
                    .collect(),
            ),
            RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
                Box::new(self.visit_rexpr(*zc)),
                Box::new(self.visit_rexpr(*sc)),
                Box::new(self.visit_rexpr(*scrut)),
            ),
            other => other,
        }
    }

    fn visit_rdefine(&mut self, d: RDefine) -> RDefine {
        RDefine {
            name: d.name,
            global_idx: d.global_idx,
            body: self.visit_rexpr(d.body),
        }
    }

    fn visit_rprogram(&mut self, defs: Vec<RDefine>) -> Vec<RDefine> {
        defs.into_iter().map(|d| self.visit_rdefine(d)).collect()
    }
}
