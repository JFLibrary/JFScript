// lsapi -- (S2) the host's description (api.rs) in the language services: which type an
// expression has, the members after a dot, hover and signatures of host members, and the
// diagnostics for unknown members of a known host type.  Simple and honest (G3): a type is
// known only where the description says it, never guessed; where it is not known, the services
// say what they said before S2.
//
//   infer(&prog, &idx, api) -> Types   the type of each symbol of the Index: a let's initializer
//                    (when every assignment to it in the file has the same type), a for
//                    variable over a list<T> (T), the parameter of a function given to
//                    obj.on("event", fn(e) { .. }) (the event's type); parameters, catch
//                    variables and the rest are not known
//   Types::expr(e)   the type of an expression: literals, names (the symbol's, a host value's),
//                    obj.prop, obj.m(..), f(..) of a host function, l[i] of a list<T>, operators
//                    of host types (the description's), int and float arithmetic
//   Types::chain(src, toks, idx, r)   the same for the code before a dot as tokens (it may not
//                    parse while it is typed): name(.member | .call(..) | [..])* back from token r
//   diagnostics(..)  warnings: a property, method or event a known host type does not have (with
//                    the closest name: check::suggest_member), a method called as a property or
//                    the other way round, a read-only property assigned, a member of a host
//                    namespace that is not there; (S3) a removed member or name with what to take
//                    instead; with Api::check_calls a call of a host function or method whose
//                    number of arguments fits none of its forms
//
// (S3) Forms (overloads): the result of a call is the one all forms its arguments may bind to
// share (Sig::bind with the types known here, also of the arguments of a call in a chain: an
// argument whose type is not known fits every form), when none fits the one all forms share;
// else not known.  A union type has members only as "T | nil" (T's).

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::api::{self, Api, Arg, GlobalKind, Sig};
use crate::ast::*;
use crate::lex::{Tok, TokKind};
use crate::ls::Index;
use crate::vm::Operator;
use crate::Diagnostic;

/// what the code before a dot is: a value of a type, or a namespace of the host
pub(crate) enum Known {
    Ty(String),
    Ns(String),
}

/// the types of a script's symbols (see the module comment)
pub(crate) struct Types<'a> {
    pub api: &'a Api,
    /// by the symbol's index in the Index: its type ("" not known)
    pub sym: Vec<String>,
    /// the assignments to the symbol disagree: not known, for good
    mixed: Vec<bool>,
    idx: &'a Index,
}

/// see the module comment
pub(crate) fn infer<'a>(prog: &Program, idx: &'a Index, api: &'a Api) -> Types<'a> {
    let n = idx.symbols.len();
    let mut t = Types { api, sym: vec![String::new(); n], mixed: vec![false; n], idx };
    // a function may use a top-level let declared after it: a few passes until nothing changes
    for _ in 0..4 {
        let before = t.sym.clone();
        for s in &prog.body {
            t.stmt(s);
        }
        if t.sym == before {
            break;
        }
    }
    t
}

