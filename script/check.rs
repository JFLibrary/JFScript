// check -- name resolution over the syntax tree, with the scope rules of ast.rs
//
//   resolve(&program, globals) -> Vec<Diagnostic>
//     errors:   a name declared twice in one block (also parameters, the two for
//               variables, top-level declarations), break / continue outside a loop (a
//               function body starts outside of any loop), return outside a function, two
//               variables over a range a..b
//     warnings: a name that is neither declared nor in globals (with a suggestion for a
//               typing error), a top-level let read by top-level code before it ran, (S1) a
//               division of two values that are certainly ints (intdiv.rs: it truncates),
//               (S3) await in a function that list.map, list.sort .. run (awaitchk.rs)
//   globals: the predeclared names (crate::GLOBALS for check())
//   resolve_strict(&program, globals, api)   the same with unknown names as errors
//               (crate::compile_with: the host says which names it has); (S3) with the
//               removed names of a host's description: "box wurde entfernt – nimm geom.box"
//
// (S3) The syntax pass: the names of patterns (let [a, b], parameters, for {x, y}, the for
// clauses of a comprehension, a scope of its own) are declared like a let's; a default sees the
// parameters and names before it.  Named arguments: a name twice in one call is an error; one
// to a function of the host is an error without the host's description (it binds them: no
// form takes it), one to the library is checked against its parameters (lsdoc.rs), one to a fn
// declaration of the file that is never assigned gives a warning when it names no parameter
// (at run time that throws).
//
// Suggestions (S3): suggest(name, candidates) is the one rule for "meinst du x?", here and for
// the members of host types and namespaces (lsapi.rs).  A candidate is suggested only when it
// is really close to what was typed: the distance counts inserted, deleted and changed
// characters and swapped neighbours as one each (prnt -> print, pirnt -> print, wran -> warn),
// a difference only in case not at all; it may be at most 1 for names of 3 to 5 characters, 2
// for 6 to 8, 3 for longer ones, and from 2 on the first character must be the same (normal is
// no typo of format).  Names of 1 or 2 characters get one only for a difference in case or as a
// member that lacks a letter (vec.ad -> vec.add, geom.bx -> geom.box).  Among several the
// smallest distance wins, then the longest common start, then the first.  A member is compared
// without its namespace: geom.move is no typo of geom.cone (move -> cone is 2 changes in 4
// characters).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::api::{self, Api, Arg, Removed};
use crate::ast::*;
use crate::Diagnostic;

struct Resolver<'a> {
    globals: &'a [&'a str],
    /// an unknown name is an error (resolve_strict)
    strict: bool,
    /// (S3) names that are gone, with what to take instead
    removed: &'a [Removed],
    /// (S3) the host's description: named arguments to its functions are checked against it
    api: Option<&'a Api>,
    /// top-level lets and fns: name, the byte from which top-level code may read it, line
    top: Vec<(&'a str, u32, u32)>,
    /// the block scopes inside the top level, innermost last
    scopes: Vec<Vec<&'a Name>>,
    fns: u32,
    loops: u32,
    /// (S3) the fn declarations in blocks by their name's byte, the top-level ones by name
    /// (named arguments to them are checked)
    decls: BTreeMap<u32, &'a FnDef>,
    top_fns: Vec<(&'a str, &'a FnDef)>,
    /// (S3) names that an assignment changes somewhere in the file
    assigned: BTreeSet<&'a str>,
    out: Vec<Diagnostic>,
}

/// see the module comment
pub fn resolve(prog: &Program, globals: &[&str]) -> Vec<Diagnostic> {
    run(prog, globals, false, None)
}

/// resolve() with unknown names as errors; api: the host's description (its removed names, the
/// signatures named arguments are checked against)
pub(crate) fn resolve_strict(prog: &Program, globals: &[&str], api: Option<&Api>) -> Vec<Diagnostic> {
    run(prog, globals, true, api)
}

