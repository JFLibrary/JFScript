// ast -- the syntax tree of a .jfs script (parse() builds it, the compiler reads it)
//
//   Program { name, body }   the top-level statements
//   Stmt { span, kind }      StmtKind: Let, Fn, If, While, For, Break, Continue, Return, Try,
//                            Throw, Expr, Assign, (S3) AssignPat, Block
//   Expr { span, kind }      ExprKind: literals, Interp, List, Map, Fn, Var, Unary (- ! and
//                            (S3) await), Binary, Call, Method, Field, Index, (S3) Cond (c ? a :
//                            b), Named (an argument `width: 2`), Comp ([e for x in xs if c])
//   Pattern { span, kind }   (S3) what let, parameters, for and comprehensions bind: a name,
//                            [a, b, ...rest], {a, b: c}, p = default; Target leaves in a
//                            destructuring assignment ([a, b] = [b, a])
//   Span                     bytes start..end plus line and column of the start (1-based,
//                            columns count characters); a name's span.start is unique per
//                            source, a key for side tables of later passes
//   x.sexpr()                the tree as S-expressions: (+ 1 (* 2 3)), (call f a), (. m k) ..
//
// Scopes (check::resolve and the compiler follow the same rules):
//   * top-level let and fn are the script's globals: visible in every function wherever they
//     are declared (functions run later, as callbacks); top-level code itself reads a let
//     only after it ran (check warns otherwise); (S3) every name of a top-level let [a, b] = ..
//     is a global
//   * a block { } is a scope: its fn declarations are visible in the whole block (local
//     functions may call each other), a let from the next statement on; a second let of a
//     name in one block is an error, an inner block may shadow it
//   * parameters form a scope around the function body; the for variables and the catch
//     variable a scope around their block; (S3) a parameter's default sees the parameters
//     before it (and what is visible around the fn), a default inside a pattern the names of
//     the pattern before it
//   * (S3) a list comprehension is a scope of its own: each for clause's variables are visible
//     in the clauses after it and in the expression in front; the first clause's source is
//     read outside of it
//   * fn declarations and fn literals are closures: they capture the locals they use
//   * && || ?? evaluate their right side only when needed, (S3) c ? a : b one of a and b; a..b
//     exists only in for (and the for clauses of comprehensions) and counts a, a + 1 .. b - 1
//   * (S3) a map literal's {a} is {a: a}: the key and the value share the name's span

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// a source range: bytes start..end, line and column of start (1-based; columns count characters)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub line: u32,
    pub col: u32,
}

impl Span {
    /// from the start of self to the end of other
    pub fn to(self, other: Span) -> Span {
        Span { end: other.end.max(self.end), ..self }
    }
}

/// a name where it is written: declarations, parameters, field and method names
#[derive(Clone, PartialEq, Debug)]
pub struct Name {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Program {
    /// the name given to parse() (a file name for messages)
    pub name: String,
    pub body: Vec<Stmt>,
}

/// { statements }
#[derive(Clone, PartialEq, Debug)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

/// a function: fn name(params) { body }, or the literal fn(params) { body } without a name;
/// (S3) a parameter is a pattern, `p = e` with a default
#[derive(Clone, PartialEq, Debug)]
pub struct FnDef {
    pub span: Span,
    pub name: Option<Name>,
    pub params: Vec<Pattern>,
    pub body: Block,
}

/// (S3) what a let, a parameter, a for loop or a comprehension's for clause binds
#[derive(Clone, PartialEq, Debug)]
pub struct Pattern {
    pub span: Span,
    pub kind: PatKind,
}

#[derive(Clone, PartialEq, Debug)]
pub enum PatKind {
    /// a name: let x, fn f(x), for x in ..
    Name(Name),
    /// [a, b, ...rest]: a list's items by position (nil past its end), rest a new list of the
    /// items after them
    List(Vec<Pattern>, Option<Box<Pattern>>),
    /// {a, b: c}: (key, pattern) of a map's entries or a host object's properties (nil for a
    /// missing key); {a} is {a: a}, the pattern's name then has the key's span
    Map(Vec<(Name, Pattern)>),
    /// p = e: e when the value is nil (a missing argument, item or key)
    Default(Box<Pattern>, Box<Expr>),
    /// (a destructuring assignment [a, m.k, l[i]] = ..) a variable, field or index that exists
    Target(Box<Expr>),
}

impl Pattern {
    /// the name of a pattern that is one (`x`, also `x = 1`)
    pub fn name(&self) -> Option<&Name> {
        match &self.kind {
            PatKind::Name(n) => Some(n),
            PatKind::Default(p, _) => p.name(),
            _ => None,
        }
    }

