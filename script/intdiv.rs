// intdiv -- (S1) the warning of the language services for a division of two ints: `/` on two
// ints truncates (7 / 2 is 3; G7.2 of docs/RICHTUNG keeps that), the editor says so
//
//   warnings(&program) -> Vec<Diagnostic>   a warning at every `a / b` and `x /= b` whose two
//                        operands are certainly ints, with the fix: float(a) / b for a float,
//                        int(a / b) to truncate on purpose (no warning inside int(..))
//
// Certainly an int (never a false warning): int literals; len(..) and int(..) where the name is
// the built-in (not a variable of the script); -a and a + b, a - b, a * b, a / b, a % b of such
// values; a ?? b and (S3) c ? a : b when both are; variables whose every assignment in the file
// is such a value
// (let, =, +=, -=, *=, /=, %=, in nested functions too) and the variable of a for loop over a
// range a..b.  A variable may also be nil (before its let ran, a let without a value): that
// throws in arithmetic, so a division that gives a value has two ints.  Parameters, for
// variables over lists, catch variables, fields, indexes and calls of functions are never
// certain; (S3) nor are the names of patterns (destructured items, keys and parameters, their
// defaults or not) and the variables of a comprehension over a list (one over a range a..b is,
// as a for loop's).  A division of two literals that is exact (6 / 2) or by 0 gets no warning.  The file
// is the unit, as for all of check: a later script of Vm::add_script or a host's set_global
// could change a top-level variable unseen.
//
// The variables' types are a greatest fixpoint: every candidate starts as int and loses it
// when one of its values is not certainly int (a worklist over the variables a value reads).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::ast::*;
use crate::Diagnostic;

/// a value as far as the warning cares
enum K {
    /// certainly an int
    Int,
    /// the literal nil (a variable may hold it, arithmetic throws on it)
    Nil,
    /// a variable (its declaration)
    Var(usize),
    /// -a
    Neg(Box<K>),
    /// a + - * / % b
    Arith(Box<K>, Box<K>),
    /// a ?? b
    Coalesce(Box<K>, Box<K>),
    /// (S3) c ? a : b
    Either(Box<K>, Box<K>),
    Unknown,
}

/// what a value is: an int, an int or nil, or anything
#[derive(Clone, Copy, PartialEq, Eq)]
enum Is {
    Int,
    IntOrNil,
    Any,
}

impl K {
    fn eval(&self, int: &[bool]) -> Is {
        match self {
            K::Int => Is::Int,
            K::Nil => Is::IntOrNil,
            K::Var(d) => {
                if int[*d] { Is::IntOrNil } else { Is::Any }
            }
            K::Neg(a) => {
                if a.eval(int) == Is::Any { Is::Any } else { Is::Int }
            }
            K::Arith(a, b) => {
                if a.eval(int) == Is::Any || b.eval(int) == Is::Any { Is::Any } else { Is::Int }
            }
            K::Coalesce(a, b) => match a.eval(int) {
                Is::Any => Is::Any,
                Is::Int => Is::Int,
                Is::IntOrNil => b.eval(int),
            },
            K::Either(a, b) => match (a.eval(int), b.eval(int)) {
                (Is::Any, _) | (_, Is::Any) => Is::Any,
                (Is::Int, Is::Int) => Is::Int,
                _ => Is::IntOrNil,
            },
            K::Unknown => Is::Any,
        }
    }

    /// the declarations it reads
    fn vars(&self, out: &mut Vec<usize>) {
        match self {
            K::Var(d) => out.push(*d),
            K::Neg(a) => a.vars(out),
            K::Arith(a, b) | K::Coalesce(a, b) | K::Either(a, b) => {
                a.vars(out);
                b.vars(out);
            }
            _ => {}
        }
    }
}

/// a declaration: whether it can be an int variable, the values assigned to it
struct Decl {
    cand: bool,
    values: Vec<K>,
}

/// a division to judge once the variables are known
struct Div<'a> {
    a: K,
    b: K,
    lhs: &'a Expr,
    rhs: &'a Expr,
    /// x /= b: the variable's name
    assign: Option<&'a str>,
    /// where the warning goes: the start of a / b (of x /= b)
    at: Span,
}