impl Types<'_> {
    /// the symbol declared by the name at byte start
    fn decl(&self, start: u32) -> Option<usize> {
        self.idx.symbols.iter().position(|s| s.span.start == start)
    }

    fn set(&mut self, sym: Option<usize>, ty: String) {
        if let Some(i) = sym
            && !self.mixed[i]
        {
            self.sym[i] = ty;
        }
    }

    /// an assignment of a value of type ty to the symbol: a different one makes it unknown
    fn assigned(&mut self, sym: Option<usize>, ty: &str) {
        if let Some(i) = sym
            && self.sym[i] != ty
        {
            self.mixed[i] = true;
            self.sym[i] = String::new();
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn func(&mut self, f: &FnDef) {
        self.block(&f.body);
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let { name, init } => {
                let ty = match init {
                    Some(e) => {
                        self.expr_walk(e);
                        self.expr(e)
                    }
                    None => String::new(),
                };
                let sym = self.decl(name.span.start);
                self.set(sym, ty);
            }
            StmtKind::Fn(f) => self.func(f),
            StmtKind::If { arms, els } => {
                for (c, b) in arms {
                    self.expr_walk(c);
                    self.block(b);
                }
                if let Some(b) = els {
                    self.block(b);
                }
            }
            StmtKind::While { cond, body } => {
                self.expr_walk(cond);
                self.block(body);
            }
            StmtKind::For { a, b, src, body } => {
                match src {
                    ForSrc::Each(e) => {
                        self.expr_walk(e);
                        let item = String::from(api::list_of(api::member_type(&self.expr(e))).unwrap_or(""));
                        let (sa, sb) = (self.decl(a.span.start), b.as_ref().and_then(|b| self.decl(b.span.start)));
                        match b {
                            None => self.set(sa, item),
                            Some(_) => {
                                // (an index of a list; a map's key is not known)
                                self.set(sb, item.clone());
                                self.set(sa, if item.is_empty() { String::new() } else { String::from("int") });
                            }
                        }
                    }
                    ForSrc::Range(lo, hi) => {
                        self.expr_walk(lo);
                        self.expr_walk(hi);
                        let sa = self.decl(a.span.start);
                        self.set(sa, String::from("int"));
                    }
                }
                self.block(body);
            }
            StmtKind::Return(Some(e)) | StmtKind::Throw(e) | StmtKind::Expr(e) => self.expr_walk(e),
            StmtKind::Try { body, catch, .. } => {
                self.block(body);
                self.block(catch);
            }
            StmtKind::Assign { op, target, value } => {
                self.expr_walk(target);
                self.expr_walk(value);
                if let ExprKind::Var(_) = &target.kind {
                    let sym = self.idx.use_from(target.span.start).and_then(|u| u.sym);
                    let v = self.expr(value);
                    let ty = match op {
                        None => v,
                        Some(o) => self.binary(*o, &self.expr(target), &v),
                    };
                    self.assigned(sym, &ty);
                }
            }
            StmtKind::Block(b) => self.block(b),
            _ => {}
        }
    }

    /// the statements inside an expression (fn literals) and the parameters of handlers
    fn expr_walk(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Interp(v) | ExprKind::List(v) => v.iter().for_each(|x| self.expr_walk(x)),
            ExprKind::Map(kv) => kv.iter().for_each(|(k, v)| {
                self.expr_walk(k);
                self.expr_walk(v);
            }),
            ExprKind::Fn(f) => self.func(f),
            ExprKind::Unary(_, a) | ExprKind::Field(a, _) => self.expr_walk(a),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr_walk(a);
                self.expr_walk(b);
            }
            ExprKind::Call(f, args) => {
                self.expr_walk(f);
                args.iter().for_each(|x| self.expr_walk(x));
            }
            ExprKind::Method(obj, name, args) => {
                self.expr_walk(obj);
                // obj.on("event", fn(e) { .. }): e is the event's object
                if (name.text == "on" || name.text == "off")
                    && let (Some(Expr { kind: ExprKind::Str(ev), .. }), Some(Expr { kind: ExprKind::Fn(f), .. })) = (args.first(), args.get(1))
                {
                    let ty = self.expr(obj);
                    let et = self.api.get_type(api::member_type(&ty)).and_then(|t| t.find_event(ev)).map(|x| x.ty.clone());
                    if let (Some(et), Some(p)) = (et, f.params.first()) {
                        let sym = self.decl(p.span.start);
                        self.set(sym, et);
                    }
                }
                args.iter().for_each(|x| self.expr_walk(x));
            }
            _ => {}
        }
    }

    /// the host namespace an expression names (log, a.b), if it is one
    fn namespace(&self, e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Var(n) => {
                let script = self.idx.use_from(e.span.start).and_then(|u| u.sym).is_some();
                (!script && self.api.is_namespace(n)).then(|| n.clone())
            }
            ExprKind::Field(o, name) => {
                let full = format!("{}.{}", self.namespace(o)?, name.text);
                self.api.is_namespace(&full).then_some(full)
            }
            _ => None,
        }
    }

    /// the type of a global name of the host ("" a function, a namespace or nothing known)
    fn global(&self, name: &str) -> String {
        match self.api.global(name).map(|g| &g.kind) {
            Some(GlobalKind::Value(t)) => t.clone(),
            Some(GlobalKind::Func(_)) => String::from("fn"),
            _ => String::new(),
        }
    }

    /// the result of a host function (namespaced or not); args: the types of the call's
    /// arguments when they are there
    fn func_ret(&self, name: &str, args: Option<&[String]>) -> String {
        self.ret_of(&self.api.sigs(name), args)
    }

    /// (S3) the result of a call of one of these forms: the one all forms the arguments may bind
    /// to share (their types as far as known: an argument of a type not known fits every form,
    /// so that is no guess), else the one all forms share, else not known
    fn ret_of(&self, forms: &[&Sig], args: Option<&[String]>) -> String {
        if forms.len() == 1 {
            return forms[0].ret.clone();
        }
        let same = |fs: &[&Sig]| match fs.first() {
            Some(f) if fs.iter().all(|x| x.ret == f.ret) => f.ret.clone(),
            _ => String::new(),
        };
        if let Some(tys) = args {
            let a: Vec<Arg> = tys.iter().map(|t| Arg::pos(t)).collect();
            let fit: Vec<&Sig> = forms.iter().copied().filter(|s| s.bind(&a).is_ok()).collect();
            if !fit.is_empty() {
                return same(&fit);
            }
        }
        same(forms)
    }

    /// the type of a member (prop or method's result) of type ty
    fn member(&self, ty: &str, name: &str, call: Option<Option<&[String]>>) -> String {
        let Some(t) = self.api.get_type(api::member_type(ty)) else { return String::new() };
        match call {
            Some(args) => self.ret_of(&t.sigs(name), args),
            None => t.find_prop(name).map_or(String::new(), |p| p.ty.clone()),
        }
    }

    /// a op b of these types: numbers as the VM computes them, host types by their operators
    fn binary(&self, op: BinOp, a: &str, b: &str) -> String {
        let num = |t: &str| t == "int" || t == "float";
        let o = match op {
            BinOp::Add => Operator::Add,
            BinOp::Sub => Operator::Sub,
            BinOp::Mul => Operator::Mul,
            BinOp::Div => Operator::Div,
            BinOp::Rem => Operator::Rem,
            BinOp::Lt => Operator::Lt,
            BinOp::Le => Operator::Le,
            BinOp::Gt => Operator::Gt,
            BinOp::Ge => Operator::Ge,
            BinOp::Eq | BinOp::Ne => return String::from("bool"),
            BinOp::Coalesce => return if a == b { String::from(a) } else { String::new() },
            BinOp::And | BinOp::Or => return String::new(),
        };
        if num(a) && num(b) {
            return String::from(match o {
                Operator::Lt | Operator::Le | Operator::Gt | Operator::Ge => "bool",
                _ if a == "int" && b == "int" => "int",
                _ => "float",
            });
        }
        if a == "str" && b == "str" && o == Operator::Add {
            return String::from("str");
        }
        String::from(self.api.op_result(o, a, b).unwrap_or(""))
    }

    /// see the module comment
    pub(crate) fn expr(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Nil => String::from("nil"),
            ExprKind::Bool(_) => String::from("bool"),
            ExprKind::Int(_) => String::from("int"),
            ExprKind::Float(_) => String::from("float"),
            ExprKind::Str(_) | ExprKind::Interp(_) => String::from("str"),
            ExprKind::List(_) => String::from("list"),
            ExprKind::Map(_) => String::from("map"),
            ExprKind::Fn(_) => String::from("fn"),
            ExprKind::Var(n) => match self.idx.use_from(e.span.start).and_then(|u| u.sym) {
                Some(i) => self.sym[i].clone(),
                None if self.idx.use_from(e.span.start).is_some() => self.global(n),
                None => self.idx.resolve(n, e.span.start).map_or_else(|| self.global(n), |i| self.sym[i].clone()),
            },
            ExprKind::Field(obj, name) => match self.namespace(obj) {
                Some(ns) => self.global(&format!("{}.{}", ns, name.text)),
                None => self.member(&self.expr(obj), &name.text, None),
            },
            ExprKind::Method(obj, name, args) => {
                let tys: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                match self.namespace(obj) {
                    Some(ns) => self.func_ret(&format!("{}.{}", ns, name.text), Some(&tys)),
                    None => self.member(&self.expr(obj), &name.text, Some(Some(&tys))),
                }
            }
            ExprKind::Call(f, args) => match &f.kind {
                ExprKind::Var(n) if self.idx.use_from(f.span.start).and_then(|u| u.sym).is_none() => {
                    let tys: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                    self.func_ret(n, Some(&tys))
                }
                _ => String::new(),
            },
            ExprKind::Index(obj, _) => String::from(api::list_of(api::member_type(&self.expr(obj))).unwrap_or("")),
            ExprKind::Binary(op, a, b) => self.binary(*op, &self.expr(a), &self.expr(b)),
            ExprKind::Unary(UnOp::Neg, a) => {
                let t = self.expr(a);
                if t == "int" || t == "float" { t } else { String::from(self.api.op_result(Operator::Neg, "", &t).unwrap_or("")) }
            }
            ExprKind::Unary(_, _) => String::from("bool"),
        }
    }

    // ---------------------------------------------------------------- the code before a dot

    /// the type of the code that ends at token r of the full token list (see the module comment)
    pub(crate) fn chain(&self, src: &str, toks: &[Tok], r: usize) -> Option<Known> {
        self.chain_in(src, toks, r, 0)
    }

    /// chain(); depth: how many calls' arguments it is inside (their types are looked at to a
    /// few levels only)
    fn chain_in(&self, src: &str, toks: &[Tok], r: usize, depth: u32) -> Option<Known> {
        enum Step {
            Member(String),
            Call(String, Vec<String>),
            Index,
        }
        let text = |t: &Tok| &src[t.start as usize..t.end as usize];
        let prev = |k: usize| (0..k).rev().find(|&j| !matches!(toks[j].kind, TokKind::Space | TokKind::Newline | TokKind::Comment | TokKind::Eof));
        // the matching opener of the closer at k
        let open = |k: usize, close: TokKind, opener: TokKind| -> Option<usize> {
            let mut depth = 0i32;
            for j in (0..=k).rev() {
                if toks[j].kind == close {
                    depth += 1;
                } else if toks[j].kind == opener {
                    depth -= 1;
                    if depth == 0 {
                        return Some(j);
                    }
                }
            }
            None
        };
        let mut steps: Vec<Step> = Vec::new();
        let mut k = r;
        // the start: Ok((name, start, the arguments' types when called)) or a str literal
        let start: Result<(String, u32, Option<Vec<String>>), ()> = loop {
            let t = toks[k];
            match t.kind {
                TokKind::Ident => {
                    let name = String::from(text(&t));
                    match prev(k) {
                        Some(d) if toks[d].kind == TokKind::Dot => {
                            steps.push(Step::Member(name));
                            k = prev(d)?;
                        }
                        _ => break Ok((name, t.start, None)),
                    }
                }
                TokKind::RParen => {
                    let o = open(k, TokKind::RParen, TokKind::LParen)?;
                    let f = prev(o).filter(|&j| toks[j].kind == TokKind::Ident)?;
                    let name = String::from(text(&toks[f]));
                    let args = self.args_in(src, toks, o, k, true, depth + 1);
                    match prev(f) {
                        Some(d) if toks[d].kind == TokKind::Dot => {
                            steps.push(Step::Call(name, args));
                            k = prev(d)?;
                        }
                        _ => break Ok((name, toks[f].start, Some(args))),
                    }
                }
                TokKind::RBracket => {
                    let o = open(k, TokKind::RBracket, TokKind::LBracket)?;
                    steps.push(Step::Index);
                    k = prev(o)?;
                }
                TokKind::Str | TokKind::StrTail => break Err(()),
                _ => return None,
            }
        };
        let mut cur = match start {
            Err(()) => Known::Ty(String::from("str")),
            Ok((name, at, called)) => {
                let sym = self.idx.sym_of(&name, at);
                match (sym, called) {
                    (Some(i), None) => Known::Ty(self.sym[i].clone()),
                    (Some(_), Some(_)) => return None,
                    (None, Some(args)) => Known::Ty(self.func_ret(&name, Some(&args))),
                    (None, None) if self.api.is_namespace(&name) => Known::Ns(name),
                    (None, None) => Known::Ty(self.global(&name)),
                }
            }
        };
        for s in steps.into_iter().rev() {
            cur = match (cur, s) {
                (Known::Ns(ns), Step::Member(m)) => {
                    let full = format!("{}.{}", ns, m);
                    if self.api.is_namespace(&full) { Known::Ns(full) } else { Known::Ty(self.global(&full)) }
                }
                (Known::Ns(ns), Step::Call(m, args)) => Known::Ty(self.func_ret(&format!("{}.{}", ns, m), Some(&args))),
                (Known::Ty(t), Step::Member(m)) => Known::Ty(self.member(&t, &m, None)),
                (Known::Ty(t), Step::Call(m, args)) => Known::Ty(self.member(&t, &m, Some(Some(&args)))),
                (Known::Ty(t), Step::Index) => Known::Ty(String::from(api::list_of(api::member_type(&t)).unwrap_or(""))),
                (Known::Ns(_), Step::Index) => return None,
            };
            if matches!(&cur, Known::Ty(t) if t.is_empty()) {
                return None;
            }
        }
        match cur {
            Known::Ty(t) if t.is_empty() => None,
            k => Some(k),
        }
    }

    /// (S3) the types of the arguments after the ( at token open of the full token list up to
    /// token end: those followed by a comma and, with last (a call that is closed at end), the one
    /// after the last comma; each a literal's ("int", "str", "list" ..), a name's or a chain's
    /// (`m`, `doc.page`, `cube(1)`), else "" (not known)
    pub(crate) fn arg_types(&self, src: &str, toks: &[Tok], open: usize, end: usize, last: bool) -> Vec<String> {
        self.args_in(src, toks, open, end, last, 0)
    }

    fn args_in(&self, src: &str, toks: &[Tok], open: usize, end: usize, last: bool, depth: u32) -> Vec<String> {
        let mut out = Vec::new();
        let mut seg: Vec<usize> = Vec::new();
        let (mut level, mut commas) = (0i32, 0usize);
        for j in open + 1..end.min(toks.len()) {
            match toks[j].kind {
                TokKind::Space | TokKind::Newline | TokKind::Comment | TokKind::Eof => continue,
                TokKind::LParen | TokKind::LBracket | TokKind::LBrace | TokKind::StrHead => level += 1,
                TokKind::RParen | TokKind::RBracket | TokKind::RBrace | TokKind::StrTail => level -= 1,
                TokKind::Comma if level == 0 => {
                    out.push(self.arg_type(src, toks, &seg, depth));
                    seg.clear();
                    commas += 1;
                    continue;
                }
                _ => {}
            }
            seg.push(j);
        }
        if last && (!seg.is_empty() || commas > 0) {
            out.push(self.arg_type(src, toks, &seg, depth));
        }
        out
    }

    /// the type of one argument, its tokens' indexes in seg (see arg_types)
    fn arg_type(&self, src: &str, toks: &[Tok], seg: &[usize], depth: u32) -> String {
        let kind = |i: usize| toks[seg[i]].kind;
        let lit = |k: TokKind| -> &'static str {
            match k {
                TokKind::Int => "int",
                TokKind::Float => "float",
                TokKind::Str => "str",
                TokKind::True | TokKind::False => "bool",
                TokKind::Nil => "nil",
                _ => "",
            }
        };
        // the bracket that opens at the first token closes at the last
        let whole = |open: TokKind, close: TokKind| -> bool {
            let mut level = 0i32;
            for i in 0..seg.len() {
                if kind(i) == open {
                    level += 1;
                } else if kind(i) == close {
                    level -= 1;
                    if level == 0 {
                        return i + 1 == seg.len();
                    }
                }
            }
            false
        };
        let n = seg.len();
        if n == 0 || depth > 8 {
            return String::new();
        }
        let first = kind(0);
        if n == 1 && !lit(first).is_empty() {
            return String::from(lit(first));
        }
        if n == 2 && first == TokKind::Minus && matches!(kind(1), TokKind::Int | TokKind::Float) {
            return String::from(lit(kind(1)));
        }
        for (open, close, ty) in [(TokKind::LBracket, TokKind::RBracket, "list"), (TokKind::LBrace, TokKind::RBrace, "map"), (TokKind::StrHead, TokKind::StrTail, "str")] {
            if first == open && whole(open, close) {
                return String::from(ty);
            }
        }
        if first != TokKind::Ident {
            return String::new();
        }
        // name(.member | .call(..) | [..])*: nothing else outside the brackets
        let mut level = 0i32;
        for i in 0..n {
            match kind(i) {
                TokKind::LParen | TokKind::LBracket => level += 1,
                TokKind::RParen | TokKind::RBracket => level -= 1,
                TokKind::Ident | TokKind::Dot => {}
                _ if level > 0 => {}
                _ => return String::new(),
            }
        }
        match self.chain_in(src, toks, seg[n - 1], depth) {
            Some(Known::Ty(t)) => t,
            _ => String::new(),
        }
    }
}