    /// the default of `p = e` (a parameter's)
    pub fn default(&self) -> Option<&Expr> {
        match &self.kind {
            PatKind::Default(_, e) => Some(e),
            _ => None,
        }
    }

    /// the pattern without its default
    pub fn inner(&self) -> &Pattern {
        match &self.kind {
            PatKind::Default(p, _) => p.inner(),
            _ => self,
        }
    }

    /// every name it binds, in source order
    pub fn names<'a>(&'a self, out: &mut Vec<&'a Name>) {
        match &self.kind {
            PatKind::Name(n) => out.push(n),
            PatKind::List(items, rest) => {
                for p in items {
                    p.names(out);
                }
                if let Some(r) = rest {
                    r.names(out);
                }
            }
            PatKind::Map(entries) => {
                for (_, p) in entries {
                    p.names(out);
                }
            }
            PatKind::Default(p, _) => p.names(out),
            PatKind::Target(_) => {}
        }
    }

    /// the pattern as written, short: "x", "[x, y]", "{w, h}", "b = …" (signatures, hovers)
    pub fn label(&self) -> String {
        match &self.kind {
            PatKind::Name(n) => n.text.clone(),
            PatKind::List(items, rest) => {
                let mut v: Vec<String> = items.iter().map(Pattern::label).collect();
                if let Some(r) = rest {
                    v.push(format!("...{}", r.label()));
                }
                format!("[{}]", v.join(", "))
            }
            PatKind::Map(entries) => {
                let v: Vec<String> = entries
                    .iter()
                    .map(|(k, p)| match &p.inner().kind {
                        PatKind::Name(n) if n.text == k.text && p.default().is_none() => k.text.clone(),
                        PatKind::Name(n) if n.text == k.text => format!("{} = …", k.text),
                        _ => format!("{}: {}", k.text, p.label()),
                    })
                    .collect();
                format!("{{{}}}", v.join(", "))
            }
            PatKind::Default(p, _) => format!("{} = …", p.label()),
            PatKind::Target(e) => e.sexpr(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}

#[derive(Clone, PartialEq, Debug)]
pub enum StmtKind {
    /// let pat = init (no init: nil; (S3) a pattern other than a name needs one)
    Let { pat: Pattern, init: Option<Expr> },
    /// fn name(params) { } (the FnDef has a name)
    Fn(FnDef),
    /// if c { } else if c { } else { }: the (condition, block) arms in order
    If { arms: Vec<(Expr, Block)>, els: Option<Block> },
    While { cond: Expr, body: Block },
    /// for a in src / for a, b in src (key and value of a map, index and item of a list); (S3)
    /// a and b are patterns (for [i, p] in pairs)
    For { a: Pattern, b: Option<Pattern>, src: ForSrc, body: Block },
    Break,
    Continue,
    Return(Option<Expr>),
    /// try { body } catch var { catch }
    Try { body: Block, var: Option<Name>, catch: Block },
    Throw(Expr),
    Expr(Expr),
    /// target = value, or target op= value (op Some(Add) for +=); target is Var, Field or Index
    Assign { op: Option<BinOp>, target: Expr, value: Expr },
    /// (S3) [a, b] = value: a list pattern of Target leaves (and nested list patterns)
    AssignPat { pat: Pattern, value: Expr },
    /// a bare { } block
    Block(Block),
}

/// what a for loop runs over
#[derive(Clone, PartialEq, Debug)]
pub enum ForSrc {
    /// a list, map, str or bytes value
    Each(Expr),
    /// lo..hi
    Range(Expr, Expr),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ExprKind {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    /// "a${x}b": the pieces in order, Str for the text (empty pieces left out)
    Interp(Vec<Expr>),
    List(Vec<Expr>),
    /// {k: v, "s": v, 1: v, [e]: v}: (key, value) in order; a bare name key is a Str
    Map(Vec<(Expr, Expr)>),
    Fn(Box<FnDef>),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// f(args)
    Call(Box<Expr>, Vec<Expr>),
    /// obj.name(args)
    Method(Box<Expr>, Name, Vec<Expr>),
    /// obj.name
    Field(Box<Expr>, Name),
    /// obj[index]
    Index(Box<Expr>, Box<Expr>),
    /// (S3) c ? a : b
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    /// (S3) a named argument of a call or method call: `width: 2` (positional ones come first)
    Named(Name, Box<Expr>),
    /// (S3) [e for a in src if c ..]: the expression and the clauses in order, the first a for
    Comp(Box<Expr>, Vec<Clause>),
}

/// (S3) a clause of a list comprehension
#[derive(Clone, PartialEq, Debug)]
pub enum Clause {
    /// for a in src / for a, b in src, as in a for loop
    For { a: Pattern, b: Option<Pattern>, src: ForSrc },
    /// if c: the rounds where c is truthy
    If(Expr),
}

/// the unary operators: minus, not (!) and (S3) await
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnOp {
    Neg,
    Not,
    /// await e: the value of a promise once it is there (vm.rs, task.rs)
    Await,
}

/// binary operators; And, Or and Coalesce (??) short-circuit
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    Mul, Div, Rem, Add, Sub, Lt, Le, Gt, Ge, Eq, Ne, And, Or, Coalesce,
}

impl BinOp {
    /// the operator as written: "*", "&&", "??" ..
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::Coalesce => "??",
        }
    }
}

// ---- (S3) visiting every node

/// a node of the tree for visit()
#[derive(Clone, Copy)]
pub enum Node<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
    Pattern(&'a Pattern),
}

/// (S3) every statement, expression and pattern of the statements, outer ones before the ones
/// inside them, in source order, the bodies of fn declarations and literals included (for
/// passes that look at all of them)
pub fn visit<'a>(stmts: &'a [Stmt], f: &mut dyn FnMut(Node<'a>)) {
    for s in stmts {
        visit_stmt(s, f);
    }
}

fn visit_block<'a>(b: &'a Block, f: &mut dyn FnMut(Node<'a>)) {
    visit(&b.stmts, f);
}

fn visit_fn<'a>(d: &'a FnDef, f: &mut dyn FnMut(Node<'a>)) {
    for p in &d.params {
        visit_pattern(p, f);
    }
    visit_block(&d.body, f);
}

fn visit_src<'a>(src: &'a ForSrc, f: &mut dyn FnMut(Node<'a>)) {
    match src {
        ForSrc::Each(e) => visit_expr(e, f),
        ForSrc::Range(a, b) => {
            visit_expr(a, f);
            visit_expr(b, f);
        }
    }
}

fn visit_stmt<'a>(s: &'a Stmt, f: &mut dyn FnMut(Node<'a>)) {
    f(Node::Stmt(s));
    match &s.kind {
        StmtKind::Let { pat, init } => {
            visit_pattern(pat, f);
            if let Some(e) = init {
                visit_expr(e, f);
            }
        }
        StmtKind::Fn(d) => visit_fn(d, f),
        StmtKind::If { arms, els } => {
            for (c, b) in arms {
                visit_expr(c, f);
                visit_block(b, f);
            }
            if let Some(b) = els {
                visit_block(b, f);
            }
        }
        StmtKind::While { cond, body } => {
            visit_expr(cond, f);
            visit_block(body, f);
        }
        StmtKind::For { a, b, src, body } => {
            visit_pattern(a, f);
            if let Some(b) = b {
                visit_pattern(b, f);
            }
            visit_src(src, f);
            visit_block(body, f);
        }
        StmtKind::Break | StmtKind::Continue | StmtKind::Return(None) => {}
        StmtKind::Return(Some(e)) | StmtKind::Throw(e) | StmtKind::Expr(e) => visit_expr(e, f),
        StmtKind::Try { body, catch, .. } => {
            visit_block(body, f);
            visit_block(catch, f);
        }
        StmtKind::Assign { target, value, .. } => {
            visit_expr(target, f);
            visit_expr(value, f);
        }
        StmtKind::AssignPat { pat, value } => {
            visit_pattern(pat, f);
            visit_expr(value, f);
        }
        StmtKind::Block(b) => visit_block(b, f),
    }
}

fn visit_pattern<'a>(p: &'a Pattern, f: &mut dyn FnMut(Node<'a>)) {
    f(Node::Pattern(p));
    match &p.kind {
        PatKind::Name(_) => {}
        PatKind::List(items, rest) => {
            for x in items {
                visit_pattern(x, f);
            }
            if let Some(r) = rest {
                visit_pattern(r, f);
            }
        }
        PatKind::Map(entries) => {
            for (_, x) in entries {
                visit_pattern(x, f);
            }
        }
        PatKind::Default(x, e) => {
            visit_pattern(x, f);
            visit_expr(e, f);
        }
        PatKind::Target(e) => visit_expr(e, f),
    }
}

fn visit_expr<'a>(e: &'a Expr, f: &mut dyn FnMut(Node<'a>)) {
    f(Node::Expr(e));
    match &e.kind {
        ExprKind::Nil | ExprKind::Bool(_) | ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) | ExprKind::Var(_) => {}
        ExprKind::Interp(v) | ExprKind::List(v) => v.iter().for_each(|x| visit_expr(x, f)),
        ExprKind::Map(kv) => {
            for (k, v) in kv {
                visit_expr(k, f);
                visit_expr(v, f);
            }
        }
        ExprKind::Fn(d) => visit_fn(d, f),
        ExprKind::Unary(_, a) | ExprKind::Field(a, _) | ExprKind::Named(_, a) => visit_expr(a, f),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
            visit_expr(a, f);
            visit_expr(b, f);
        }
        ExprKind::Call(g, args) | ExprKind::Method(g, _, args) => {
            visit_expr(g, f);
            args.iter().for_each(|x| visit_expr(x, f));
        }
        ExprKind::Cond(c, a, b) => {
            visit_expr(c, f);
            visit_expr(a, f);
            visit_expr(b, f);
        }
        ExprKind::Comp(x, clauses) => {
            for c in clauses {
                match c {
                    Clause::For { a, b, src } => {
                        visit_src(src, f);
                        visit_pattern(a, f);
                        if let Some(b) = b {
                            visit_pattern(b, f);
                        }
                    }
                    Clause::If(c) => visit_expr(c, f),
                }
            }
            visit_expr(x, f);
        }
    }
}

