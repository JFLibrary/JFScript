// awaitchk -- (S3) the warning of check and the language services for an await that cannot
// wait: list.map, list.filter, list.reduce and list.sort run their function through Cx::call to
// its end, so an await of a promise that is not settled yet throws there at run time (task.rs)
//
//   warnings(&program) -> Vec<Diagnostic>   a warning at every await directly in a fn literal
//                        that is an argument of .map / .filter / .reduce / .sort (a method call,
//                        or list.map(xs, f)), and at the name of a top-level fn with such an
//                        await passed there (when nothing else in the file has that name)
//
// "Directly": not inside a further fn literal or declaration (that one runs when it is called,
// maybe elsewhere).  The methods are taken by their name whatever the receiver is: a host's
// method of that name runs its function the same way.  Other functions of a host that take
// one may run it through Cx::call (no wait) or later as an invocation of its own (await works),
// so they get no warning; neither does obj.on(..), whose listeners are invocations.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::ast::*;
use crate::Diagnostic;

/// the library's functions that call their function to its end
const CALLERS: &[&str] = &["map", "filter", "reduce", "sort"];

/// see the module comment
pub(crate) fn warnings(prog: &Program) -> Vec<Diagnostic> {
    let mut w = W { decls: BTreeMap::new(), top: BTreeMap::new(), out: Vec::new() };
    for s in &prog.body {
        w.declared(s);
    }
    for s in &prog.body {
        if let StmtKind::Fn(FnDef { name: Some(n), body, .. }) = &s.kind {
            let mut at = Vec::new();
            awaits_block(body, &mut at);
            if !at.is_empty() {
                w.top.insert(n.text.as_str(), at.len());
            }
        }
    }
    for s in &prog.body {
        w.stmt(s);
    }
    w.out
}

struct W<'a> {
    /// how often each name is declared in the file (let, fn, parameters, for and catch)
    decls: BTreeMap<&'a str, u32>,
    /// the top-level fns with an await directly in their body
    top: BTreeMap<&'a str, usize>,
    out: Vec<Diagnostic>,
}

/// a diagnostic over the first line of a span (the await keyword: 5 characters)
fn warn_at(sp: Span, len: u32, msg: String) -> Diagnostic {
    Diagnostic { line: sp.line, col: sp.col, end_col: sp.col + len.max(1), error: false, msg, file: String::new() }
}

/// the awaits directly in a block (not in fns inside it)
fn awaits_block(b: &Block, out: &mut Vec<Span>) {
    for s in &b.stmts {
        awaits_stmt(s, out);
    }
}

fn awaits_stmt(s: &Stmt, out: &mut Vec<Span>) {
    match &s.kind {
        StmtKind::Let { init: Some(e), .. } | StmtKind::Return(Some(e)) | StmtKind::Throw(e) | StmtKind::Expr(e) => awaits_expr(e, out),
        StmtKind::If { arms, els } => {
            for (c, b) in arms {
                awaits_expr(c, out);
                awaits_block(b, out);
            }
            if let Some(b) = els {
                awaits_block(b, out);
            }
        }
        StmtKind::While { cond, body } => {
            awaits_expr(cond, out);
            awaits_block(body, out);
        }
        StmtKind::For { src, body, .. } => {
            match src {
                ForSrc::Each(e) => awaits_expr(e, out),
                ForSrc::Range(a, b) => {
                    awaits_expr(a, out);
                    awaits_expr(b, out);
                }
            }
            awaits_block(body, out);
        }
        StmtKind::Try { body, catch, .. } => {
            awaits_block(body, out);
            awaits_block(catch, out);
        }
        StmtKind::Assign { target, value, .. } => {
            awaits_expr(target, out);
            awaits_expr(value, out);
        }
        StmtKind::Block(b) => awaits_block(b, out),
        _ => {}
    }
}

fn awaits_expr(e: &Expr, out: &mut Vec<Span>) {
    match &e.kind {
        ExprKind::Interp(v) | ExprKind::List(v) => v.iter().for_each(|x| awaits_expr(x, out)),
        ExprKind::Map(kv) => kv.iter().for_each(|(k, v)| {
            awaits_expr(k, out);
            awaits_expr(v, out);
        }),
        ExprKind::Unary(op, a) => {
            if *op == UnOp::Await {
                out.push(e.span);
            }
            awaits_expr(a, out);
        }
        ExprKind::Field(a, _) => awaits_expr(a, out),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
            awaits_expr(a, out);
            awaits_expr(b, out);
        }
        ExprKind::Call(f, args) | ExprKind::Method(f, _, args) => {
            awaits_expr(f, out);
            args.iter().for_each(|x| awaits_expr(x, out));
        }
        _ => {}
    }
}

impl<'a> W<'a> {
    fn decl(&mut self, n: &'a Name) {
        *self.decls.entry(n.text.as_str()).or_insert(0) += 1;
    }