// ---------------------------------------------------------------- diagnostics

/// the member a misspelt one is really close to (the rule of check.rs)
fn closest<'a>(name: &str, names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    crate::check::suggest_member(name, names)
}

/// (S3) the names of a parsed script against a host's description and `more` names (the roots
/// of HostItems, the names of earlier scripts): unknown ones are errors, removed ones say what
/// to take instead (check::resolve_strict)
pub(crate) fn check_names<'a>(prog: &Program, api: &'a Api, more: &[&'a str]) -> Vec<Diagnostic> {
    let mut names = api.names();
    for m in more {
        if !names.contains(m) {
            names.push(m);
        }
    }
    crate::check::resolve_strict(prog, &crate::known_names(&names), &api.removed)
}

/// (S3) the description's warnings on a parsed script: diagnostics() over infer()
pub(crate) fn check_members(prog: &Program, src: &str, api: &Api) -> Vec<Diagnostic> {
    let idx = crate::ls::index_of(prog, src);
    let types = infer(prog, &idx, api);
    diagnostics(prog, &types, src)
}

/// see the module comment
pub(crate) fn diagnostics(prog: &Program, t: &Types, src: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut d = Diags { t, src, out: &mut out };
    for s in &prog.body {
        d.stmt(s);
    }
    out
}