// ---- S-expressions

impl Program {
    /// one S-expression per top-level statement, one per line
    pub fn sexpr(&self) -> String {
        let v: Vec<String> = self.body.iter().map(Stmt::sexpr).collect();
        v.join("\n")
    }
}

impl Stmt {
    /// (let x 1), (fn f (a) (block ..)), (if c (block ..) else (block ..)), (= x 1) ..
    pub fn sexpr(&self) -> String {
        let mut o = String::new();
        stmt(&mut o, self);
        o
    }
}

impl Expr {
    /// (+ 1 (* 2 3)), (call f a), (method s split ","), (. m k), (index a 0), (interp "a" x) ..
    pub fn sexpr(&self) -> String {
        let mut o = String::new();
        expr(&mut o, self);
        o
    }
}

fn block(o: &mut String, b: &Block) {
    o.push_str("(block");
    for s in &b.stmts {
        o.push(' ');
        stmt(o, s);
    }
    o.push(')');
}

fn func(o: &mut String, f: &FnDef) {
    o.push_str("(fn ");
    if let Some(n) = &f.name {
        o.push_str(&n.text);
        o.push(' ');
    }
    o.push('(');
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            o.push(' ');
        }
        pattern(o, p);
    }
    o.push_str(") ");
    block(o, &f.body);
    o.push(')');
}