struct Walk<'a> {
    top: Vec<(&'a str, usize)>,
    scopes: Vec<Vec<(&'a str, usize)>>,
    decls: Vec<Decl>,
    divs: Vec<Div<'a>>,
}

/// see the module comment
pub(crate) fn warnings(prog: &Program) -> Vec<Diagnostic> {
    let mut w = Walk { top: Vec::new(), scopes: Vec::new(), decls: Vec::new(), divs: Vec::new() };
    for s in &prog.body {
        match &s.kind {
            StmtKind::Let { pat, .. } => {
                // (S3) a destructured name is never certain
                let mut names = Vec::new();
                pat.names(&mut names);
                let cand = matches!(pat.kind, PatKind::Name(_));
                for name in names {
                    if !w.top.iter().any(|t| t.0 == name.text) {
                        let d = w.decl(cand);
                        w.top.push((name.text.as_str(), d));
                    }
                }
            }
            StmtKind::Fn(FnDef { name: Some(name), .. }) => {
                if !w.top.iter().any(|t| t.0 == name.text) {
                    let d = w.decl(false);
                    w.top.push((name.text.as_str(), d));
                }
            }
            _ => {}
        }
    }
    for s in &prog.body {
        w.stmt(s, true);
    }
    let int = w.fixpoint();
    let mut out = Vec::new();
    for d in &w.divs {
        let (ea, eb) = (d.a.eval(&int), d.b.eval(&int));
        if ea == Is::Any || eb == Is::Any || is_nil(d.lhs) || is_nil(d.rhs) || exact(d.lhs, d.rhs) {
            continue;
        }
        let (l, r) = match (text(d.lhs), text(d.rhs)) {
            (Some(l), Some(r)) => (l, r),
            _ => (String::from("a"), String::from("b")),
        };
        // the right side in parentheses when it is an operation; the left side when it binds
        // looser than / (in float(..) it needs none)
        let r = if matches!(d.rhs.kind, ExprKind::Binary(..)) { format!("({})", r) } else { r };
        let loose = matches!(&d.lhs.kind, ExprKind::Binary(op, ..) if !matches!(op, BinOp::Mul | BinOp::Div | BinOp::Rem));
        let lp = if loose { format!("({})", l) } else { l.clone() };
        let msg = match d.assign {
            Some(x) => tf!(
                "Ganzzahlige Division: {x} /= {r} schneidet ab (7 / 2 ist 3) – {x} = float({x}) / {r} rechnet mit Komma, {x} = int({x} / {r}) schneidet ausdrücklich ab",
                x = x,
                r = r
            ),
            None => tf!("Ganzzahlige Division: {lp} / {r} schneidet ab (7 / 2 ist 3) – float({l}) / {r} rechnet mit Komma, int({lp} / {r}) schneidet ausdrücklich ab", lp = lp, r = r, l = l),
        };
        // to the end of the right operand when it is on the same line
        let end = if d.rhs.span.line == d.at.line { d.rhs.span.col + width(d.rhs) } else { d.at.col + 1 };
        out.push(Diagnostic { line: d.at.line, col: d.at.col, end_col: end.max(d.at.col + 1), error: false, msg, file: String::new() });
    }
    out
}

/// whether e is the literal nil
fn is_nil(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::Nil)
}

/// an int literal, also with a minus
fn literal(e: &Expr) -> Option<i64> {
    match &e.kind {
        ExprKind::Int(v) => Some(*v),
        ExprKind::Unary(UnOp::Neg, a) => literal(a).map(i64::wrapping_neg),
        _ => None,
    }
}

/// two literals that divide without a rest, or a division by 0 (an error, not a truncation)
fn exact(a: &Expr, b: &Expr) -> bool {
    match (literal(a), literal(b)) {
        (Some(x), Some(y)) => y == 0 || x.wrapping_rem(y) == 0,
        _ => false,
    }
}

/// the characters of e on its line, as far as they are known (names exactly, the rest by bytes)
fn width(e: &Expr) -> u32 {
    match &e.kind {
        ExprKind::Var(n) => n.chars().count() as u32,
        _ => e.span.end.saturating_sub(e.span.start).max(1),
    }
}

