// check -- name resolution over the syntax tree, with the scope rules of ast.rs
//
//   resolve(&program, globals) -> Vec<Diagnostic>
//     errors:   a name declared twice in one block (also parameters, the two for
//               variables, top-level declarations), break / continue outside a loop (a
//               function body starts outside of any loop), return outside a function, two
//               variables over a range a..b
//     warnings: a name that is neither declared nor in globals (with a suggestion for a
//               typing error), a top-level let read by top-level code before it ran, (S1) a
//               division of two values that are certainly ints (intdiv.rs: it truncates)
//   globals: the predeclared names (crate::GLOBALS for check())
//   resolve_strict(&program, globals)   the same with unknown names as errors
//               (crate::compile_with: the host says which names it has); (S3) with the
//               removed names of a host's description: "box wurde entfernt – nimm geom.box"
//
// Suggestions (S3): suggest(name, candidates) is the one rule for "meinst du x?", here and for
// the members of host types and namespaces (lsapi.rs).  A candidate is suggested only when it
// is really close to what was typed: the distance counts inserted, deleted and changed
// characters and swapped neighbours as one each (prnt -> print, pirnt -> print, wran -> warn),
// a difference only in case not at all; it may be at most 1 for names of 3 to 5 characters, 2
// for 6 to 8, 3 for longer ones, and from 2 on the first character must be the same (normal is
// no typo of format).  Names of 1 or 2 characters get none, except a member that lacks a letter
// (vec.ad -> vec.add, geom.bx -> geom.box).  Among several the smallest distance wins, then the
// longest common start, then the first.  A member is compared without its namespace: geom.move
// is no typo of geom.cone (move -> cone is 2 changes in 4 characters).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::api::Removed;
use crate::ast::*;
use crate::Diagnostic;

struct Resolver<'a> {
    globals: &'a [&'a str],
    /// an unknown name is an error (resolve_strict)
    strict: bool,
    /// (S3) names that are gone, with what to take instead
    removed: &'a [Removed],
    /// top-level lets and fns: name, the byte from which top-level code may read it, line
    top: Vec<(&'a str, u32, u32)>,
    /// the block scopes inside the top level, innermost last
    scopes: Vec<Vec<&'a Name>>,
    fns: u32,
    loops: u32,
    out: Vec<Diagnostic>,
}

/// see the module comment
pub fn resolve(prog: &Program, globals: &[&str]) -> Vec<Diagnostic> {
    run(prog, globals, false, &[])
}

/// resolve() with unknown names as errors; removed: names that are gone (a host's description)
pub(crate) fn resolve_strict(prog: &Program, globals: &[&str], removed: &[Removed]) -> Vec<Diagnostic> {
    run(prog, globals, true, removed)
}

fn run(prog: &Program, globals: &[&str], strict: bool, removed: &[Removed]) -> Vec<Diagnostic> {
    let mut r = Resolver { globals, strict, removed, top: Vec::new(), scopes: Vec::new(), fns: 0, loops: 0, out: Vec::new() };
    for s in &prog.body {
        match &s.kind {
            StmtKind::Let { name, .. } => r.declare_top(name, s.span.end),
            StmtKind::Fn(FnDef { name: Some(name), .. }) => r.declare_top(name, 0),
            _ => {}
        }
    }
    for s in &prog.body {
        r.stmt(s);
    }
    // (S1) a division of two ints truncates: the editor warns (intdiv.rs)
    r.out.extend(crate::intdiv::warnings(prog));
    r.out
}

/// the edit distance of two names in characters: an inserted, deleted or changed character or
/// two swapped neighbours count one each (optimal string alignment)
pub(crate) fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let w = b.len() + 1;
    // three rows: i - 2, i - 1, i
    let mut d: Vec<usize> = alloc::vec![0; 3 * w];
    for (j, x) in d[..w].iter_mut().enumerate() {
        *x = j;
    }
    for i in 1..=a.len() {
        let (cur, prev, pp) = ((i % 3) * w, ((i - 1) % 3) * w, ((i + 1) % 3) * w);
        d[cur] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[prev + j] + 1).min(d[cur + j - 1] + 1).min(d[prev + j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[pp + j - 2] + 1);
            }
            d[cur + j] = v;
        }
    }
    d[(a.len() % 3) * w + b.len()]
}