/// x, [a b ...r], {a key:p}, (= p e); a Target leaf as its expression
fn pattern(o: &mut String, p: &Pattern) {
    match &p.kind {
        PatKind::Name(n) => o.push_str(&n.text),
        PatKind::List(items, rest) => {
            o.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    o.push(' ');
                }
                pattern(o, x);
            }
            if let Some(r) = rest {
                if !items.is_empty() {
                    o.push(' ');
                }
                o.push_str("...");
                pattern(o, r);
            }
            o.push(']');
        }
        PatKind::Map(entries) => {
            o.push('{');
            for (i, (k, x)) in entries.iter().enumerate() {
                if i > 0 {
                    o.push(' ');
                }
                if matches!(&x.kind, PatKind::Name(n) if n.text == k.text) {
                    o.push_str(&k.text);
                } else {
                    quote_key(o, &k.text);
                    o.push(':');
                    pattern(o, x);
                }
            }
            o.push('}');
        }
        PatKind::Default(x, e) => {
            o.push_str("(= ");
            pattern(o, x);
            o.push(' ');
            expr(o, e);
            o.push(')');
        }
        PatKind::Target(e) => expr(o, e),
    }
}

/// a map pattern's key: a name as it is, another text quoted
fn quote_key(o: &mut String, k: &str) {
    let mut it = k.chars();
    let name = it.next().is_some_and(|c| c.is_alphabetic() || c == '_') && it.all(|c| c.is_alphanumeric() || c == '_');
    if name { o.push_str(k) } else { quote(o, k) }
}