    /// count the declarations of a statement and everything in it
    fn declared(&mut self, s: &'a Stmt) {
        let mut stmts: Vec<&'a Stmt> = Vec::new();
        let mut exprs: Vec<&'a Expr> = Vec::new();
        stmts.push(s);
        while !stmts.is_empty() || !exprs.is_empty() {
            if let Some(s) = stmts.pop() {
                match &s.kind {
                    StmtKind::Let { name, init } => {
                        self.decl(name);
                        exprs.extend(init.iter());
                    }
                    StmtKind::Fn(f) => {
                        if let Some(n) = &f.name {
                            self.decl(n);
                        }
                        f.params.iter().for_each(|p| self.decl(p));
                        stmts.extend(f.body.stmts.iter());
                    }
                    StmtKind::If { arms, els } => {
                        for (c, b) in arms {
                            exprs.push(c);
                            stmts.extend(b.stmts.iter());
                        }
                        if let Some(b) = els {
                            stmts.extend(b.stmts.iter());
                        }
                    }
                    StmtKind::While { cond, body } => {
                        exprs.push(cond);
                        stmts.extend(body.stmts.iter());
                    }
                    StmtKind::For { a, b, src, body } => {
                        self.decl(a);
                        if let Some(b) = b {
                            self.decl(b);
                        }
                        match src {
                            ForSrc::Each(e) => exprs.push(e),
                            ForSrc::Range(x, y) => {
                                exprs.push(x);
                                exprs.push(y);
                            }
                        }
                        stmts.extend(body.stmts.iter());
                    }
                    StmtKind::Try { body, var, catch } => {
                        if let Some(v) = var {
                            self.decl(v);
                        }
                        stmts.extend(body.stmts.iter());
                        stmts.extend(catch.stmts.iter());
                    }
                    StmtKind::Return(Some(e)) | StmtKind::Throw(e) | StmtKind::Expr(e) => exprs.push(e),
                    StmtKind::Assign { target, value, .. } => {
                        exprs.push(target);
                        exprs.push(value);
                    }
                    StmtKind::Block(b) => stmts.extend(b.stmts.iter()),
                    _ => {}
                }
            } else if let Some(e) = exprs.pop() {
                match &e.kind {
                    ExprKind::Interp(v) | ExprKind::List(v) => exprs.extend(v.iter()),
                    ExprKind::Map(kv) => kv.iter().for_each(|(k, v)| {
                        exprs.push(k);
                        exprs.push(v);
                    }),
                    ExprKind::Fn(f) => {
                        f.params.iter().for_each(|p| self.decl(p));
                        stmts.extend(f.body.stmts.iter());
                    }
                    ExprKind::Unary(_, a) | ExprKind::Field(a, _) => exprs.push(a),
                    ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                        exprs.push(a);
                        exprs.push(b);
                    }
                    ExprKind::Call(f, args) | ExprKind::Method(f, _, args) => {
                        exprs.push(f);
                        exprs.extend(args.iter());
                    }
                    _ => {}
                }
            }
        }
    }

    /// the functions given to a caller (`how`: "list.map" as written for the message)
    fn callbacks(&mut self, how: &str, args: &[Expr]) {
        for a in args {
            match &a.kind {
                ExprKind::Fn(f) => {
                    let mut at = Vec::new();
                    awaits_block(&f.body, &mut at);
                    for sp in at {
                        let m = format!(
                            "await im Rückruf von {}: der läuft ohne Unterbrechung zu Ende, ein Wert, der noch aussteht, gibt hier einen Fehler – warte außerhalb, etwa in einer for-Schleife",
                            how
                        );
                        self.out.push(warn_at(sp, 5, m));
                    }
                }
                ExprKind::Var(n) if self.top.contains_key(n.as_str()) && self.decls.get(n.as_str()) == Some(&1) => {
                    let m = format!(
                        "{} wartet mit await, {} ruft es aber ohne Unterbrechung auf: ein Wert, der noch aussteht, gibt dort einen Fehler – warte außerhalb, etwa in einer for-Schleife",
                        n, how
                    );
                    self.out.push(warn_at(a.span, n.chars().count() as u32, m));
                }
                _ => {}
            }
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let { init: Some(e), .. } | StmtKind::Return(Some(e)) | StmtKind::Throw(e) | StmtKind::Expr(e) => self.expr(e),
            StmtKind::Fn(f) => self.block(&f.body),
            StmtKind::If { arms, els } => {
                for (c, b) in arms {
                    self.expr(c);
                    self.block(b);
                }
                if let Some(b) = els {
                    self.block(b);
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            StmtKind::For { src, body, .. } => {
                match src {
                    ForSrc::Each(e) => self.expr(e),
                    ForSrc::Range(a, b) => {
                        self.expr(a);
                        self.expr(b);
                    }
                }
                self.block(body);
            }
            StmtKind::Try { body, catch, .. } => {
                self.block(body);
                self.block(catch);
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            StmtKind::Block(b) => self.block(b),
            _ => {}
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Interp(v) | ExprKind::List(v) => v.iter().for_each(|x| self.expr(x)),
            ExprKind::Map(kv) => kv.iter().for_each(|(k, v)| {
                self.expr(k);
                self.expr(v);
            }),
            ExprKind::Fn(f) => self.block(&f.body),
            ExprKind::Unary(_, a) | ExprKind::Field(a, _) => self.expr(a),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Method(obj, name, args) => {
                self.expr(obj);
                if CALLERS.contains(&name.text.as_str()) {
                    // xs.map(f), or list.map(xs, f): the module's function with the list first
                    let how = match &obj.kind {
                        ExprKind::Var(l) if l == "list" => format!("list.{}", name.text),
                        _ => name.text.clone(),
                    };
                    self.callbacks(&how, args);
                }
                args.iter().for_each(|x| self.expr(x));
            }
            ExprKind::Call(f, args) => {
                self.expr(f);
                args.iter().for_each(|x| self.expr(x));
            }
            _ => {}
        }
    }
}