/// the most a candidate may differ from a name of n characters to be suggested (see the module
/// comment)
fn bound(n: usize) -> usize {
    match n {
        0..=2 => 0,
        3..=5 => 1,
        6..=8 => 2,
        _ => 3,
    }
}

/// (S3) the candidate that is really close to a misspelt name (see the module comment), or none
pub(crate) fn suggest<'a>(name: &str, cands: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    pick(name, cands, false)
}

/// suggest() for a member of a namespace or type (name and candidates without the namespace):
/// also a name of 2 characters that lacks a letter
pub(crate) fn suggest_member<'a>(name: &str, cands: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    pick(name, cands, true)
}

fn pick<'a>(name: &str, cands: impl IntoIterator<Item = &'a str>, member: bool) -> Option<&'a str> {
    let low = name.to_lowercase();
    let n = low.chars().count();
    let first = low.chars().next()?;
    let max = bound(n);
    // (distance, -common start, the candidate)
    let mut best: Option<(usize, usize, &str)> = None;
    for c in cands {
        if c == name || c.is_empty() {
            continue;
        }
        let cl = c.to_lowercase();
        let d = if cl == low {
            0
        } else if n <= 2 {
            // a short member that lacks a letter: ad -> add, bx -> box
            if member && n == 2 && cl.chars().count() == 3 && distance(&low, &cl) == 1 { 1 } else { continue }
        } else {
            let d = distance(&low, &cl);
            if d > max || (d >= 2 && !cl.starts_with(first)) {
                continue;
            }
            d
        };
        let common = low.chars().zip(cl.chars()).take_while(|(x, y)| x == y).count();
        if best.is_none_or(|(bd, bc, _)| (d, usize::MAX - common) < (bd, usize::MAX - bc)) {
            best = Some((d, common, c));
        }
    }
    best.map(|b| b.2)
}

impl<'a> Resolver<'a> {
    fn report(&mut self, sp: Span, len: usize, error: bool, msg: String) {
        let end_col = sp.col + (len as u32).max(1);
        self.out.push(Diagnostic { line: sp.line, col: sp.col, end_col, error, msg, file: String::new() });
    }

    fn name_error(&mut self, n: &Name, msg: String) {
        self.report(n.span, n.text.chars().count(), true, msg);
    }

    fn declare_top(&mut self, n: &'a Name, from: u32) {
        if let Some(&(_, _, line)) = self.top.iter().find(|d| d.0 == n.text) {
            self.name_error(n, format!("{} ist schon deklariert (Zeile {})", n.text, line));
        } else {
            self.top.push((n.text.as_str(), from, n.span.line));
        }
    }

    /// a name in the innermost scope; param: the scope holds parameters
    fn declare(&mut self, n: &'a Name, param: bool) {
        let Some(scope) = self.scopes.last_mut() else { return };
        if let Some(prev) = scope.iter().find(|p| p.text == n.text) {
            let line = prev.span.line;
            let msg = if param {
                format!("Der Parameter {} kommt zweimal vor", n.text)
            } else {
                format!("{} ist in diesem Block schon deklariert (Zeile {})", n.text, line)
            };
            self.name_error(n, msg);
        } else {
            scope.push(n);
        }
    }

    /// the visible or global name a misspelt one is really close to (suggest)
    fn similar(&self, name: &str) -> Option<String> {
        let scoped = self.scopes.iter().flatten().map(|n| n.text.as_str());
        let top = self.top.iter().map(|d| d.0);
        suggest(name, scoped.chain(top).chain(self.globals.iter().copied())).map(String::from)
    }

    /// a name that is read (assign: written)
    fn use_name(&mut self, name: &str, sp: Span, assign: bool) {
        if self.scopes.iter().any(|s| s.iter().any(|n| n.text == name)) {
            return;
        }
        if let Some(&(_, from, line)) = self.top.iter().find(|d| d.0 == name) {
            if self.fns == 0 && sp.start < from && !assign {
                let msg = format!("{} wird benutzt, bevor es mit let angelegt wird (Zeile {})", name, line);
                self.report(sp, name.chars().count(), false, msg);
            }
            return;
        }
        if self.globals.contains(&name) {
            return;
        }
        if let Some(r) = self.removed.iter().find(|r| r.name == name) {
            let msg = r.message(name);
            self.report(sp, name.chars().count(), self.strict, msg);
            return;
        }
        let mut msg = format!("Unbekannter Name {}", name);
        if let Some(s) = self.similar(name) {
            msg.push_str(&format!(" – meinst du {}?", s));
        } else if assign {
            msg.push_str(&format!(" – eine neue Variable wird mit let {} = … angelegt", name));
        }
        self.report(sp, name.chars().count(), self.strict, msg);
    }