/// the head of a for loop or clause: a or (a b), then the source
fn for_head(o: &mut String, a: &Pattern, b: Option<&Pattern>, src: &ForSrc) {
    match b {
        Some(b) => {
            o.push('(');
            pattern(o, a);
            o.push(' ');
            pattern(o, b);
            o.push(')');
        }
        None => pattern(o, a),
    }
    o.push(' ');
    match src {
        ForSrc::Each(e) => expr(o, e),
        ForSrc::Range(lo, hi) => {
            o.push_str("(.. ");
            expr(o, lo);
            o.push(' ');
            expr(o, hi);
            o.push(')');
        }
    }
}

fn stmt(o: &mut String, s: &Stmt) {
    match &s.kind {
        StmtKind::Let { pat, init } => {
            o.push_str("(let ");
            pattern(o, pat);
            if let Some(e) = init {
                o.push(' ');
                expr(o, e);
            }
            o.push(')');
        }
        StmtKind::Fn(f) => func(o, f),
        StmtKind::If { arms, els } => {
            o.push_str("(if");
            for (c, b) in arms {
                o.push(' ');
                expr(o, c);
                o.push(' ');
                block(o, b);
            }
            if let Some(b) = els {
                o.push_str(" else ");
                block(o, b);
            }
            o.push(')');
        }
        StmtKind::While { cond, body } => {
            o.push_str("(while ");
            expr(o, cond);
            o.push(' ');
            block(o, body);
            o.push(')');
        }
        StmtKind::For { a, b, src, body } => {
            o.push_str("(for ");
            for_head(o, a, b.as_ref(), src);
            o.push(' ');
            block(o, body);
            o.push(')');
        }
        StmtKind::Break => o.push_str("break"),
        StmtKind::Continue => o.push_str("continue"),
        StmtKind::Return(v) => {
            o.push_str("(return");
            if let Some(e) = v {
                o.push(' ');
                expr(o, e);
            }
            o.push(')');
        }
        StmtKind::Try { body, var, catch } => {
            o.push_str("(try ");
            block(o, body);
            o.push(' ');
            o.push_str(var.as_ref().map_or("_", |v| v.text.as_str()));
            o.push(' ');
            block(o, catch);
            o.push(')');
        }
        StmtKind::Throw(e) => {
            o.push_str("(throw ");
            expr(o, e);
            o.push(')');
        }
        StmtKind::Expr(e) => expr(o, e),
        StmtKind::Assign { op, target, value } => {
            o.push('(');
            o.push_str(op.map_or("", BinOp::text));
            o.push_str("= ");
            expr(o, target);
            o.push(' ');
            expr(o, value);
            o.push(')');
        }
        StmtKind::AssignPat { pat, value } => {
            o.push_str("(= ");
            pattern(o, pat);
            o.push(' ');
            expr(o, value);
            o.push(')');
        }
        StmtKind::Block(b) => block(o, b),
    }
}