fn run(prog: &Program, globals: &[&str], strict: bool, api: Option<&Api>) -> Vec<Diagnostic> {
    let removed = api.map_or(&[][..], |a| &a.removed[..]);
    let mut r = Resolver {
        globals,
        strict,
        removed,
        api,
        top: Vec::new(),
        scopes: Vec::new(),
        fns: 0,
        loops: 0,
        decls: BTreeMap::new(),
        top_fns: Vec::new(),
        assigned: BTreeSet::new(),
        out: Vec::new(),
    };
    for s in &prog.body {
        match &s.kind {
            StmtKind::Let { pat, .. } => {
                let mut ns = Vec::new();
                pat.names(&mut ns);
                for n in ns {
                    r.declare_top(n, s.span.end);
                }
            }
            StmtKind::Fn(f @ FnDef { name: Some(name), .. }) => {
                r.declare_top(name, 0);
                if !r.top_fns.iter().any(|x| x.0 == name.text) {
                    r.top_fns.push((name.text.as_str(), f));
                }
            }
            _ => {}
        }
    }
    visit(&prog.body, &mut |n| match n {
        Node::Stmt(Stmt { kind: StmtKind::Assign { target: Expr { kind: ExprKind::Var(x), .. }, .. }, .. }) => {
            r.assigned.insert(x.as_str());
        }
        Node::Pattern(Pattern { kind: PatKind::Target(t), .. }) => {
            if let ExprKind::Var(x) = &t.kind {
                r.assigned.insert(x.as_str());
            }
        }
        _ => {}
    });
    for s in &prog.body {
        r.stmt(s);
    }
    // (S1) a division of two ints truncates: the editor warns (intdiv.rs)
    r.out.extend(crate::intdiv::warnings(prog));
    // (S3) await in a callback of list.map, list.sort ..: it cannot wait there (awaitchk.rs)
    r.out.extend(crate::awaitchk::warnings(prog));
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
            self.name_error(n, tf!("{} ist schon deklariert (Zeile {})", n.text, line));
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
                tf!("Der Parameter {} kommt zweimal vor", n.text)
            } else {
                tf!("{} ist in diesem Block schon deklariert (Zeile {})", n.text, line)
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
                let msg = tf!("{} wird benutzt, bevor es mit let angelegt wird (Zeile {})", name, line);
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
        let mut msg = tf!("Unbekannter Name {}", name);
        if let Some(s) = self.similar(name) {
            msg = tf!("{} – meinst du {}?", msg, s);
        } else if assign {
            msg = tf!("{} – eine neue Variable wird mit let {} = … angelegt", msg, name);
        }
        self.report(sp, name.chars().count(), self.strict, msg);
    }

    fn block(&mut self, b: &'a Block) {
        self.scopes.push(Vec::new());
        for s in &b.stmts {
            if let StmtKind::Fn(f @ FnDef { name: Some(n), .. }) = &s.kind {
                self.declare(n, false);
                self.decls.insert(n.span.start, f);
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
        // (S3) a parameter's default sees the parameters before it
        for p in &f.params {
            self.bind(p, true);
        }
        self.block(&f.body);
        self.scopes.pop();
        self.fns -= 1;
        self.loops = loops;
    }

    /// (S3) the names of a pattern into the innermost scope, the defaults in it read before the
    /// names after them; param: the scope holds parameters.  A Target leaf (a destructuring
    /// assignment) is written, not declared.
    fn bind(&mut self, p: &'a Pattern, param: bool) {
        match &p.kind {
            PatKind::Name(n) => {
                // a top-level let's name has its value from here on (a default after it in the
                // pattern reads it)
                if self.scopes.is_empty()
                    && let Some(d) = self.top.iter_mut().find(|d| d.0 == n.text)
                {
                    d.1 = d.1.min(n.span.end);
                }
                self.declare(n, param)
            }
            PatKind::List(items, rest) => {
                for x in items {
                    self.bind(x, param);
                }
                if let Some(r) = rest {
                    self.bind(r, param);
                }
            }
            PatKind::Map(entries) => {
                for (_, x) in entries {
                    self.bind(x, param);
                }
            }
            PatKind::Default(x, e) => {
                self.expr(e);
                self.bind(x, param);
            }
            PatKind::Target(t) => match &t.kind {
                ExprKind::Var(n) => self.use_name(n, t.span, true),
                _ => self.expr(t),
            },
        }
    }

    /// the variables of a for loop or clause: a scope is open; a range has one, a name
    fn for_vars(&mut self, a: &'a Pattern, b: Option<&'a Pattern>, src: &'a ForSrc) {
        self.bind(a, false);
        if let Some(b) = b {
            self.bind(b, false);
        }
        if matches!(src, ForSrc::Range(..)) {
            if let Some(b) = b {
                let at = b.name().map_or(Name { text: String::from(","), span: b.span }, |n| n.clone());
                self.name_error(&at, String::from(t!("Ein Bereich a..b hat nur eine Laufvariable")));
            } else if a.name().is_none() {
                self.report(a.span, 1, true, String::from(t!("Ein Bereich a..b gibt ganze Zahlen: die lassen sich nicht zerlegen")));
            }
        }
    }

    fn src(&mut self, src: &'a ForSrc) {
        match src {
            ForSrc::Each(e) => self.expr(e),
            ForSrc::Range(lo, hi) => {
                self.expr(lo);
                self.expr(hi);
            }
        }
    }

    fn stmt(&mut self, s: &'a Stmt) {
        match &s.kind {
            StmtKind::Let { pat, init } => {
                if let Some(e) = init {
                    self.expr(e);
                }
                self.bind(pat, false);
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
                self.src(src);
                self.scopes.push(Vec::new());
                self.for_vars(a, b.as_ref(), src);
                self.loops += 1;
                self.block(body);
                self.loops -= 1;
                self.scopes.pop();
            }
            StmtKind::Break | StmtKind::Continue => {
                if self.loops == 0 {
                    let (w, len) = if s.kind == StmtKind::Break { ("break", 5) } else { ("continue", 8) };
                    self.report(s.span, len, true, tf!("{} steht außerhalb einer Schleife", w));
                }
            }
            StmtKind::Return(v) => {
                if self.fns == 0 {
                    self.report(s.span, 6, true, String::from(t!("return steht außerhalb einer Funktion")));
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
            StmtKind::AssignPat { pat, value } => {
                self.expr(value);
                self.bind(pat, false);
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
            ExprKind::Unary(_, a) | ExprKind::Field(a, _) | ExprKind::Named(_, a) => self.expr(a),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Call(f, args) | ExprKind::Method(f, _, args) => {
                self.expr(f);
                for x in args {
                    self.expr(x);
                }
                if args.iter().any(|a| matches!(a.kind, ExprKind::Named(..))) {
                    self.named(e, args);
                }
            }
            ExprKind::Cond(c, a, b) => {
                self.expr(c);
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Comp(x, clauses) => {
                // (S3) a scope per for clause, the expression in front last
                let depth = self.scopes.len();
                for c in clauses {
                    match c {
                        Clause::For { a, b, src } => {
                            self.src(src);
                            self.scopes.push(Vec::new());
                            self.for_vars(a, b.as_ref(), src);
                        }
                        Clause::If(c) => self.expr(c),
                    }
                }
                self.expr(x);
                self.scopes.truncate(depth);
            }
        }
    }

    /// whether a name means a declaration of the script (not the host's)
    fn declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.iter().any(|n| n.text == name)) || self.top.iter().any(|d| d.0 == name)
    }

    /// the fn declaration a name means, when it is one (innermost scope first)
    fn decl_of(&self, name: &str) -> Option<&'a FnDef> {
        for s in self.scopes.iter().rev() {
            if let Some(n) = s.iter().rev().find(|n| n.text == name) {
                return self.decls.get(&n.span.start).copied();
            }
        }
        self.top_fns.iter().find(|x| x.0 == name).map(|x| x.1)
    }

    /// (S3) a call with named arguments (see the module comment)
    fn named(&mut self, call: &'a Expr, args: &'a [Expr]) {
        // a name twice
        for (i, a) in args.iter().enumerate() {
            if let ExprKind::Named(n, _) = &a.kind
                && args[..i].iter().any(|b| matches!(&b.kind, ExprKind::Named(m, _) if m.text == n.text))
            {
                self.name_error(n, tf!("Das benannte Argument {} kommt zweimal vor", n.text));
                return;
            }
        }
        // the function: a name of the host or of the library (f, ns.f), or a fn of the file
        // (the message's place: the name of the function or method as written)
        let (full, shown) = match &call.kind {
            ExprKind::Call(f, _) => match &f.kind {
                ExprKind::Var(n) if !self.declared(n) => (n.clone(), Name { text: n.clone(), span: f.span }),
                ExprKind::Var(n) => {
                    if let Some(d) = self.decl_of(n)
                        && !self.assigned.contains(n.as_str())
                    {
                        self.script_fn(n, d, args);
                    }
                    return;
                }
                _ => return,
            },
            ExprKind::Method(obj, name, _) => match &obj.kind {
                ExprKind::Var(ns) if !self.declared(ns) => (format!("{}.{}", ns, name.text), name.clone()),
                _ => return,
            },
            _ => return,
        };
        let forms: Vec<api::Sig> = match (crate::lsdoc::sig(&full), self.api) {
            (Some(s), _) => alloc::vec![s],
            (None, Some(a)) => a.sigs(&full).into_iter().cloned().collect(),
            (None, None) => {
                let msg = tf!("{}: benannte Argumente gehen an Funktionen des Hosts nur mit seiner Beschreibung (script::Api, compile_api)", full);
                self.name_error(&shown, msg);
                return;
            }
        };
        if forms.is_empty() {
            if self.api.is_some_and(|a| a.global(&full).is_some()) {
                let msg = tf!("{} ist keine Funktion des Hosts, die benannte Argumente nimmt", full);
                self.name_error(&shown, msg);
            }
            return;
        }
        let a: Vec<Arg> = args.iter().map(arg_of).collect();
        let refs: Vec<&api::Sig> = forms.iter().collect();
        if let Err(Some(m)) = api::bind_lenient(&refs, &a) {
            let msg = api::named_message(&full, refs[0], &a, m);
            let at = mismatch_at(args, m).unwrap_or(shown);
            self.name_error(&at, msg);
        }
    }

    /// named arguments to a fn of the file: a warning for a name that is no parameter, or one a
    /// positional argument fills already (at run time they throw)
    fn script_fn(&mut self, name: &str, d: &'a FnDef, args: &'a [Expr]) {
        let pos = args.iter().filter(|a| !matches!(a.kind, ExprKind::Named(..))).count();
        let params: Vec<Option<&str>> = d.params.iter().map(|p| p.name().map(|n| n.text.as_str())).collect();
        for a in args {
            let ExprKind::Named(n, _) = &a.kind else { continue };
            let msg = match params.iter().position(|p| *p == Some(n.text.as_str())) {
                None => {
                    let mut m = tf!("{} hat keinen Parameter {}", name, n.text);
                    if let Some(c) = suggest_member(&n.text, params.iter().flatten().copied()) {
                        m = tf!("{} – meinst du {}?", m, c);
                    }
                    m
                }
                Some(i) if i < pos => tf!("{}: der Parameter {} hat schon ein Argument", name, n.text),
                Some(_) => continue,
            };
            self.report(n.span, n.text.chars().count(), false, msg);
        }
    }
}

/// (S3) a call's argument as api::bind sees it (a named one by its name; types not known)
pub(crate) fn arg_of(e: &Expr) -> Arg<'_> {
    match &e.kind {
        ExprKind::Named(n, _) => Arg::named(&n.text, ""),
        _ => Arg::pos(""),
    }
}

/// the name of the argument a mismatch is about (a named one), for the message's place
fn mismatch_at(args: &[Expr], m: api::Mismatch) -> Option<Name> {
    let i = match m {
        api::Mismatch::NoParam { arg } | api::Mismatch::Twice { arg, .. } => arg,
        _ => return None,
    };
    match &args.get(i)?.kind {
        ExprKind::Named(n, _) => Some(n.clone()),
        _ => None,
    }
}