/// e as short source text for the message (None when it is long or not simple)
fn text(e: &Expr) -> Option<String> {
    let t = text_of(e, 0)?;
    if t.chars().count() <= 24 { Some(t) } else { None }
}

fn text_of(e: &Expr, depth: u32) -> Option<String> {
    if depth > 3 {
        return None;
    }
    Some(match &e.kind {
        ExprKind::Int(v) => v.to_string(),
        ExprKind::Nil => String::from("nil"),
        ExprKind::Bool(b) => String::from(if *b { "true" } else { "false" }),
        ExprKind::Str(s) if !s.contains(['"', '\\', '\n', '$']) => format!("\"{}\"", s),
        ExprKind::Var(n) => n.clone(),
        ExprKind::Unary(UnOp::Neg, a) => match a.kind {
            ExprKind::Binary(..) => format!("-({})", text_of(a, depth + 1)?),
            _ => format!("-{}", text_of(a, depth + 1)?),
        },
        ExprKind::Field(a, n) => format!("{}.{}", text_of(a, depth + 1)?, n.text),
        ExprKind::Call(f, args) if args.len() <= 2 => {
            let a: Option<Vec<String>> = args.iter().map(|x| text_of(x, depth + 1)).collect();
            format!("{}({})", text_of(f, depth + 1)?, a?.join(", "))
        }
        ExprKind::Method(o, n, args) if args.len() <= 2 => {
            let a: Option<Vec<String>> = args.iter().map(|x| text_of(x, depth + 1)).collect();
            format!("{}.{}({})", text_of(o, depth + 1)?, n.text, a?.join(", "))
        }
        ExprKind::Binary(op, a, b) => {
            let side = |x: &Expr| -> Option<String> {
                let t = text_of(x, depth + 1)?;
                Some(if matches!(x.kind, ExprKind::Binary(..)) { format!("({})", t) } else { t })
            };
            format!("{} {} {}", side(a)?, op.text(), side(b)?)
        }
        _ => return None,
    })
}

impl<'a> Walk<'a> {
    fn decl(&mut self, cand: bool) -> usize {
        self.decls.push(Decl { cand, values: Vec::new() });
        self.decls.len() - 1
    }

    /// a name in the innermost scope
    fn declare(&mut self, n: &'a str, cand: bool) -> usize {
        let d = self.decl(cand);
        if let Some(s) = self.scopes.last_mut() {
            s.push((n, d));
        }
        d
    }

    fn resolve(&self, n: &str) -> Option<usize> {
        for s in self.scopes.iter().rev() {
            if let Some(&(_, d)) = s.iter().rev().find(|x| x.0 == n) {
                return Some(d);
            }
        }
        self.top.iter().find(|x| x.0 == n).map(|x| x.1)
    }

    /// the greatest fixpoint of "certainly an int" over the declarations
    fn fixpoint(&self) -> Vec<bool> {
        let n = self.decls.len();
        let mut int: Vec<bool> = self.decls.iter().map(|d| d.cand).collect();
        let mut readers: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut vars = Vec::new();
        for (i, d) in self.decls.iter().enumerate() {
            for k in &d.values {
                vars.clear();
                k.vars(&mut vars);
                for &v in &vars {
                    readers[v].push(i);
                }
            }
        }
        let mut work: Vec<usize> = (0..n).filter(|i| int[*i]).collect();
        while let Some(i) = work.pop() {
            if !int[i] || self.decls[i].values.iter().all(|k| k.eval(&int) != Is::Any) {
                continue;
            }
            int[i] = false;
            work.extend(readers[i].iter().copied().filter(|j| int[*j]));
        }
        int
    }

    fn block(&mut self, b: &'a Block) {
        self.scopes.push(Vec::new());
        for s in &b.stmts {
            if let StmtKind::Fn(FnDef { name: Some(n), .. }) = &s.kind {
                self.declare(&n.text, false);
            }
        }
        for s in &b.stmts {
            self.stmt(s, false);
        }
        self.scopes.pop();
    }

    fn func(&mut self, f: &'a FnDef) {
        self.scopes.push(Vec::new());
        for p in &f.params {
            self.pattern(p, false);
        }
        self.block(&f.body);
        self.scopes.pop();
    }