struct Diags<'a, 'b> {
    t: &'a Types<'a>,
    src: &'a str,
    out: &'b mut Vec<Diagnostic>,
}

impl Diags<'_, '_> {
    fn warn(&mut self, n: &Name, msg: String) {
        self.out.push(crate::diag(self.src, n.span, false, msg));
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
                match &target.kind {
                    ExprKind::Field(obj, name) => {
                        self.expr(obj);
                        self.member(obj, name, Use::Set);
                    }
                    _ => self.expr(target),
                }
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
            ExprKind::Unary(_, a) => self.expr(a),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Field(obj, name) => {
                self.expr(obj);
                self.member(obj, name, Use::Get);
            }
            ExprKind::Method(obj, name, args) => {
                self.expr(obj);
                self.member(obj, name, Use::Call(args));
                args.iter().for_each(|x| self.expr(x));
            }
            ExprKind::Call(f, args) => {
                self.expr(f);
                self.call(f, args);
                args.iter().for_each(|x| self.expr(x));
            }
            _ => {}
        }
    }

    /// (S3) f(args) of a host function: the number of arguments against its forms (check_calls)
    fn call(&mut self, f: &Expr, args: &[Expr]) {
        let api = self.t.api;
        let ExprKind::Var(n) = &f.kind else { return };
        if !api.check_calls || self.t.idx.use_from(f.span.start).and_then(|u| u.sym).is_some() {
            return;
        }
        if let Some(m) = api::count_warning(n, &api.sigs(n), args.len()) {
            let at = Name { text: n.clone(), span: f.span };
            self.warn(&at, m);
        }
    }

    /// obj.name read, set or called
    fn member(&mut self, obj: &Expr, name: &Name, how: Use) {
        let api = self.t.api;
        if let Some(ns) = self.t.namespace(obj) {
            let full = format!("{}.{}", ns, name.text);
            if api.global(&full).is_none() && !api.is_namespace(&full) {
                if let Some(r) = api.find_removed(&full) {
                    self.warn(name, r.message(&full));
                    return;
                }
                // (the members of the namespace, compared without it)
                let pre = format!("{}.", ns);
                let mut shorts: Vec<&str> = Vec::new();
                for g in api.globals.iter().filter_map(|g| g.name.strip_prefix(pre.as_str())) {
                    let first = g.split('.').next().unwrap_or(g);
                    if !shorts.contains(&first) {
                        shorts.push(first);
                    }
                }
                let mut m = format!("Unbekannter Name {}", full);
                if let Some(c) = closest(&name.text, shorts.into_iter()) {
                    m.push_str(&format!(" – meinst du {}{}?", pre, c));
                }
                self.warn(name, m);
            } else if let Use::Call(args) = how
                && api.check_calls
                && let Some(m) = api::count_warning(&full, &api.sigs(&full), args.len())
            {
                self.warn(name, m);
            }
            return;
        }
        let ty = self.t.expr(obj);
        let Some(t) = api.get_type(api::member_type(&ty)) else { return };
        let n = name.text.as_str();
        let names = || t.props.iter().map(|p| p.name.as_str()).chain(t.methods.iter().map(|m| m.name.as_str()));
        let unknown = |what: &str| -> String {
            if let Some(r) = t.removed.iter().find(|r| r.name == n) {
                return r.message(&format!("{}.{}", t.name, n));
            }
            match closest(n, names()) {
                Some(c) => format!("{} hat keine {} {} – meinst du {}?", t.name, what, n, c),
                None => format!("{} hat keine {} {}", t.name, what, n),
            }
        };
        match how {
            Use::Get | Use::Set => {
                if let Some(p) = t.find_prop(n) {
                    if p.readonly && matches!(how, Use::Set) {
                        self.warn(name, format!("{}.{} ist nur lesbar", t.name, n));
                    }
                } else if t.find_method(n).is_some() {
                    self.warn(name, format!("{} ist eine Methode von {}, keine Eigenschaft – mit Klammern: {}(..)", n, t.name, n));
                } else {
                    let m = unknown("Eigenschaft");
                    self.warn(name, m);
                }
            }
            Use::Call(args) => {
                if n == "on" || n == "off" {
                    if t.events.is_empty() {
                        self.warn(name, format!("{} hat keine Ereignisse", t.name));
                    } else if let Some(Expr { kind: ExprKind::Str(ev), span, .. }) = args.first()
                        && t.find_event(ev).is_none()
                    {
                        let mut m = format!("{} hat kein Ereignis {}", t.name, ev);
                        if let Some(c) = closest(ev, t.events.iter().map(|x| x.name.as_str())) {
                            m.push_str(&format!(" – meinst du {}?", c));
                        }
                        let at = Name { text: format!("\"{}\"", ev), span: *span };
                        self.warn(&at, m);
                    }
                } else if t.find_method(n).is_none() {
                    if t.find_prop(n).is_some() {
                        self.warn(name, format!("{} ist eine Eigenschaft von {}, keine Methode – ohne Klammern: .{}", n, t.name, n));
                    } else {
                        let m = unknown("Methode");
                        self.warn(name, m);
                    }
                } else if api.check_calls
                    && let Some(m) = api::count_warning(&format!("{}.{}", t.name, n), &t.sigs(n), args.len())
                {
                    self.warn(name, m);
                }
            }
        }
    }
}

/// how a member is used
#[derive(Clone, Copy)]
enum Use<'a> {
    Get,
    Set,
    Call(&'a [Expr]),
}