fn list(o: &mut String, head: &str, items: &[Expr]) {
    o.push('(');
    o.push_str(head);
    for e in items {
        o.push(' ');
        expr(o, e);
    }
    o.push(')');
}

fn expr(o: &mut String, e: &Expr) {
    match &e.kind {
        ExprKind::Nil => o.push_str("nil"),
        ExprKind::Bool(b) => o.push_str(if *b { "true" } else { "false" }),
        ExprKind::Int(v) => o.push_str(&format!("{}", v)),
        ExprKind::Float(v) => o.push_str(&format!("{:?}", v)),
        ExprKind::Str(s) => quote(o, s),
        ExprKind::Interp(parts) => list(o, "interp", parts),
        ExprKind::List(items) => list(o, "list", items),
        ExprKind::Map(kv) => {
            o.push_str("(map");
            for (k, v) in kv {
                o.push_str(" (");
                expr(o, k);
                o.push(' ');
                expr(o, v);
                o.push(')');
            }
            o.push(')');
        }
        ExprKind::Fn(f) => func(o, f),
        ExprKind::Var(n) => o.push_str(n),
        ExprKind::Unary(op, a) => {
            o.push_str(match op {
                UnOp::Neg => "(- ",
                UnOp::Not => "(! ",
                UnOp::Await => "(await ",
            });
            expr(o, a);
            o.push(')');
        }
        ExprKind::Binary(op, a, b) => {
            o.push('(');
            o.push_str(op.text());
            o.push(' ');
            expr(o, a);
            o.push(' ');
            expr(o, b);
            o.push(')');
        }
        ExprKind::Call(f, args) => {
            o.push_str("(call ");
            expr(o, f);
            for a in args {
                o.push(' ');
                expr(o, a);
            }
            o.push(')');
        }
        ExprKind::Method(obj, name, args) => {
            o.push_str("(method ");
            expr(o, obj);
            o.push(' ');
            o.push_str(&name.text);
            for a in args {
                o.push(' ');
                expr(o, a);
            }
            o.push(')');
        }
        ExprKind::Field(obj, name) => {
            o.push_str("(. ");
            expr(o, obj);
            o.push(' ');
            o.push_str(&name.text);
            o.push(')');
        }
        ExprKind::Index(obj, i) => {
            o.push_str("(index ");
            expr(o, obj);
            o.push(' ');
            expr(o, i);
            o.push(')');
        }
        ExprKind::Cond(c, a, b) => {
            o.push_str("(? ");
            expr(o, c);
            o.push(' ');
            expr(o, a);
            o.push(' ');
            expr(o, b);
            o.push(')');
        }
        ExprKind::Named(n, v) => {
            o.push_str("(: ");
            o.push_str(&n.text);
            o.push(' ');
            expr(o, v);
            o.push(')');
        }
        ExprKind::Comp(e, clauses) => {
            o.push_str("(comp ");
            expr(o, e);
            for c in clauses {
                match c {
                    Clause::For { a, b, src } => {
                        o.push_str(" (for ");
                        for_head(o, a, b.as_ref(), src);
                        o.push(')');
                    }
                    Clause::If(x) => {
                        o.push_str(" (if ");
                        expr(o, x);
                        o.push(')');
                    }
                }
            }
            o.push(')');
        }
    }
}

/// a string in double quotes with \" \\ \n \t and \u{..} for other control characters
fn quote(o: &mut String, s: &str) {
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if c.is_control() => o.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
}