    /// (S3) a pattern's defaults and names: never certain (top: a top-level let's, declared
    /// already: they get an unknown value)
    fn pattern(&mut self, p: &'a Pattern, top: bool) {
        match &p.kind {
            PatKind::Name(n) => {
                let d = if top { self.resolve(&n.text) } else { Some(self.declare(&n.text, false)) };
                if let Some(d) = d {
                    self.decls[d].values.push(K::Unknown);
                }
            }
            PatKind::List(items, rest) => {
                for x in items {
                    self.pattern(x, top);
                }
                if let Some(r) = rest {
                    self.pattern(r, top);
                }
            }
            PatKind::Map(entries) => {
                for (_, x) in entries {
                    self.pattern(x, top);
                }
            }
            PatKind::Default(x, e) => {
                self.expr(e, false);
                self.pattern(x, top);
            }
            PatKind::Target(t) => match &t.kind {
                ExprKind::Var(n) => {
                    if let Some(d) = self.resolve(n) {
                        self.decls[d].values.push(K::Unknown);
                    }
                }
                _ => {
                    self.expr(t, false);
                }
            },
        }
    }

    /// the variables of a for loop or clause (a scope is open): over a range one name is
    /// certainly an int
    fn for_vars(&mut self, a: &'a Pattern, b: Option<&'a Pattern>, src: &'a ForSrc) {
        let range = match src {
            ForSrc::Each(e) => {
                self.expr(e, false);
                false
            }
            ForSrc::Range(lo, hi) => {
                self.expr(lo, false);
                self.expr(hi, false);
                true
            }
        };
        match (&a.kind, range && b.is_none()) {
            (PatKind::Name(n), true) => {
                let d = self.declare(&n.text, true);
                self.decls[d].values.push(K::Int);
            }
            _ => self.pattern(a, false),
        }
        if let Some(b) = b {
            self.pattern(b, false);
        }
    }