    fn block(&mut self, b: &'a Block) {
        self.scopes.push(Vec::new());
        for s in &b.stmts {
            if let StmtKind::Fn(FnDef { name: Some(n), .. }) = &s.kind {
                self.declare(n, false);
            }
        }
        for s in &b.stmts {
            self.stmt(s);
        }
        self.scopes.pop();
    }

    fn func(&mut self, f: &'a FnDef) {
        let loops = core::mem::replace(&mut self.loops, 0);
        self.fns += 1;
        self.scopes.push(Vec::new());
        for p in &f.params {
            self.declare(p, true);
        }
        self.block(&f.body);
        self.scopes.pop();
        self.fns -= 1;
        self.loops = loops;
    }

    fn stmt(&mut self, s: &'a Stmt) {
        match &s.kind {
            StmtKind::Let { name, init } => {
                if let Some(e) = init {
                    self.expr(e);
                }
                self.declare(name, false);
            }
            StmtKind::Fn(f) => self.func(f),
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
                self.loops += 1;
                self.block(body);
                self.loops -= 1;
            }
            StmtKind::For { a, b, src, body } => {
                match src {
                    ForSrc::Each(e) => self.expr(e),
                    ForSrc::Range(lo, hi) => {
                        self.expr(lo);
                        self.expr(hi);
                    }
                }
                self.scopes.push(Vec::new());
                self.declare(a, false);
                if let Some(b) = b {
                    self.declare(b, false);
                    if matches!(src, ForSrc::Range(..)) {
                        self.name_error(b, String::from("Ein Bereich a..b hat nur eine Laufvariable"));
                    }
                }
                self.loops += 1;
                self.block(body);
                self.loops -= 1;
                self.scopes.pop();
            }
            StmtKind::Break | StmtKind::Continue => {
                if self.loops == 0 {
                    let (w, len) = if s.kind == StmtKind::Break { ("break", 5) } else { ("continue", 8) };
                    self.report(s.span, len, true, format!("{} steht außerhalb einer Schleife", w));
                }
            }
            StmtKind::Return(v) => {
                if self.fns == 0 {
                    self.report(s.span, 6, true, String::from("return steht außerhalb einer Funktion"));
                }
                if let Some(e) = v {
                    self.expr(e);
                }
            }
            StmtKind::Try { body, var, catch } => {
                self.block(body);
                self.scopes.push(Vec::new());
                if let Some(v) = var {
                    self.declare(v, false);
                }
                self.block(catch);
                self.scopes.pop();
            }
            StmtKind::Throw(e) | StmtKind::Expr(e) => self.expr(e),
            StmtKind::Assign { target, value, .. } => {
                match &target.kind {
                    ExprKind::Var(n) => self.use_name(n, target.span, true),
                    _ => self.expr(target),
                }
                self.expr(value);
            }
            StmtKind::Block(b) => self.block(b),
        }
    }

    fn expr(&mut self, e: &'a Expr) {
        match &e.kind {
            ExprKind::Nil | ExprKind::Bool(_) | ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) => {}
            ExprKind::Var(n) => self.use_name(n, e.span, false),
            ExprKind::Interp(items) | ExprKind::List(items) => {
                for x in items {
                    self.expr(x);
                }
            }
            ExprKind::Map(kv) => {
                for (k, v) in kv {
                    self.expr(k);
                    self.expr(v);
                }
            }
            ExprKind::Fn(f) => self.func(f),
            ExprKind::Unary(_, a) | ExprKind::Field(a, _) => self.expr(a),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Call(f, args) | ExprKind::Method(f, _, args) => {
                self.expr(f);
                for x in args {
                    self.expr(x);
                }
            }
        }
    }
}