    fn stmt(&mut self, s: &'a Stmt, top: bool) {
        match &s.kind {
            StmtKind::Let { pat: Pattern { kind: PatKind::Name(name), .. }, init } => {
                let k = match init {
                    Some(e) => self.expr(e, false),
                    None => K::Nil,
                };
                let d = if top { self.resolve(&name.text) } else { Some(self.declare(&name.text, true)) };
                if let Some(d) = d {
                    self.decls[d].values.push(k);
                }
            }
            StmtKind::Let { pat, init } => {
                if let Some(e) = init {
                    self.expr(e, false);
                }
                self.pattern(pat, top);
            }
            StmtKind::AssignPat { pat, value } => {
                self.expr(value, false);
                self.pattern(pat, false);
            }
            StmtKind::Fn(f) => self.func(f),
            StmtKind::If { arms, els } => {
                for (c, b) in arms {
                    self.expr(c, false);
                    self.block(b);
                }
                if let Some(b) = els {
                    self.block(b);
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond, false);
                self.block(body);
            }
            StmtKind::For { a, b, src, body } => {
                // (the source is walked before the variables are declared)
                self.scopes.push(Vec::new());
                self.for_vars(a, b.as_ref(), src);
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::Try { body, var, catch } => {
                self.block(body);
                self.scopes.push(Vec::new());
                if let Some(v) = var {
                    self.declare(&v.text, false);
                }
                self.block(catch);
                self.scopes.pop();
            }
            StmtKind::Throw(e) | StmtKind::Expr(e) | StmtKind::Return(Some(e)) => {
                self.expr(e, false);
            }
            StmtKind::Assign { op, target, value } => {
                let kv = self.expr(value, false);
                let ExprKind::Var(n) = &target.kind else {
                    self.expr(target, false);
                    return;
                };
                let Some(d) = self.resolve(n) else { return };
                let k = match op {
                    None => kv,
                    Some(BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem) => {
                        if *op == Some(BinOp::Div) {
                            self.divs.push(Div { a: K::Var(d), b: copy(&kv), lhs: target, rhs: value, assign: Some(n), at: s.span });
                        }
                        K::Arith(Box::new(K::Var(d)), Box::new(kv))
                    }
                    Some(_) => K::Unknown,
                };
                self.decls[d].values.push(k);
            }
            StmtKind::Block(b) => self.block(b),
            StmtKind::Break | StmtKind::Continue | StmtKind::Return(None) => {}
        }
    }

    /// what e is; quiet: e is the argument of int(..), its division truncates on purpose
    fn expr(&mut self, e: &'a Expr, quiet: bool) -> K {
        match &e.kind {
            ExprKind::Int(_) => K::Int,
            ExprKind::Nil => K::Nil,
            ExprKind::Bool(_) | ExprKind::Float(_) | ExprKind::Str(_) => K::Unknown,
            ExprKind::Interp(items) | ExprKind::List(items) => {
                for x in items {
                    self.expr(x, false);
                }
                K::Unknown
            }
            ExprKind::Map(kv) => {
                for (k, v) in kv {
                    self.expr(k, false);
                    self.expr(v, false);
                }
                K::Unknown
            }
            ExprKind::Fn(f) => {
                self.func(f);
                K::Unknown
            }
            ExprKind::Var(n) => self.resolve(n).map_or(K::Unknown, K::Var),
            ExprKind::Unary(op, a) => {
                let k = self.expr(a, false);
                if *op == UnOp::Neg { K::Neg(Box::new(k)) } else { K::Unknown }
            }
            ExprKind::Binary(op, a, b) => {
                let (ka, kb) = (self.expr(a, false), self.expr(b, false));
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Rem => K::Arith(Box::new(ka), Box::new(kb)),
                    BinOp::Div => {
                        if !quiet {
                            self.divs.push(Div { a: copy(&ka), b: copy(&kb), lhs: a, rhs: b, assign: None, at: e.span });
                        }
                        K::Arith(Box::new(ka), Box::new(kb))
                    }
                    BinOp::Coalesce => K::Coalesce(Box::new(ka), Box::new(kb)),
                    _ => K::Unknown,
                }
            }
            ExprKind::Call(f, args) => {
                let builtin = match &f.kind {
                    ExprKind::Var(n) if (n == "len" || n == "int") && self.resolve(n).is_none() => Some(n.as_str()),
                    _ => None,
                };
                if builtin.is_none() {
                    self.expr(f, false);
                }
                let one = args.len() == 1;
                for x in args {
                    self.expr(x, builtin == Some("int") && one);
                }
                if builtin.is_some() { K::Int } else { K::Unknown }
            }
            ExprKind::Method(o, _, args) => {
                self.expr(o, false);
                for x in args {
                    self.expr(x, false);
                }
                K::Unknown
            }
            ExprKind::Field(a, _) => {
                self.expr(a, false);
                K::Unknown
            }
            ExprKind::Index(a, b) => {
                self.expr(a, false);
                self.expr(b, false);
                K::Unknown
            }
            // (S3) c ? a : b is certainly an int when both are
            ExprKind::Cond(c, a, b) => {
                self.expr(c, false);
                let (ka, kb) = (self.expr(a, false), self.expr(b, false));
                K::Either(Box::new(ka), Box::new(kb))
            }
            ExprKind::Named(_, v) => self.expr(v, quiet),
            ExprKind::Comp(x, clauses) => {
                let n = self.scopes.len();
                for c in clauses {
                    match c {
                        Clause::For { a, b, src } => {
                            self.scopes.push(Vec::new());
                            self.for_vars(a, b.as_ref(), src);
                        }
                        Clause::If(c) => {
                            self.expr(c, false);
                        }
                    }
                }
                self.expr(x, false);
                self.scopes.truncate(n);
                K::Unknown
            }
        }
    }
}

/// a copy of a K (it holds no references)
fn copy(k: &K) -> K {
    match k {
        K::Int => K::Int,
        K::Nil => K::Nil,
        K::Var(d) => K::Var(*d),
        K::Neg(a) => K::Neg(Box::new(copy(a))),
        K::Arith(a, b) => K::Arith(Box::new(copy(a)), Box::new(copy(b))),
        K::Coalesce(a, b) => K::Coalesce(Box::new(copy(a)), Box::new(copy(b))),
        K::Either(a, b) => K::Either(Box::new(copy(a)), Box::new(copy(b))),
        K::Unknown => K::Unknown,
    }
}
