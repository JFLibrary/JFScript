// ls -- language services for editors: the symbols of a script with their positions, go to
// definition, references, rename, completion, hover, signature help, folding ranges and the
// symbols of an outline.  Generic (no editor types); everything works on code that does not
// parse (parse_partial: what parsed) and never fails.
//
//   index(src) -> Index              every declaration (Symbol: name, kind, where it is
//                                    declared and visible, parameters, doc comment, the fn it
//                                    is in, an import path) and every use of a name (Use: its
//                                    span and the symbol it means, None for a name of the host
//                                    or the library, or an unknown one), by the scope rules of
//                                    ast.rs; idx.at(pos) the symbol at a position
//   definition(src, pos, &cx) -> Option<Location>   where the name at pos is declared; a member
//                                    of a module (m.f with let m = import("x.jfs")) in the
//                                    other file, through cx.load
//   references(src, pos) -> Vec<Span>   the declaration and every use of the symbol at pos,
//                                    in source order
//   rename(src, pos, new) -> Result<Vec<Edit>, String>   the edits that rename the symbol at
//                                    pos; refused (a reason, msg.rs): a keyword, no valid name, a
//                                    name that clashes (declared in the same scope, or a use
//                                    that would then mean another declaration), code that does
//                                    not parse, no symbol of this script at pos
//   complete(src, pos, &cx) -> Completion   the items for the word at pos (from its start to
//                                    pos as the prefix): the names in scope, the host's
//                                    globals (cx.host), the built-ins and the library's
//                                    modules, keywords where a statement may start; after a dot
//                                    the members of what is before it: the methods of str /
//                                    list / map / bytes (all four when its type is not known
//                                    from a literal), the functions of a library module, the
//                                    host's dotted names, the top-level names of an imported
//                                    module.  Ranked: the exact prefix first, then by kind
//                                    (locals, the script's globals, the host, the library,
//                                    keywords), then by name.  Nothing in comments and strings
//   hover(src, pos, &cx) -> Option<Hover>   kind, signature ("fn greet(name)") and the doc
//                                    comment, or the description of a library or host name
//   signature(src, pos, &cx) -> Option<Signature>   inside the parentheses of a call: the
//                                    function's label, the parameters' ranges in it, the one
//                                    at pos (commas counted); (S3) every form (overload) of a
//                                    host function or method, the one shown the first with room
//                                    for the arguments typed so far whose known types fit
//   folding(src) -> Vec<Fold>        blocks, /* */ comments and runs of // lines, lists, maps
//                                    and calls over several lines: the first line stays, the
//                                    rest up to `end` folds (a closing line of its own stays)
//   symbols(src) -> Vec<DocSymbol>   the outline: top-level fns (with the fns declared inside
//                                    them as children) and lets in source order
//   CompleteCx { host, load }        the host's globals (HostItem: "net.listen", parameters,
//                                    a short description) and the reader of imported files
//   Context { api, host, load }      (S2) CompleteCx with the host's typed description
//                                    (script::Api, api.rs); definition, complete, hover and
//                                    signature take either (`&cx` of both, or `&api`)
//   check(src, cx) -> Vec<Diagnostic>   (S2) the editor's diagnostics for a host: syntax, the
//                                    names (unknown ones are errors, as compile_with makes
//                                    them), and with an Api the members of known host types,
//                                    (S3) removed names and, with Api::check_calls, calls
//                                    whose number of arguments fits none of their forms; its
//                                    warnings are what script::compile_api gives
//   apply(src, &edits) -> String     the source with edits; is_name(s) a valid new name;
//                                    library_doc(name) the parameters and description of a
//                                    built-in or library name (lsdoc.rs)
//
// The syntax pass (S3): the names of patterns are symbols (globals, locals, parameters, loop
// variables; a comprehension's variables Loop symbols visible in it), a named argument of a call
// of the script's own fn is a use of that parameter (references and rename go to the call
// sites; a rename of a shorthand {a} writes {a: new}), one of another call a use without a
// symbol; hover on it shows the parameter, completion at an argument's start offers the
// parameters still to name ("width:", from a script fn, the description or the library),
// signature help makes a named argument's parameter the active one; the parameters' labels
// show defaults ("b = 2", "b = …") and patterns ("[x, y]").
//
// With an Api (S2, lsapi.rs): completion after a dot lists the members of the type the code
// before it has (a host value such as `doc`, a host function's result, a property, a chain
// such as `doc.page.shapes[0]`, a let of one of those, the item of a list<T> in a for, the
// parameter of a function given to obj.on("event", ..)); hover shows a member's declaration
// and a variable's type; signature help a method's parameters with their types; check warns
// about members the type does not have.  Where the type is not known, everything is as before.
//
// Doc comments: the `///` lines right above a fn or let (no blank line between) belong to it;
// their text without the slashes and one blank is the doc.  Positions: bytes into src (a
// position at the end of a word belongs to the word); spans as ast::Span (lines and columns
// from 1, columns in characters).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::api::{Api, GlobalKind, Sig};
use crate::ast::*;
use crate::lex::{self, Tok, TokKind};
use crate::lsapi::{self, Known, Types};
use crate::lsdoc;
use crate::Diagnostic;

/// what a symbol is
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SymKind {
    /// a top-level let (a global of the script)
    Global,
    /// a fn declaration (any level)
    Fn,
    /// a let inside a block
    Local,
    /// a parameter of a fn or a fn literal
    Param,
    /// a for variable
    Loop,
    /// the variable of a catch
    Catch,
}

impl SymKind {
    /// the word for a hover in the current language ("Funktion", "lokale Variable" ..)
    pub fn word(self) -> &'static str {
        match self {
            SymKind::Global => t!("globale Variable"),
            SymKind::Fn => t!("Funktion"),
            SymKind::Local => t!("lokale Variable"),
            SymKind::Param => t!("Parameter"),
            SymKind::Loop => t!("Schleifenvariable"),
            SymKind::Catch => t!("Fehlervariable"),
        }
    }
}

/// a declaration (see the module comment)
#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymKind,
    /// the name where it is declared
    pub span: Span,
    /// the whole declaration: the let statement, the fn up to its `}`, the parameter, the loop
    pub decl: Span,
    /// the bytes where the name is visible (a top-level let: the whole script; functions read
    /// it later, top-level code from `from` on)
    pub scope: (u32, u32),
    pub from: u32,
    /// the parameters of a fn, also of a let bound to fn(..) { }
    pub params: Vec<String>,
    /// the doc comment (see the module comment)
    pub doc: String,
    /// the symbol of the fn (or the let of a fn literal) it is declared in
    pub parent: Option<usize>,
    /// let m = import("path"): the path
    pub import: Option<String>,
    /// the type of a let's value when a literal or a known call tells it: "str", "list",
    /// "map", "bytes", "fn" ("": not known)
    pub ty: &'static str,
    /// the scope it is declared in (symbols of one scope clash)
    pub(crate) scope_id: u32,
}

/// a name that is read or written
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Use {
    pub span: Span,
    /// the symbol (None: a name of the host or the library, or unknown)
    pub sym: Option<usize>,
    /// an assignment to it
    pub write: bool,
}

/// the symbols and uses of a source
#[derive(Clone, Debug, Default)]
pub struct Index {
    pub symbols: Vec<Symbol>,
    /// by position; (S3) a named argument of a call of the script's fn is a use of its
    /// parameter (one of another call a use without a symbol)
    pub uses: Vec<Use>,
    /// (S3) where a shorthand {a} names a variable (map literals and patterns), sorted: a rename
    /// writes {a: new} there
    pub(crate) shorthand: Vec<u32>,
}

impl Index {
    /// the symbol whose declared name or one of whose uses is at byte pos (a position at a
    /// name's end counts)
    pub fn at(&self, pos: u32) -> Option<usize> {
        let hit = |s: Span| s.start <= pos && pos <= s.end;
        if let Some(u) = self.use_at(pos) {
            return u.sym;
        }
        self.symbols.iter().position(|s| hit(s.span))
    }

    /// the use at byte pos
    pub fn use_at(&self, pos: u32) -> Option<&Use> {
        let k = self.uses.partition_point(|u| u.span.end < pos);
        self.uses[k..].iter().take_while(|u| u.span.start <= pos).find(|u| u.span.start <= pos && pos <= u.span.end)
    }

    /// the use that starts at byte start
    pub(crate) fn use_from(&self, start: u32) -> Option<&Use> {
        let k = self.uses.partition_point(|u| u.span.start < start);
        self.uses.get(k).filter(|u| u.span.start == start)
    }

    /// the symbol a name at byte `at` means by the visible ranges (for a name in a statement
    /// that did not parse, which has no Use): the innermost one of that name
    pub fn resolve(&self, name: &str, at: u32) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (i, s) in self.symbols.iter().enumerate() {
            if s.name == name && s.scope.0 <= at && at <= s.scope.1 && best.is_none_or(|b| self.symbols[b].scope.0 < s.scope.0) {
                best = Some(i);
            }
        }
        best
    }

    /// the symbol of the name token at byte start: its use, or (no use: a statement that did
    /// not parse) by the visible ranges; a declaration's own name
    pub(crate) fn sym_of(&self, name: &str, start: u32) -> Option<usize> {
        match self.use_from(start) {
            Some(u) => u.sym,
            None => self.symbols.iter().position(|s| s.span.start == start).or_else(|| self.resolve(name, start)),
        }
    }
}

/// a place: a span in this source (path None) or in an imported file
#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub path: Option<String>,
    pub span: Span,
}

/// a change of the source: bytes start..end become text
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

/// a name the host gives scripts (for completion and hover)
#[derive(Clone, Copy, Debug)]
pub struct HostItem<'a> {
    /// "log", "net.listen", "config"
    pub name: &'a str,
    /// the parameters of a function ("port, handlers"); None: a value or a namespace
    pub params: Option<&'a str>,
    /// a short description (in the host's language)
    pub doc: &'a str,
}

/// what the services know beyond the source
#[derive(Clone, Copy, Default)]
pub struct CompleteCx<'a> {
    /// the host's globals
    pub host: &'a [HostItem<'a>],
    /// the text of an imported file by the path written in import("…") (None: not there)
    pub load: Option<Loader<'a>>,
}

/// the reader of imported files: the text by the path written in import("…"), None when not there
pub type Loader<'a> = &'a dyn Fn(&str) -> Option<String>;

/// (S2) what the services know beyond the source, with the host's typed description: a
/// CompleteCx plus an Api (both kinds of description may be given; the Api's names come first)
#[derive(Clone, Copy, Default)]
pub struct Context<'a> {
    /// the host's object model (api.rs)
    pub api: Option<&'a Api>,
    /// host names in the form before S2
    pub host: &'a [HostItem<'a>],
    /// the text of an imported file by the path written in import("…") (None: not there)
    pub load: Option<Loader<'a>>,
}

impl<'a> Context<'a> {
    /// the services with this description
    pub fn new(api: &'a Api) -> Context<'a> {
        Context { api: Some(api), host: &[], load: None }
    }

    /// and the reader of imported files
    pub fn with_loader(mut self, load: Loader<'a>) -> Context<'a> {
        self.load = Some(load);
        self
    }
}

impl<'a> From<&CompleteCx<'a>> for Context<'a> {
    fn from(c: &CompleteCx<'a>) -> Context<'a> {
        Context { api: None, host: c.host, load: c.load }
    }
}

impl<'a> From<CompleteCx<'a>> for Context<'a> {
    fn from(c: CompleteCx<'a>) -> Context<'a> {
        Context { api: None, host: c.host, load: c.load }
    }
}

impl<'a> From<&Context<'a>> for Context<'a> {
    fn from(c: &Context<'a>) -> Context<'a> {
        *c
    }
}

impl<'a> From<&'a Api> for Context<'a> {
    fn from(api: &'a Api) -> Context<'a> {
        Context::new(api)
    }
}

/// the kind of a completion item
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemKind {
    /// a let, a for or catch variable
    Variable,
    Param,
    /// a fn of the script, a built-in, a function of the library or the host
    Function,
    /// a method of str / list / map / bytes
    Method,
    /// a library module, a host namespace, an imported module
    Module,
    /// math.pi, int.max
    Constant,
    /// a value of the host (config)
    Value,
    Keyword,
}

/// a completion item
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub label: String,
    pub kind: ItemKind,
    /// a short line beside it: "fn greet(name)", "s.split(sep, n)", "net.listen(port, handlers)"
    pub detail: String,
    /// the description (doc comment, library or host text)
    pub doc: String,
}

/// what complete() offers: the items replace bytes start..end (the word at pos)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Completion {
    pub start: u32,
    pub end: u32,
    pub items: Vec<Item>,
}

/// what hover() tells of a name
#[derive(Clone, Debug, PartialEq)]
pub struct Hover {
    /// the name it is about
    pub span: Span,
    /// "fn greet(name)", "let hits", "s.split(sep, n)"
    pub sig: String,
    /// "Funktion", "Parameter von greet", "Standardbibliothek" ..
    pub kind: String,
    pub doc: String,
}

impl Hover {
    /// signature, kind and doc on lines of their own (the empty ones left out)
    pub fn text(&self) -> String {
        let parts: Vec<&str> = [self.sig.as_str(), self.kind.as_str(), self.doc.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
        parts.join("\n")
    }
}

/// signature help inside a call: the form the arguments typed so far fit (label, params, active,
/// doc), and (S3) every form of the function
#[derive(Clone, Debug, PartialEq)]
pub struct Signature {
    /// "greet(name, greeting)", "s.split(sep, n)"
    pub label: String,
    /// the bytes of each parameter in label
    pub params: Vec<(u32, u32)>,
    /// the parameter at the position (params.len(): none of them)
    pub active: usize,
    pub doc: String,
    /// (S3) the forms (overloads) in the host's order; one for most functions
    pub forms: Vec<Form>,
    /// which of them the fields above are: the first with room for the argument at the position
    /// whose finished arguments fit where their types are known, else the first with room
    pub form: usize,
}

/// (S3) a form of a function in signature help
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    /// "vec(x: float, y: float, z: float): vec"
    pub label: String,
    /// the bytes of each parameter in label
    pub params: Vec<(u32, u32)>,
    /// the parameter at the position in this form (params.len(): none of them)
    pub active: usize,
    pub doc: String,
}

/// what a folding range holds
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FoldKind {
    Block,
    Comment,
    /// a list, map or argument list
    Literal,
}

/// a folding range: line stays visible, line + 1 ..= end fold (lines from 1)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fold {
    pub line: u32,
    pub end: u32,
    pub kind: FoldKind,
}

/// an entry of the outline
#[derive(Clone, Debug, PartialEq)]
pub struct DocSymbol {
    pub name: String,
    /// Fn or Global
    pub kind: SymKind,
    /// "fn greet(name)", "let hits"
    pub detail: String,
    /// the name
    pub span: Span,
    /// the whole declaration
    pub range: Span,
    pub children: Vec<DocSymbol>,
}

// ---------------------------------------------------------------- lines and tokens

/// the starts of the lines of a source
pub(crate) struct Lines(Vec<u32>);

impl Lines {
    pub(crate) fn new(src: &str) -> Lines {
        let mut v = alloc::vec![0];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                v.push(i as u32 + 1);
            }
        }
        Lines(v)
    }

    /// the line (from 1) of byte b
    pub(crate) fn line(&self, b: u32) -> u32 {
        self.0.partition_point(|&s| s <= b) as u32
    }

    /// the span of bytes a..b (line and column of a)
    pub(crate) fn span(&self, src: &str, a: u32, b: u32) -> Span {
        let line = self.line(a);
        let s = self.0[line as usize - 1] as usize;
        let col = src.get(s..a as usize).map_or(0, |t| t.chars().count()) as u32 + 1;
        Span { start: a, end: b, line, col }
    }

    /// the text of line l (from 1) without its break
    pub(crate) fn text<'a>(&self, src: &'a str, l: u32) -> &'a str {
        let a = self.0.get(l as usize - 1).copied().unwrap_or(src.len() as u32) as usize;
        let b = self.0.get(l as usize).map_or(src.len(), |&e| e as usize - 1);
        src.get(a..b.max(a)).unwrap_or("").trim_end_matches('\r')
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic() || (!c.is_ascii() && c.is_alphabetic())
}

fn is_ident_char(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric() || (!c.is_ascii() && c.is_alphanumeric())
}

/// the parameters (None: a constant) and the description of a built-in or library name
/// ("print", "str.split", "math.pi")
pub fn library_doc(name: &str) -> Option<(Option<&'static str>, &'static str)> {
    lsdoc::doc(name).map(|d| (d.params, d.doc()))
}

/// a valid name that is no keyword
pub fn is_name(s: &str) -> bool {
    let mut it = s.chars();
    it.next().is_some_and(is_ident_start) && it.all(is_ident_char) && TokKind::keyword(s).is_none()
}

/// the token (of all of them) that holds byte pos: start <= pos < end, or the one ending at pos
/// when no other starts there
fn tok_at(toks: &[Tok], pos: u32) -> Option<usize> {
    // (not the Eof at the end)
    let toks = &toks[..toks.iter().rposition(|t| t.kind != TokKind::Eof).map_or(0, |i| i + 1)];
    let k = toks.partition_point(|t| t.end <= pos);
    if toks.get(k).is_some_and(|t| t.start <= pos && pos < t.end) {
        return Some(k);
    }
    k.checked_sub(1).filter(|&j| toks[j].end == pos)
}

/// the significant token before index k of the full list (no space, line break, comment or
/// the end)
fn prev_sig(toks: &[Tok], k: usize) -> Option<usize> {
    (0..k.min(toks.len())).rev().find(|&j| !matches!(toks[j].kind, TokKind::Space | TokKind::Newline | TokKind::Comment | TokKind::Eof))
}

/// the significant token after index k of the full list
fn next_sig(toks: &[Tok], k: usize) -> Option<usize> {
    (k + 1..toks.len()).find(|&j| !matches!(toks[j].kind, TokKind::Space | TokKind::Newline | TokKind::Comment | TokKind::Eof))
}

/// a line break between token j and token k (j < k)
fn break_between(toks: &[Tok], src: &str, j: usize, k: usize) -> bool {
    toks[j + 1..k].iter().any(|t| t.kind == TokKind::Newline || (t.kind == TokKind::Comment && src[t.start as usize..t.end as usize].contains('\n')))
}

fn text_of<'a>(src: &'a str, t: &Tok) -> &'a str {
    &src[t.start as usize..t.end as usize]
}

/// the word that ends at or holds byte pos: an Ident or keyword token
fn word_at(toks: &[Tok], pos: u32) -> Option<usize> {
    let k = toks.partition_point(|t| t.end < pos);
    toks[k..].iter().take_while(|t| t.start <= pos).position(|t| t.start <= pos && pos <= t.end && (t.kind == TokKind::Ident || t.kind.is_keyword())).map(|i| k + i)
}

// ---------------------------------------------------------------- the index

struct Res<'a> {
    src: &'a str,
    lines: Lines,
    syms: Vec<Symbol>,
    uses: Vec<Use>,
    top: Vec<usize>,
    /// the scopes inside the top level, innermost last: (scope id, symbols)
    scopes: Vec<(u32, Vec<usize>)>,
    /// the fn symbols around, innermost last
    parents: Vec<Option<usize>>,
    next_scope: u32,
    /// (S3) named arguments of calls of a fn of the script: (the name, the fn's symbol); they
    /// become uses of its parameter when the index is complete
    links: Vec<(Span, String, usize)>,
    /// (S3) the shorthands {a} of map literals and patterns: the name's start
    shorthand: Vec<u32>,
}

/// see the module comment
pub fn index(src: &str) -> Index {
    let (prog, _) = crate::parse_partial("", src);
    index_of(&prog, src)
}

pub(crate) fn index_of(prog: &Program, src: &str) -> Index {
    let mut r = Res {
        src,
        lines: Lines::new(src),
        syms: Vec::new(),
        uses: Vec::new(),
        top: Vec::new(),
        scopes: Vec::new(),
        parents: Vec::new(),
        next_scope: 1,
        links: Vec::new(),
        shorthand: Vec::new(),
    };
    let all = (0, src.len() as u32);
    // the top level: its lets and fns first (visible in every function)
    let mut ids: Vec<Option<usize>> = Vec::with_capacity(prog.body.len());
    for s in &prog.body {
        let mut new: Vec<usize> = Vec::new();
        let id = match &s.kind {
            StmtKind::Let { pat: Pattern { kind: PatKind::Name(name), .. }, init } => {
                let id = r.add(name, SymKind::Global, s.span, all, s.span.end, 0, None);
                r.let_info(id, init.as_ref());
                new.push(id);
                Some(id)
            }
            StmtKind::Let { pat, .. } => {
                // (S3) every name of a pattern is a global
                let mut names = Vec::new();
                pat.names(&mut names);
                for n in names {
                    let id = r.add(n, SymKind::Global, s.span, all, s.span.end, 0, None);
                    new.push(id);
                }
                None
            }
            StmtKind::Fn(f) => f.name.as_ref().map(|n| {
                let id = r.add_fn(n, f, all, 0);
                new.push(id);
                id
            }),
            _ => None,
        };
        // (a name declared twice: the first one, as check)
        for id in new {
            if !r.top.iter().any(|&t| r.syms[t].name == r.syms[id].name) {
                r.top.push(id);
            }
        }
        ids.push(id);
    }
    for (s, id) in prog.body.iter().zip(&ids) {
        match (&s.kind, *id) {
            (StmtKind::Let { init: Some(e), .. }, Some(id)) => {
                let fnlit = r.syms[id].ty == "fn";
                r.parents.push(Some(id).filter(|_| fnlit));
                r.expr(e);
                r.parents.pop();
            }
            (StmtKind::Fn(f), Some(id)) => r.func(f, Some(id)),
            _ => r.stmt(s),
        }
    }
    // (S3) the named arguments of calls of the script's fns: uses of their parameters
    for (sp, name, f) in core::mem::take(&mut r.links) {
        // (its own parameters, not those of a fn literal inside it: they end where it ends)
        let end = r.syms[f].decl.end;
        let p = r.syms.iter().position(|x| x.kind == SymKind::Param && x.parent == Some(f) && x.name == name && x.scope.1 == end);
        r.uses.push(Use { span: sp, sym: p, write: false });
    }
    r.uses.sort_by_key(|u| u.span.start);
    r.shorthand.sort_unstable();
    Index { symbols: r.syms, uses: r.uses, shorthand: r.shorthand }
}

impl Res<'_> {
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, n: &Name, kind: SymKind, decl: Span, scope: (u32, u32), from: u32, scope_id: u32, params: Option<&[Pattern]>) -> usize {
        let doc = if matches!(kind, SymKind::Global | SymKind::Fn | SymKind::Local) { self.doc_before(decl.line) } else { String::new() };
        let parent = self.parents.last().copied().flatten();
        self.syms.push(Symbol {
            name: n.text.clone(),
            kind,
            span: n.span,
            decl,
            scope,
            from,
            params: params.map_or(Vec::new(), |p| p.iter().map(param_label).collect()),
            doc,
            parent,
            import: None,
            ty: "",
            scope_id,
        });
        self.syms.len() - 1
    }

    fn add_fn(&mut self, n: &Name, f: &FnDef, scope: (u32, u32), scope_id: u32) -> usize {
        let id = self.add(n, SymKind::Fn, f.span, scope, scope.0, scope_id, Some(&f.params));
        self.syms[id].ty = "fn";
        id
    }

    /// the type, the parameters of a fn literal and the path of an import of a let's value
    fn let_info(&mut self, id: usize, init: Option<&Expr>) {
        let Some(e) = init else { return };
        let s = &mut self.syms[id];
        s.ty = type_of(e);
        match &e.kind {
            ExprKind::Fn(f) => s.params = f.params.iter().map(param_label).collect(),
            ExprKind::Call(f, args) if matches!(&f.kind, ExprKind::Var(n) if n == "import") => {
                if let [Expr { kind: ExprKind::Str(p), .. }] = args.as_slice() {
                    s.import = Some(p.clone());
                }
            }
            _ => {}
        }
    }

    /// the /// lines right above line `line`
    fn doc_before(&self, line: u32) -> String {
        let mut v: Vec<&str> = Vec::new();
        let mut l = line;
        while l > 1 {
            l -= 1;
            let t = self.lines.text(self.src, l).trim_start();
            match t.strip_prefix("///") {
                Some(rest) if !rest.starts_with('/') => v.push(rest.strip_prefix(' ').unwrap_or(rest).trim_end()),
                _ => break,
            }
        }
        v.reverse();
        v.join("\n")
    }

    fn scope_push(&mut self) -> u32 {
        let id = self.next_scope;
        self.next_scope += 1;
        self.scopes.push((id, Vec::new()));
        id
    }

    fn declare(&mut self, id: usize) {
        if let Some(s) = self.scopes.last_mut() {
            s.1.push(id);
        }
    }

    fn lookup(&self, name: &str) -> Option<usize> {
        for (_, s) in self.scopes.iter().rev() {
            if let Some(&id) = s.iter().find(|&&id| self.syms[id].name == name) {
                return Some(id);
            }
        }
        self.top.iter().copied().find(|&id| self.syms[id].name == name)
    }

    fn use_name(&mut self, name: &str, sp: Span, write: bool) {
        let sym = self.lookup(name);
        let a = sp.start as usize;
        let span = if self.src[a..].starts_with(name) {
            Span { end: sp.start + name.len() as u32, ..sp }
        } else {
            // a name in parentheses: its span holds them
            let t = self.src.get(a..sp.end as usize).unwrap_or("");
            let lead = t.len() - t.trim_start_matches(|c: char| c == '(' || c.is_whitespace()).len();
            let at = if t[lead..].starts_with(name) { lead } else { t.find(name).unwrap_or(0) };
            let s = (a + at) as u32;
            self.lines.span(self.src, s, s + name.len() as u32)
        };
        self.uses.push(Use { span, sym, write });
    }

    /// (S3) the names of a pattern as symbols of `kind` visible in `range` (from its start, or
    /// from `from`), declared in the innermost scope; its defaults read before the names after
    /// them, an assignment's targets written
    fn bind(&mut self, p: &Pattern, kind: SymKind, decl: Option<Span>, range: (u32, u32), from: Option<u32>, sid: u32) {
        match &p.kind {
            PatKind::Name(n) => {
                let id = self.add(n, kind, decl.unwrap_or(n.span), range, from.unwrap_or(range.0), sid, None);
                self.declare(id);
            }
            PatKind::List(items, rest) => {
                for x in items {
                    self.bind(x, kind, decl, range, from, sid);
                }
                if let Some(r) = rest {
                    self.bind(r, kind, decl, range, from, sid);
                }
            }
            PatKind::Map(entries) => {
                for (k, x) in entries {
                    if matches!(&x.inner().kind, PatKind::Name(n) if n.span == k.span) {
                        self.shorthand.push(k.span.start);
                    }
                    self.bind(x, kind, decl, range, from, sid);
                }
            }
            PatKind::Default(x, e) => {
                self.expr(e);
                self.bind(x, kind, decl, range, from, sid);
            }
            PatKind::Target(t) => match &t.kind {
                ExprKind::Var(n) => self.use_name(n, t.span, true),
                _ => self.expr(t),
            },
        }
    }

    fn block(&mut self, b: &Block) {
        let sid = self.scope_push();
        let range = (b.span.start, b.span.end);
        for s in &b.stmts {
            if let StmtKind::Fn(f) = &s.kind {
                if let Some(n) = &f.name {
                    let id = self.add_fn(n, f, range, sid);
                    self.declare(id);
                }
            }
        }
        for s in &b.stmts {
            match &s.kind {
                StmtKind::Fn(f) if f.name.is_some() => {
                    let id = self.scopes.last().and_then(|(_, v)| v.iter().copied().find(|&i| self.syms[i].span == f.name.as_ref().map_or(Span::default(), |n| n.span)));
                    self.func(f, id);
                }
                StmtKind::Let { pat: Pattern { kind: PatKind::Name(name), .. }, init } => {
                    let id = self.add(name, SymKind::Local, s.span, (s.span.end, b.span.end), s.span.end, sid, None);
                    self.let_info(id, init.as_ref());
                    if let Some(e) = init {
                        let around = self.parents.last().copied().flatten();
                        let p = if self.syms[id].ty == "fn" { Some(id) } else { around };
                        self.parents.push(p);
                        self.expr(e);
                        self.parents.pop();
                    }
                    self.declare(id);
                }
                StmtKind::Let { pat, init } => {
                    if let Some(e) = init {
                        self.expr(e);
                    }
                    self.bind(pat, SymKind::Local, Some(s.span), (s.span.end, b.span.end), None, sid);
                }
                _ => self.stmt(s),
            }
        }
        self.scopes.pop();
    }

    fn func(&mut self, f: &FnDef, sym: Option<usize>) {
        let around = self.parents.last().copied().flatten();
        self.parents.push(sym.or(around));
        let sid = self.scope_push();
        // (S3) a default sees the parameters before it
        for p in &f.params {
            self.bind(p, SymKind::Param, None, (f.span.start, f.span.end), Some(f.span.start), sid);
        }
        self.block(&f.body);
        self.scopes.pop();
        self.parents.pop();
    }

    /// the variables of a for loop or a comprehension's for clause, visible in range
    fn for_vars(&mut self, a: &Pattern, b: Option<&Pattern>, decl: Span, range: (u32, u32), sid: u32) {
        for p in [Some(a), b].into_iter().flatten() {
            self.bind(p, SymKind::Loop, Some(decl), range, None, sid);
        }
    }

    fn src(&mut self, src: &ForSrc) {
        match src {
            ForSrc::Each(e) => self.expr(e),
            ForSrc::Range(lo, hi) => {
                self.expr(lo);
                self.expr(hi);
            }
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let { pat, init } => {
                // (a let outside a block: the top level, declared by index_of)
                if let Some(e) = init {
                    self.expr(e);
                }
                self.top_pattern(pat);
            }
            StmtKind::Fn(f) => self.func(f, None),
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
            StmtKind::For { a, b, src, body } => {
                self.src(src);
                let sid = self.scope_push();
                let range = (a.span.start, body.span.end);
                self.for_vars(a, b.as_ref(), s.span, range, sid);
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Return(v) => {
                if let Some(e) = v {
                    self.expr(e);
                }
            }
            StmtKind::Try { body, var, catch } => {
                self.block(body);
                let sid = self.scope_push();
                if let Some(v) = var {
                    let id = self.add(v, SymKind::Catch, v.span, (v.span.start, catch.span.end), v.span.start, sid, None);
                    self.declare(id);
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
                let sid = self.scopes.last().map_or(0, |s| s.0);
                self.bind(pat, SymKind::Local, None, (0, 0), None, sid);
            }
            StmtKind::Block(b) => self.block(b),
        }
    }

    /// (S3) a top-level let's pattern: its names are declared (index_of), its defaults read, its
    /// shorthands noted
    fn top_pattern(&mut self, p: &Pattern) {
        match &p.kind {
            PatKind::Name(_) | PatKind::Target(_) => {}
            PatKind::List(items, rest) => {
                for x in items {
                    self.top_pattern(x);
                }
                if let Some(r) = rest {
                    self.top_pattern(r);
                }
            }
            PatKind::Map(entries) => {
                for (k, x) in entries {
                    if matches!(&x.inner().kind, PatKind::Name(n) if n.span == k.span) {
                        self.shorthand.push(k.span.start);
                    }
                    self.top_pattern(x);
                }
            }
            PatKind::Default(x, e) => {
                self.expr(e);
                self.top_pattern(x);
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
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
                    if k.span == v.span {
                        // (S3) {a}: the key is the name's use
                        self.shorthand.push(v.span.start);
                    } else {
                        self.expr(k);
                    }
                    self.expr(v);
                }
            }
            ExprKind::Fn(f) => self.func(f, None),
            ExprKind::Unary(_, a) | ExprKind::Field(a, _) => self.expr(a),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Call(f, args) | ExprKind::Method(f, _, args) => {
                self.expr(f);
                // (S3) a named argument of a call of a fn of the script names its parameter
                let callee = match (&e.kind, &f.kind) {
                    (ExprKind::Call(..), ExprKind::Var(n)) => self.lookup(n).filter(|&i| self.syms[i].kind == SymKind::Fn || self.syms[i].ty == "fn"),
                    _ => None,
                };
                for x in args {
                    if let ExprKind::Named(n, _) = &x.kind {
                        match callee {
                            Some(c) => self.links.push((n.span, n.text.clone(), c)),
                            None => self.uses.push(Use { span: n.span, sym: None, write: false }),
                        }
                    }
                    self.expr(x);
                }
            }
            ExprKind::Cond(c, a, b) => {
                self.expr(c);
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Named(_, v) => self.expr(v),
            ExprKind::Comp(x, clauses) => {
                // (S3) a scope per for clause; its variables are visible in the whole list
                let n = self.scopes.len();
                for c in clauses {
                    match c {
                        Clause::For { a, b, src } => {
                            self.src(src);
                            let sid = self.scope_push();
                            self.for_vars(a, b.as_ref(), a.span, (e.span.start, e.span.end), sid);
                        }
                        Clause::If(c) => self.expr(c),
                    }
                }
                self.expr(x);
                self.scopes.truncate(n);
            }
        }
    }
}

/// a parameter as the signature shows it: "a", "b = 2", "[x, y]" (the default's text when it
/// is short and simple, else "…")
fn param_label(p: &Pattern) -> String {
    match (&p.kind, p.default()) {
        (PatKind::Default(x, _), Some(d)) => match short_text(d) {
            Some(t) => format!("{} = {}", x.label(), t),
            None => format!("{} = …", x.label()),
        },
        _ => p.label(),
    }
}

/// a literal or a name as written (a parameter's default in a signature)
fn short_text(e: &Expr) -> Option<String> {
    Some(match &e.kind {
        ExprKind::Int(v) => format!("{}", v),
        ExprKind::Float(v) => format!("{:?}", v),
        ExprKind::Bool(b) => String::from(if *b { "true" } else { "false" }),
        ExprKind::Nil => String::from("nil"),
        ExprKind::Str(s) if s.chars().count() <= 16 && !s.contains(['"', '\\', '\n']) => format!("\"{}\"", s),
        ExprKind::Var(n) => n.clone(),
        ExprKind::Unary(UnOp::Neg, a) => format!("-{}", short_text(a)?),
        ExprKind::List(v) if v.is_empty() => String::from("[]"),
        ExprKind::Map(v) if v.is_empty() => String::from("{}"),
        _ => return None,
    })
}

// the type of a value as far as a literal or a call tells it
fn type_of(e: &Expr) -> &'static str {
    match &e.kind {
        ExprKind::Str(_) | ExprKind::Interp(_) => "str",
        ExprKind::List(_) => "list",
        ExprKind::Map(_) => "map",
        ExprKind::Fn(_) => "fn",
        ExprKind::Call(f, _) => match &f.kind {
            ExprKind::Var(n) if n == "bytes" => "bytes",
            ExprKind::Var(n) if n == "str" => "str",
            ExprKind::Field(m, n) if matches!(&m.kind, ExprKind::Var(x) if x == "json") && n.text == "encode" => "str",
            _ => "",
        },
        ExprKind::Method(_, n, _) => match n.text.as_str() {
            "split" | "chars" | "keys" | "values" | "entries" | "map" | "filter" => "list",
            "upper" | "lower" | "trim" | "trim_start" | "trim_end" | "replace" | "repeat" | "join" | "format" | "hex" => "str",
            _ => "",
        },
        _ => "",
    }
}

/// the declaration line of a symbol: "fn greet(name)", "let hits", "let f = fn(a)", "name"
fn sig_of(s: &Symbol) -> String {
    match s.kind {
        SymKind::Fn => format!("fn {}({})", s.name, s.params.join(", ")),
        SymKind::Global | SymKind::Local if s.ty == "fn" => format!("let {} = fn({})", s.name, s.params.join(", ")),
        SymKind::Global | SymKind::Local => match &s.import {
            Some(p) => format!("let {} = import(\"{}\")", s.name, p),
            None => format!("let {}", s.name),
        },
        SymKind::Loop => format!("for {}", s.name),
        SymKind::Catch => format!("catch {}", s.name),
        SymKind::Param => String::from(s.name.as_str()),
    }
}

/// the kind line of a symbol's hover: "Funktion", "Parameter von greet", "lokale Variable in
/// handle"
fn kind_of(idx: &Index, s: &Symbol) -> String {
    let w = if s.import.is_some() { t!("Modul") } else { s.kind.word() };
    match s.parent.map(|p| idx.symbols[p].name.as_str()) {
        Some(p) if s.kind == SymKind::Param => tf!("{} von {}", w, p),
        Some(p) => format!("{} in {}", w, p),
        None => String::from(w),
    }
}

// ---------------------------------------------------------------- definition, references, rename

/// the members of an imported module: its top-level symbols (lets and fns) and its index
fn module(cx: &Context, path: &str) -> Option<(Index, String)> {
    let load = cx.load?;
    let text = load(path)?;
    Some((index(&text), text))
}

/// the receiver of a member at word token k (`x.name`): the token before the dot
fn receiver(toks: &[Tok], k: usize) -> Option<usize> {
    let d = prev_sig(toks, k)?;
    if toks[d].kind != TokKind::Dot {
        return None;
    }
    prev_sig(toks, d)
}

/// see the module comment
pub fn definition<'a>(src: &str, pos: u32, cx: impl Into<Context<'a>>) -> Option<Location> {
    let cx: Context = cx.into();
    let cx = &cx;
    let (toks, _) = lex::lex(src);
    let k = word_at(&toks, pos)?;
    let idx = index(src);
    if let Some(r) = receiver(&toks, k) {
        if toks[r].kind != TokKind::Ident {
            return None;
        }
        let sym = idx.sym_of(text_of(src, &toks[r]), toks[r].start)?;
        let path = idx.symbols[sym].import.clone()?;
        let (mi, _) = module(cx, &path)?;
        let name = text_of(src, &toks[k]);
        let m = mi.symbols.iter().find(|s| s.name == name && matches!(s.kind, SymKind::Global | SymKind::Fn) && s.parent.is_none())?;
        return Some(Location { path: Some(path), span: m.span });
    }
    let sym = idx.sym_of(text_of(src, &toks[k]), toks[k].start)?;
    Some(Location { path: None, span: idx.symbols[sym].span })
}

/// see the module comment
pub fn references(src: &str, pos: u32) -> Vec<Span> {
    let idx = index(src);
    let Some(sym) = idx.at(pos) else { return Vec::new() };
    refs_of(&idx, sym)
}

fn refs_of(idx: &Index, sym: usize) -> Vec<Span> {
    let mut v: Vec<Span> = alloc::vec![idx.symbols[sym].span];
    v.extend(idx.uses.iter().filter(|u| u.sym == Some(sym)).map(|u| u.span));
    v.sort_by_key(|s| s.start);
    v.dedup_by_key(|s| s.start);
    v
}

/// see the module comment
pub fn rename(src: &str, pos: u32, new: &str) -> Result<Vec<Edit>, String> {
    if TokKind::keyword(new).is_some() {
        return Err(tf!("{} ist ein Schlüsselwort und kann kein Name sein", new));
    }
    if !is_name(new) {
        return Err(tf!("„{}“ ist kein gültiger Name (Buchstaben, Ziffern und _, nicht vorn eine Ziffer)", new));
    }
    let prog = match crate::parse("", src) {
        Ok(p) => p,
        Err(d) => return Err(tf!("Umbenennen geht erst ohne Syntaxfehler (Zeile {}: {})", d[0].line, d[0].msg)),
    };
    let idx = index_of(&prog, src);
    let Some(sym) = idx.at(pos) else { return Err(String::from(t!("Hier steht kein Name, der in diesem Skript deklariert ist"))) };
    let s = &idx.symbols[sym];
    if s.name == new {
        return Ok(Vec::new());
    }
    // the same scope
    if let Some(o) = idx.symbols.iter().find(|o| o.name == new && o.scope_id == s.scope_id) {
        return Err(tf!("{} ist hier schon deklariert (Zeile {})", new, o.span.line));
    }
    let spans = refs_of(&idx, sym);
    // (S3) a shorthand {a} keeps its key: {a: new}
    let short = |at: u32| idx.shorthand.binary_search(&at).is_ok();
    let edits: Vec<Edit> = spans
        .iter()
        .map(|sp| {
            let text = if short(sp.start) { format!("{}: {}", s.name, new) } else { String::from(new) };
            Edit { start: sp.start, end: sp.end, text }
        })
        .collect();
    // the new text must mean the same: every renamed name the symbol, every other use what it
    // meant before (a declaration of `new` closer to a use, or a use of `new` that the renamed
    // one would hide, clash)
    let after = apply(src, &edits);
    // where the name at byte b is afterwards (after a shorthand's new key)
    let moved = |b: u32| -> u32 {
        let mut at = b as i64;
        for e in &edits {
            if e.start < b {
                at += e.text.len() as i64 - (e.end - e.start) as i64;
            } else if e.start == b && short(b) {
                at += s.name.len() as i64 + 2;
            }
        }
        at as u32
    };
    let (prog2, errs) = crate::parse_partial("", &after);
    if !errs.is_empty() {
        return Err(tf!("{} lässt sich hier nicht verwenden", new));
    }
    let idx2 = index_of(&prog2, &after);
    let decl_of = |ix: &Index, u: Option<usize>| u.map(|i| ix.symbols[i].span.start);
    let target = moved(s.span.start);
    for u in &idx.uses {
        let Some(u2) = idx2.use_from(moved(u.span.start)) else { continue };
        let want = if u.sym == Some(sym) { Some(target) } else { decl_of(&idx, u.sym).map(moved) };
        let got = decl_of(&idx2, u2.sym);
        if got != want {
            let line = if u.sym == Some(sym) { got.and_then(|g| idx2.symbols.iter().find(|x| x.span.start == g)).map_or(u.span.line, |x| x.span.line) } else { u.span.line };
            return Err(tf!("Der Name {} ist hier schon vergeben (Zeile {})", new, line));
        }
    }
    let before = crate::check::resolve(&prog, &[]).iter().filter(|x| x.error).count();
    let now = crate::check::resolve(&prog2, &[]).iter().filter(|x| x.error).count();
    if now > before {
        return Err(tf!("{} ist in diesem Gültigkeitsbereich schon deklariert", new));
    }
    Ok(edits)
}

/// the source with edits applied (sorted, not overlapping)
pub fn apply(src: &str, edits: &[Edit]) -> String {
    let mut out = String::with_capacity(src.len() + 16);
    let mut at = 0usize;
    for e in edits {
        let (a, b) = (e.start as usize, e.end as usize);
        if a < at || b > src.len() {
            continue;
        }
        out.push_str(&src[at..a]);
        out.push_str(&e.text);
        at = b;
    }
    out.push_str(&src[at..]);
    out
}

// ---------------------------------------------------------------- completion

const STMT_WORDS: &[&str] = &["let", "fn", "if", "while", "for", "return", "break", "continue", "try", "throw"];
const EXPR_WORDS: &[&str] = &["true", "false", "nil", "fn", "await"];

/// the built-ins that are functions (and not only modules)
const BUILTINS: &[&str] = &["print", "len", "type", "str", "int", "float", "bytes"];

/// where the word at pos is: (start, end of the word, the full token list's index of the
/// token before it); None inside a comment or the text of a string
fn word_place(src: &str, toks: &[Tok], pos: u32) -> Option<(u32, u32, Option<usize>)> {
    if let Some(k) = tok_at(toks, pos) {
        let t = toks[k];
        let inside = t.start < pos && pos < t.end;
        match t.kind {
            TokKind::Comment if t.start < pos => return None,
            TokKind::Str | TokKind::StrHead | TokKind::StrMid | TokKind::StrTail if inside => return None,
            // an unterminated string at its end
            TokKind::Str | TokKind::StrTail if t.end == pos && !closed(text_of(src, &t)) => return None,
            _ => {}
        }
    }
    match word_at(toks, pos) {
        Some(k) => Some((toks[k].start, toks[k].end, prev_sig(toks, k))),
        None => {
            let k = toks.partition_point(|t| t.end <= pos);
            Some((pos, pos, prev_sig(toks, k)))
        }
    }
}

/// a string token ends with its quote
fn closed(t: &str) -> bool {
    t.len() >= 2 && t.ends_with('"') && t[..t.len() - 1].bytes().rev().take_while(|&c| c == b'\\').count() % 2 == 0
}

/// rank of an item's source for sorting: 0 locals, 1 the script's globals, 2 host, 3 library,
/// 4 keywords
struct Cand {
    item: Item,
    rank: u8,
}

fn host_detail(h: &HostItem) -> String {
    match h.params {
        Some(p) => format!("{}({})", h.name, p),
        None => String::from(h.name),
    }
}

fn lib_detail(d: &lsdoc::Doc, as_method: bool) -> String {
    let (ty, short) = d.name.split_once('.').unwrap_or(("", d.name));
    match d.params {
        None => String::from(d.name),
        Some(p) if as_method && !d.recv.is_empty() => format!("{}.{}({})", d.recv, short, p),
        Some(p) if !d.recv.is_empty() => {
            let all = if p.is_empty() { String::from(d.recv) } else { format!("{}, {}", d.recv, p) };
            format!("{}.{}({})", ty, short, all)
        }
        Some(p) => format!("{}({})", d.name, p),
    }
}

/// see the module comment
pub fn complete<'a>(src: &str, pos: u32, cx: impl Into<Context<'a>>) -> Completion {
    let cx: Context = cx.into();
    let cx = &cx;
    let pos = clamp_pos(src, pos);
    let (toks, _) = lex::lex(src);
    let Some((start, end, prev)) = word_place(src, &toks, pos) else { return Completion { start: pos, end: pos, items: Vec::new() } };
    let prefix = &src[start as usize..pos as usize];
    let (prog, _) = crate::parse_partial("", src);
    let idx = index_of(&prog, src);
    let types = cx.api.map(|api| lsapi::infer(&prog, &idx, api));
    let mut c: Vec<Cand> = Vec::new();
    let member = prev.filter(|&p| toks[p].kind == TokKind::Dot);
    match member {
        Some(d) => {
            let r = prev_sig(&toks, d);
            // (S2) a receiver whose type the host's description tells
            let known = match (&types, r) {
                (Some(t), Some(r)) => t.chain(src, &toks, r),
                _ => None,
            };
            match known {
                Some(k) => typed_members(types.as_ref(), k, cx, &mut c),
                None => members(src, &toks, &idx, r, cx, &mut c),
            }
        }
        None => {
            // (S3) at an argument's start: the parameters still to name ("width:")
            param_items(src, pos, start, cx, &mut c);
            let mut bodies = Vec::new();
            walk_fns(&prog.body, &mut |f: &FnDef| bodies.push((f.body.span.start, f.body.span.end)));
            names(src, &toks, &idx, &bodies, start, prev, cx, &mut c)
        }
    }
    // filter and rank
    let low = prefix.to_lowercase();
    let mut c: Vec<(u8, Cand)> = c
        .into_iter()
        .filter_map(|x| {
            if x.item.label.starts_with(prefix) {
                Some((0, x))
            } else if x.item.label.to_lowercase().starts_with(&low) {
                Some((1, x))
            } else {
                None
            }
        })
        .collect();
    c.sort_by(|a, b| (a.0, a.1.rank).cmp(&(b.0, b.1.rank)).then_with(|| a.1.item.label.cmp(&b.1.item.label)));
    let mut items: Vec<Item> = Vec::with_capacity(c.len());
    for (_, x) in c {
        if !items.iter().any(|i| i.label == x.item.label) {
            items.push(x.item);
        }
    }
    Completion { start, end, items }
}

fn clamp_pos(src: &str, pos: u32) -> u32 {
    let mut p = (pos as usize).min(src.len());
    while !src.is_char_boundary(p) {
        p -= 1;
    }
    p as u32
}

/// the names visible at byte pos (the innermost per name); bodies: the fn bodies (inside one,
/// top-level lets are readable wherever they are declared)
fn visible(idx: &Index, bodies: &[(u32, u32)], pos: u32) -> Vec<usize> {
    let in_fn = bodies.iter().any(|&(a, b)| a < pos && pos < b);
    let mut best: Vec<usize> = Vec::new();
    for (i, s) in idx.symbols.iter().enumerate() {
        let inside = s.scope.0 <= pos && pos <= s.scope.1;
        if !inside || (s.kind == SymKind::Global && !in_fn && pos < s.from) || (s.span.start <= pos && pos <= s.span.end) {
            continue;
        }
        match best.iter().position(|&b| idx.symbols[b].name == s.name) {
            Some(k) if idx.symbols[best[k]].scope.0 < s.scope.0 => best[k] = i,
            Some(_) => {}
            None => best.push(i),
        }
    }
    best
}

/// (S3) the arguments before the word at `start` inside the parentheses of a call, when the
/// word begins an argument (after `(` or a comma): (the number by position, the names given)
fn args_before(src: &str, start: u32) -> Option<(usize, Vec<String>)> {
    let (all, _) = lex::lex(src);
    let toks = lex::significant(&all, src);
    let k = toks.partition_point(|t| t.end <= start);
    let p = toks[..k].iter().rposition(|t| t.kind != TokKind::Eof)?;
    if !matches!(toks[p].kind, TokKind::LParen | TokKind::Comma) {
        return None;
    }
    // back to the call's ( : the starts of the arguments before the one at `start`
    let (mut depth, mut starts) = (0i32, Vec::new());
    let mut open = (toks[p].kind == TokKind::LParen).then_some(p);
    if open.is_none() {
        for j in (0..p).rev() {
            match toks[j].kind {
                TokKind::RParen | TokKind::RBracket | TokKind::RBrace | TokKind::StrTail => depth += 1,
                TokKind::LParen if depth == 0 => {
                    open = Some(j);
                    break;
                }
                TokKind::LBracket | TokKind::LBrace | TokKind::StrHead | TokKind::StrMid if depth == 0 => return None,
                TokKind::LParen | TokKind::LBracket | TokKind::LBrace | TokKind::StrHead => depth -= 1,
                TokKind::Comma if depth == 0 => starts.push(j + 1),
                _ => {}
            }
        }
    }
    let o = open?;
    o.checked_sub(1).filter(|&j| toks[j].kind == TokKind::Ident)?;
    if p != o {
        starts.push(o + 1);
    }
    let (mut positional, mut names) = (0usize, Vec::new());
    for s in starts {
        match (toks.get(s), toks.get(s + 1)) {
            (Some(a), Some(c)) if a.kind == TokKind::Ident && c.kind == TokKind::Colon => names.push(String::from(text_of(src, a))),
            (Some(a), _) if a.kind != TokKind::Comma => positional += 1,
            _ => {}
        }
    }
    Some((positional, names))
}

/// (S3) at an argument's start in a call: the parameters of its forms that no argument fills
/// yet, as "width:" (a script fn's, the host's description's, the library's)
fn param_items(src: &str, pos: u32, start: u32, cx: &Context, c: &mut Vec<Cand>) {
    let Some((positional, given)) = args_before(src, start) else { return };
    let Some(sig) = signature(src, pos, *cx) else { return };
    let mut seen: Vec<String> = Vec::new();
    for f in &sig.forms {
        for (i, &(a, b)) in f.params.iter().enumerate() {
            let part = &f.label[a as usize..b as usize];
            let n = part_name(part);
            if n.is_empty() || part.trim_start().starts_with("...") || i < positional || given.iter().any(|g| g == n) || seen.iter().any(|s| s == n) {
                continue;
            }
            seen.push(String::from(n));
            c.push(Cand { item: Item { label: format!("{}:", n), kind: ItemKind::Param, detail: String::from(part), doc: f.doc.clone() }, rank: 0 });
        }
    }
}

/// every FnDef in statements (declarations and literals, nested ones too)
fn walk_fns(stmts: &[Stmt], f: &mut dyn FnMut(&FnDef)) {
    visit(stmts, &mut |n| match n {
        Node::Stmt(Stmt { kind: StmtKind::Fn(d), .. }) => f(d),
        Node::Expr(Expr { kind: ExprKind::Fn(d), .. }) => f(d),
        _ => {}
    });
}

/// the names at a word that is no member: symbols in scope, the host's, the library's,
/// keywords
#[allow(clippy::too_many_arguments)]
fn names(src: &str, toks: &[Tok], idx: &Index, bodies: &[(u32, u32)], start: u32, prev: Option<usize>, cx: &Context, c: &mut Vec<Cand>) {
    let mut taken: Vec<String> = Vec::new();
    for i in visible(idx, bodies, start) {
        let s = &idx.symbols[i];
        let kind = match s.kind {
            SymKind::Fn => ItemKind::Function,
            SymKind::Param => ItemKind::Param,
            _ if s.import.is_some() => ItemKind::Module,
            _ if s.ty == "fn" => ItemKind::Function,
            _ => ItemKind::Variable,
        };
        let rank = if matches!(s.kind, SymKind::Global | SymKind::Fn) && s.parent.is_none() && s.scope.0 == 0 { 1 } else { 0 };
        taken.push(s.name.clone());
        c.push(Cand { item: Item { label: s.name.clone(), kind, detail: sig_of(s), doc: s.doc.clone() }, rank });
    }
    // (S2) the host's description: its global names and the namespaces of dotted ones
    if let Some(api) = cx.api {
        for g in &api.globals {
            let root = g.name.split('.').next().unwrap_or(&g.name);
            if taken.iter().any(|t| t == root) {
                continue;
            }
            if root == g.name {
                c.push(Cand { item: global_item(api, root, g), rank: 2 });
                taken.push(String::from(root));
            } else if api.global(root).is_none() {
                c.push(Cand { item: Item { label: String::from(root), kind: ItemKind::Module, detail: String::from(root), doc: String::new() }, rank: 2 });
                taken.push(String::from(root));
            }
        }
    }
    // the host: plain names and the namespaces of dotted ones
    for h in cx.host {
        let root = h.name.split('.').next().unwrap_or(h.name);
        if taken.iter().any(|t| t == root) {
            continue;
        }
        if root == h.name {
            let kind = if h.params.is_some() { ItemKind::Function } else if cx.host.iter().any(|x| x.name.len() > root.len() && x.name.starts_with(root) && x.name.as_bytes()[root.len()] == b'.') { ItemKind::Module } else { ItemKind::Value };
            c.push(Cand { item: Item { label: String::from(root), kind, detail: host_detail(h), doc: String::from(h.doc) }, rank: 2 });
            taken.push(String::from(root));
        } else if !cx.host.iter().any(|x| x.name == root) {
            c.push(Cand { item: Item { label: String::from(root), kind: ItemKind::Module, detail: String::from(root), doc: String::new() }, rank: 2 });
            taken.push(String::from(root));
        }
    }
    // the built-ins and the library's modules
    for &b in BUILTINS.iter().chain(lsdoc::MODULES) {
        if taken.iter().any(|t| t == b) {
            continue;
        }
        taken.push(String::from(b));
        let (kind, detail, doc) = match lsdoc::doc(b) {
            Some(d) => (ItemKind::Function, lib_detail(d, false), String::from(d.doc())),
            None => (ItemKind::Module, String::from(b), String::from(t!("Modul der Standardbibliothek"))),
        };
        c.push(Cand { item: Item { label: String::from(b), kind, detail, doc }, rank: 3 });
    }
    // keywords: statements where one may start, values everywhere
    let stmt = match prev {
        None => true,
        Some(p) => matches!(toks[p].kind, TokKind::Semi | TokKind::LBrace | TokKind::RBrace) || (break_between(toks, src, p, toks.partition_point(|t| t.end <= start)) && ends_expr(toks[p].kind)),
    };
    let mut words: Vec<&str> = EXPR_WORDS.to_vec();
    if stmt {
        words.extend_from_slice(STMT_WORDS);
    }
    if prev.is_some_and(|p| toks[p].kind == TokKind::RBrace) {
        words.extend_from_slice(&["else", "catch"]);
    }
    // for x | / for k, v |: in
    let back = |k: Option<usize>, want: TokKind| k.filter(|&j| toks[j].kind == want).and_then(|j| prev_sig(toks, j));
    if let Some(b) = back(prev, TokKind::Ident) {
        let b2 = if toks[b].kind == TokKind::Comma { back(prev_sig(toks, b), TokKind::Ident) } else { Some(b) };
        if b2.is_some_and(|j| toks[j].kind == TokKind::For) {
            words.push("in");
        }
    }
    for w in words {
        if !c.iter().any(|x| x.item.label == w) {
            c.push(Cand { item: Item { label: String::from(w), kind: ItemKind::Keyword, detail: String::from(t!("Schlüsselwort")), doc: String::new() }, rank: 4 });
        }
    }
}

/// a token that may end an expression (so a line break after it ends the statement)
fn ends_expr(k: TokKind) -> bool {
    matches!(k, TokKind::Ident | TokKind::Int | TokKind::Float | TokKind::Str | TokKind::StrTail | TokKind::True | TokKind::False | TokKind::Nil | TokKind::RParen | TokKind::RBracket | TokKind::RBrace | TokKind::Break | TokKind::Continue | TokKind::Return)
}

/// what the receiver token r of a member is: an imported module's path, a library module or
/// host namespace's name, or a type ("str" ..; "" not known)
enum Recv {
    Import(String),
    Name(String),
    Type(&'static str),
}

fn recv_of(src: &str, toks: &[Tok], idx: &Index, r: Option<usize>) -> Recv {
    let Some(r) = r else { return Recv::Type("") };
    let t = toks[r];
    match t.kind {
        TokKind::Ident => match idx.sym_of(text_of(src, &t), t.start) {
            Some(s) => match &idx.symbols[s].import {
                Some(p) => Recv::Import(p.clone()),
                None => Recv::Type(idx.symbols[s].ty),
            },
            None => Recv::Name(String::from(text_of(src, &t))),
        },
        TokKind::Str | TokKind::StrTail => Recv::Type("str"),
        _ => Recv::Type(""),
    }
}

/// the library's methods of type ty as completion items (a name on several types once, with
/// their receivers in the detail)
fn lib_methods(ty: &str, c: &mut Vec<Cand>) {
    for d in lsdoc::methods(ty) {
        let short = d.name.split_once('.').map_or(d.name, |x| x.1);
        if let Some(x) = c.iter_mut().find(|x| x.item.label == short && x.item.kind == ItemKind::Method) {
            // the same name on several types: their receivers in the detail
            let more = format!(" · {}", lib_detail(d, true));
            if !x.item.detail.contains(&more) {
                x.item.detail.push_str(&more);
            }
            continue;
        }
        c.push(Cand { item: Item { label: String::from(short), kind: ItemKind::Method, detail: lib_detail(d, true), doc: String::from(d.doc()) }, rank: 3 });
    }
}

/// (S2) the completion item of a global of the host's description ("vec(x: float): vec")
fn global_item(api: &Api, label: &str, g: &crate::api::Global) -> Item {
    let (kind, detail) = match &g.kind {
        GlobalKind::Func(s) => (ItemKind::Function, s.label(&g.name)),
        GlobalKind::Namespace => (ItemKind::Module, g.name.clone()),
        GlobalKind::Value(t) => {
            let kind = if api.is_namespace(&g.name) { ItemKind::Module } else { ItemKind::Value };
            (kind, if t.is_empty() { g.name.clone() } else { format!("{}: {}", g.name, t) })
        }
    };
    Item { label: String::from(label), kind, detail, doc: g.doc.clone() }
}

/// (S2) the members of a receiver whose type the description tells: a host type's properties
/// and methods (and on / off when it has events), a library type's methods, a namespace's names
fn typed_members(types: Option<&Types>, k: Known, cx: &Context, c: &mut Vec<Cand>) {
    let (Some(api), Some(_)) = (cx.api, types) else { return };
    match k {
        Known::Ns(ns) => {
            let pre = format!("{}.", ns);
            for g in api.globals.iter().filter(|g| g.name.starts_with(&pre)) {
                let short = &g.name[pre.len()..];
                match short.split_once('.') {
                    // a namespace inside it
                    Some((first, _)) => {
                        if !c.iter().any(|x| x.item.label == first) {
                            c.push(Cand { item: Item { label: String::from(first), kind: ItemKind::Module, detail: format!("{}{}", pre, first), doc: String::new() }, rank: 2 });
                        }
                    }
                    None => c.push(Cand { item: global_item(api, short, g), rank: 2 }),
                }
            }
            for h in cx.host.iter().filter(|h| h.name.starts_with(&pre) && !h.name[pre.len()..].contains('.')) {
                let kind = if h.params.is_some() { ItemKind::Function } else { ItemKind::Value };
                c.push(Cand { item: Item { label: String::from(&h.name[pre.len()..]), kind, detail: host_detail(h), doc: String::from(h.doc) }, rank: 2 });
            }
        }
        Known::Ty(t) => {
            let t = crate::api::member_type(&t);
            let lib = crate::api::list_of(t).map_or(t, |_| "list");
            if let Some(ty) = api.get_type(t) {
                for p in &ty.props {
                    let ro = if p.readonly { format!(" ({})", t!("nur lesbar")) } else { String::new() };
                    c.push(Cand { item: Item { label: p.name.clone(), kind: ItemKind::Value, detail: format!("{}.{}{}", ty.name, p.label().trim_start_matches("readonly "), ro), doc: p.doc.clone() }, rank: 2 });
                }
                for m in &ty.methods {
                    c.push(Cand { item: Item { label: m.name.clone(), kind: ItemKind::Method, detail: m.sig.label(&format!("{}.{}", ty.name, m.name)), doc: m.doc.clone() }, rank: 2 });
                }
                if !ty.events.is_empty() {
                    let evs: Vec<&str> = ty.events.iter().map(|e| e.name.as_str()).collect();
                    let doc = tf!("Ereignisse: {}", evs.join(", "));
                    c.push(Cand { item: Item { label: String::from("on"), kind: ItemKind::Method, detail: format!("{}.on(event: str, f: fn)", ty.name), doc: tf!("Ruft f(e) bei jedem Ereignis event auf. {}", doc) }, rank: 3 });
                    c.push(Cand { item: Item { label: String::from("off"), kind: ItemKind::Method, detail: format!("{}.off(event: str, f: fn)", ty.name), doc: tf!("Nimmt f (ohne f: alle Funktionen) für event wieder weg. {}", doc) }, rank: 3 });
                }
            } else if matches!(lib, "str" | "list" | "map" | "bytes") {
                lib_methods(lib, c);
            }
        }
    }
}

/// the members after a dot (r: the token before it)
fn members(src: &str, toks: &[Tok], idx: &Index, r: Option<usize>, cx: &Context, c: &mut Vec<Cand>) {
    let methods = lib_methods;
    match recv_of(src, toks, idx, r) {
        Recv::Import(path) => {
            if let Some((mi, _)) = module(cx, &path) {
                for s in mi.symbols.iter().filter(|s| matches!(s.kind, SymKind::Global | SymKind::Fn) && s.parent.is_none() && s.scope.0 == 0) {
                    let kind = if s.kind == SymKind::Fn || s.ty == "fn" { ItemKind::Function } else { ItemKind::Variable };
                    c.push(Cand { item: Item { label: s.name.clone(), kind, detail: sig_of(s), doc: s.doc.clone() }, rank: 1 });
                }
            }
        }
        Recv::Type(ty) => {
            if ty.is_empty() || ty == "fn" {
                for t in ["str", "list", "map", "bytes"] {
                    methods(t, c);
                }
            } else {
                methods(ty, c);
            }
        }
        Recv::Name(n) => {
            let pre = format!("{}.", n);
            let mut any = false;
            for h in cx.host.iter().filter(|h| h.name.starts_with(&pre)) {
                let short = &h.name[pre.len()..];
                if short.contains('.') {
                    continue;
                }
                any = true;
                let kind = if h.params.is_some() { ItemKind::Function } else { ItemKind::Value };
                c.push(Cand { item: Item { label: String::from(short), kind, detail: host_detail(h), doc: String::from(h.doc) }, rank: 2 });
            }
            if lsdoc::MODULES.contains(&n.as_str()) {
                any = true;
                for d in lsdoc::DOCS.iter().filter(|d| d.name.starts_with(&pre)) {
                    let short = &d.name[pre.len()..];
                    let kind = if d.params.is_none() { ItemKind::Constant } else { ItemKind::Function };
                    c.push(Cand { item: Item { label: String::from(short), kind, detail: lib_detail(d, false), doc: String::from(d.doc()) }, rank: 3 });
                }
            }
            if !any {
                for t in ["str", "list", "map", "bytes"] {
                    methods(t, c);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- hover and signature help

/// see the module comment
pub fn hover<'a>(src: &str, pos: u32, cx: impl Into<Context<'a>>) -> Option<Hover> {
    let cx: Context = cx.into();
    let cx = &cx;
    let (toks, _) = lex::lex(src);
    let k = word_at(&toks, pos)?;
    let t = toks[k];
    if t.kind != TokKind::Ident {
        return None;
    }
    let lines = Lines::new(src);
    let span = lines.span(src, t.start, t.end);
    let name = text_of(src, &t);
    let (prog, _) = crate::parse_partial("", src);
    let idx = index_of(&prog, src);
    let types = cx.api.map(|api| lsapi::infer(&prog, &idx, api));
    if prev_sig(&toks, k).is_some_and(|d| toks[d].kind == TokKind::Dot) {
        let r = receiver(&toks, k);
        let (sig, kind, doc) = member_doc(src, &toks, &idx, r, name, cx, types.as_ref())?;
        return Some(Hover { span, sig, kind, doc });
    }
    // (S3) the name of a named argument to the host or the library: its parameter (one to a fn of
    // the script is a use of its parameter: below)
    if idx.use_from(t.start).is_some_and(|u| u.sym.is_none())
        && let Some(c) = next_sig(&toks, k).filter(|&c| toks[c].kind == TokKind::Colon)
        && prev_sig(&toks, k).is_some_and(|p| matches!(toks[p].kind, TokKind::LParen | TokKind::Comma))
    {
        let s = signature(src, toks[c].end, *cx)?;
        let f = &s.forms[s.form];
        let &(a, b) = f.params.get(f.active)?;
        let callee = f.label.split('(').next().unwrap_or("");
        return Some(Hover { span, sig: String::from(&f.label[a as usize..b as usize]), kind: tf!("Parameter von {}", callee), doc: f.doc.clone() });
    }
    if let Some(s) = idx.sym_of(name, t.start) {
        let s2 = &idx.symbols[s];
        let mut sig = sig_of(s2);
        // (S2) the type the host's description tells
        if let Some(ty) = types.as_ref().map(|t| t.sym[s].as_str()).filter(|t| !t.is_empty() && s2.kind != SymKind::Fn && s2.ty != "fn") {
            sig = format!("{}: {}", sig, ty);
        }
        return Some(Hover { span, sig, kind: kind_of(&idx, s2), doc: s2.doc.clone() });
    }
    if let Some(g) = cx.api.and_then(|api| api.global(name).map(|g| (api, g))) {
        let it = global_item(g.0, name, g.1);
        let kind = if it.kind == ItemKind::Module { t!("Namensraum des Hosts") } else { t!("Name des Hosts") };
        // (S3) a function with several forms: each on a line of its own
        let forms = global_forms(g.0, name);
        if forms.len() > 1 {
            let (sig, doc) = forms_text(&forms);
            return Some(Hover { span, sig, kind: String::from(kind), doc });
        }
        return Some(Hover { span, sig: it.detail, kind: String::from(kind), doc: it.doc });
    }
    if let Some(h) = cx.host.iter().find(|h| h.name == name) {
        return Some(Hover { span, sig: host_detail(h), kind: String::from(t!("Name des Hosts")), doc: String::from(h.doc) });
    }
    if let Some(d) = lsdoc::doc(name) {
        return Some(Hover { span, sig: lib_detail(d, false), kind: String::from(t!("eingebaut")), doc: String::from(d.doc()) });
    }
    if lsdoc::MODULES.contains(&name) {
        return Some(Hover { span, sig: String::from(name), kind: String::from(t!("Modul der Standardbibliothek")), doc: String::new() });
    }
    if cx.host.iter().any(|h| h.name.split('.').next() == Some(name)) || cx.api.is_some_and(|a| a.is_namespace(name)) {
        return Some(Hover { span, sig: String::from(name), kind: String::from(t!("Name des Hosts")), doc: String::new() });
    }
    None
}

/// (S3) a form of a host function or method: its label ("vec(x: float): vec"), description and
/// signature
type FormOf<'a> = (String, String, &'a Sig);

/// (S3) the forms of a global function of the description, in their order
fn global_forms<'a>(api: &'a Api, name: &str) -> Vec<FormOf<'a>> {
    api.funcs(name)
        .into_iter()
        .filter_map(|g| match &g.kind {
            GlobalKind::Func(s) => Some((s.label(name), g.doc.clone(), s)),
            _ => None,
        })
        .collect()
}

/// (S3) the forms of member `name` of a receiver whose type the description tells: a namespace's
/// function, a host type's method (none: it has no such function or method)
fn typed_forms<'a>(api: &'a Api, k: &Known, name: &str) -> Vec<FormOf<'a>> {
    match k {
        Known::Ns(ns) => global_forms(api, &format!("{}.{}", ns, name)),
        Known::Ty(t) => match api.get_type(crate::api::member_type(t)) {
            Some(ty) => ty.find_methods(name).into_iter().map(|m| (m.sig.label(&format!("{}.{}", ty.name, m.name)), m.doc.clone(), &m.sig)).collect(),
            None => Vec::new(),
        },
    }
}

/// the forms for a hover: their labels on lines of their own, their descriptions (each once)
fn forms_text(forms: &[FormOf]) -> (String, String) {
    let sigs: Vec<&str> = forms.iter().map(|f| f.0.as_str()).collect();
    let mut docs: Vec<&str> = Vec::new();
    for f in forms {
        if !f.1.is_empty() && !docs.contains(&f.1.as_str()) {
            docs.push(&f.1);
        }
    }
    (sigs.join("\n"), docs.join("\n"))
}

/// (S2) (declaration, kind, doc) of member `name` of a receiver whose type the description
/// tells; None when it does not have one of that name
fn typed_member_doc(api: &Api, k: Known, name: &str) -> Option<(String, String, String)> {
    let forms = typed_forms(api, &k, name);
    match k {
        Known::Ns(ns) => {
            let full = format!("{}.{}", ns, name);
            let g = api.global(&full)?;
            let it = global_item(api, name, g);
            if forms.len() > 1 {
                let (sig, doc) = forms_text(&forms);
                return Some((sig, String::from(t!("Name des Hosts")), doc));
            }
            Some((it.detail, String::from(t!("Name des Hosts")), it.doc))
        }
        Known::Ty(t) => {
            let t = crate::api::member_type(&t);
            let lib = crate::api::list_of(t).map_or(t, |_| "list");
            let Some(ty) = api.get_type(t) else {
                return method_doc(lib, name).map(|(s, d)| (s, String::from(t!("Standardbibliothek")), d));
            };
            if let Some(p) = ty.find_prop(name) {
                let ro = if p.readonly { format!(", {}", t!("nur lesbar")) } else { String::new() };
                return Some((format!("{}.{}", ty.name, p.label().trim_start_matches("readonly ")), tf!("Eigenschaft von {}{}", ty.name, ro), p.doc.clone()));
            }
            if !forms.is_empty() {
                let (sig, doc) = forms_text(&forms);
                return Some((sig, tf!("Methode von {}", ty.name), doc));
            }
            if (name == "on" || name == "off") && !ty.events.is_empty() {
                let evs: Vec<String> = ty.events.iter().map(|e| e.label()).collect();
                return Some((format!("{}.{}(event: str, f: fn)", ty.name, name), String::from(t!("Ereignisse")), evs.join("\n")));
            }
            None
        }
    }
}

/// (signature, kind, doc) of member `name` after receiver token r
fn member_doc(src: &str, toks: &[Tok], idx: &Index, r: Option<usize>, name: &str, cx: &Context, types: Option<&Types>) -> Option<(String, String, String)> {
    if let (Some(t), Some(rr), Some(api)) = (types, r, cx.api)
        && let Some(k) = t.chain(src, toks, rr)
    {
        return typed_member_doc(api, k, name);
    }
    let lib = String::from(t!("Standardbibliothek"));
    match recv_of(src, toks, idx, r) {
        Recv::Import(path) => {
            let (mi, _) = module(cx, &path)?;
            let s = mi.symbols.iter().find(|s| s.name == name && matches!(s.kind, SymKind::Global | SymKind::Fn) && s.parent.is_none())?;
            Some((sig_of(s), tf!("{} aus {}", s.kind.word(), path), s.doc.clone()))
        }
        Recv::Name(n) => {
            let full = format!("{}.{}", n, name);
            if let Some(h) = cx.host.iter().find(|h| h.name == full) {
                return Some((host_detail(h), String::from(t!("Name des Hosts")), String::from(h.doc)));
            }
            if let Some(d) = lsdoc::doc(&full) {
                return Some((lib_detail(d, false), lib, String::from(d.doc())));
            }
            method_doc("", name).map(|(s, d)| (s, lib, d))
        }
        Recv::Type(ty) => method_doc(ty, name).map(|(s, d)| (s, lib, d)),
    }
}

/// the signature(s) and text of method `name` of type ty ("": of every type that has it)
fn method_doc(ty: &str, name: &str) -> Option<(String, String)> {
    let tys: &[&str] = if ty.is_empty() || ty == "fn" { &["str", "list", "map", "bytes"] } else { core::slice::from_ref(&ty) };
    let found: Vec<&lsdoc::Doc> = tys.iter().filter_map(|t| lsdoc::doc(&format!("{}.{}", t, name)).filter(|d| !d.recv.is_empty())).collect();
    let first = found.first()?;
    let sigs: Vec<String> = found.iter().map(|d| lib_detail(d, true)).collect();
    Some((sigs.join(" · "), String::from(first.doc())))
}

/// see the module comment
pub fn signature<'a>(src: &str, pos: u32, cx: impl Into<Context<'a>>) -> Option<Signature> {
    let cx: Context = cx.into();
    let cx = &cx;
    let pos = clamp_pos(src, pos);
    let (all, _) = lex::lex(src);
    if word_place(src, &all, pos).is_none() {
        return None;
    }
    let toks = lex::significant(&all, src);
    let end = toks.partition_point(|t| t.end <= pos);
    // back to the innermost ( that is open at pos
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut open = None;
    // the comma before the argument at pos (or the parenthesis)
    let mut sep = None;
    for j in (0..end).rev() {
        match toks[j].kind {
            TokKind::RParen | TokKind::RBracket | TokKind::RBrace | TokKind::StrTail => depth += 1,
            TokKind::StrMid if depth == 0 => return None,
            TokKind::StrMid => {}
            TokKind::LParen if depth == 0 => {
                open = Some(j);
                break;
            }
            TokKind::LBracket | TokKind::LBrace | TokKind::StrHead if depth == 0 => return None,
            TokKind::LParen | TokKind::LBracket | TokKind::LBrace | TokKind::StrHead => depth -= 1,
            TokKind::Comma if depth == 0 => {
                commas += 1;
                sep.get_or_insert(j);
            }
            _ => {}
        }
    }
    let o = open?;
    // (S3) a named argument at pos (`width: |`): its parameter is the active one
    let s0 = sep.unwrap_or(o);
    let named = match (toks.get(s0 + 1), toks.get(s0 + 2)) {
        (Some(a), Some(c)) if a.kind == TokKind::Ident && c.kind == TokKind::Colon && c.end <= pos => Some(text_of(src, a)),
        _ => None,
    };
    let f = o.checked_sub(1).filter(|&j| toks[j].kind == TokKind::Ident)?;
    let name = text_of(src, &toks[f]);
    let dot = f.checked_sub(1).filter(|&j| toks[j].kind == TokKind::Dot);
    let (prog, _) = crate::parse_partial("", src);
    let idx = index_of(&prog, src);
    let types = cx.api.map(|api| lsapi::infer(&prog, &idx, api));
    // (S3) the forms: (label, doc, the signature when the description has one)
    let found: Vec<(String, String, Option<&Sig>)> = match dot {
        Some(d) => {
            // the receiver as the full list sees it (for its symbol)
            let r = d.checked_sub(1).map(|j| all.partition_point(|t| t.start < toks[j].start)).filter(|&j| j < all.len());
            let known = match (&types, r) {
                (Some(t), Some(rr)) => t.chain(src, &all, rr),
                _ => None,
            };
            let typed = match (cx.api, &known) {
                (Some(api), Some(k)) => typed_forms(api, k, name),
                _ => Vec::new(),
            };
            if typed.is_empty() {
                let (sig, _, doc) = member_doc(src, &all, &idx, r, name, cx, types.as_ref())?;
                alloc::vec![(sig.split(" · ").next().unwrap_or("").to_string(), doc, None)]
            } else {
                typed.into_iter().map(|(l, d, s)| (l, d, Some(s))).collect()
            }
        }
        None => match idx.sym_of(name, toks[f].start) {
            Some(s) => {
                let s = &idx.symbols[s];
                if s.kind != SymKind::Fn && s.ty != "fn" && s.kind != SymKind::Param {
                    return None;
                }
                alloc::vec![(format!("{}({})", s.name, s.params.join(", ")), s.doc.clone(), None)]
            }
            None => {
                let typed = cx.api.map(|a| global_forms(a, name)).unwrap_or_default();
                match (typed.is_empty(), cx.host.iter().find(|h| h.name == name && h.params.is_some()), lsdoc::doc(name)) {
                    (false, _, _) => typed.into_iter().map(|(l, d, s)| (l, d, Some(s))).collect(),
                    (true, Some(h), _) => alloc::vec![(host_detail(h), String::from(h.doc), None)],
                    (true, None, Some(d)) if d.params.is_some() => alloc::vec![(lib_detail(d, false), String::from(d.doc()), None)],
                    _ => return None,
                }
            }
        },
    };
    // (S3) the form of the arguments typed so far: one with room for the argument at the caret
    // whose finished arguments' types (where known) fit, else one with room, else the first
    let given = match &types {
        Some(t) => t.arg_types(src, &all, all.partition_point(|x| x.start < toks[o].start), all.partition_point(|x| x.end <= pos), false),
        None => Vec::new(),
    };
    let takes = |s: &Sig, typed: bool| -> bool {
        if s.arity().1.is_some_and(|m| commas >= m.max(1)) {
            return false;
        }
        if named.is_some_and(|n| !s.params.iter().any(|p| p.name == n && !p.rest)) {
            return false;
        }
        if !typed {
            return true;
        }
        // the finished arguments by position and (S3) by name, their types where known
        let a: Vec<crate::api::Arg> = given.iter().map(|(n, t)| crate::api::Arg { name: n.as_deref(), ty: t }).collect();
        match s.bind_names(&a) {
            Ok(filled) => filled.iter().enumerate().all(|(j, args)| {
                args.iter().all(|&i| {
                    let t = given[i].1.as_str();
                    crate::api::fits(t, &s.params[j].ty) || (t == "nil" && s.params[j].optional)
                })
            }),
            Err(_) => false,
        }
    };
    let pick = |typed: bool| found.iter().position(|f| f.2.is_none_or(|s| takes(s, typed)));
    let form = pick(true).or_else(|| pick(false)).unwrap_or(0);
    let forms: Vec<Form> = found
        .iter()
        .map(|(label, doc, _)| {
            let params = param_ranges(label);
            let variadic = label.contains("..");
            let active = match named {
                Some(n) => params.iter().position(|&(a, b)| part_name(&label[a as usize..b as usize]) == n).unwrap_or(params.len()),
                None if commas >= params.len() && variadic && !params.is_empty() => params.len() - 1,
                None => commas.min(params.len()),
            };
            Form { label: label.clone(), params, active, doc: doc.clone() }
        })
        .collect();
    let cur = forms[form].clone();
    Some(Signature { label: cur.label, params: cur.params, active: cur.active, doc: cur.doc, forms, form })
}

/// (S3) the name of a parameter as a label writes it: "width" of "width: float = 1", "z?: T",
/// "b = 2"; "" for a pattern ([x, y])
fn part_name(part: &str) -> &str {
    let p = part.trim().trim_start_matches("...");
    let n = p.find(|c: char| !is_ident_char(c)).unwrap_or(p.len());
    &p[..n]
}

/// the bytes of the parameters inside the parentheses of a label "f(a, b, ..)" (commas inside
/// brackets and quotes of a default, "= vec(0, 0)", belong to their parameter)
fn param_ranges(label: &str) -> Vec<(u32, u32)> {
    let (Some(a), Some(b)) = (label.find('('), label.rfind(')')) else { return Vec::new() };
    let inner = &label[a + 1..b];
    let mut parts: Vec<&str> = Vec::new();
    let (mut depth, mut quote, mut from) = (0i32, false, 0usize);
    for (i, c) in inner.char_indices() {
        match c {
            '"' => quote = !quote,
            '(' | '[' | '{' | '<' if !quote => depth += 1,
            ')' | ']' | '}' | '>' if !quote => depth -= 1,
            ',' if depth == 0 && !quote => {
                parts.push(&inner[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[from..]);
    let mut v = Vec::new();
    let mut at = a + 1;
    for part in parts {
        let lead = part.len() - part.trim_start().len();
        let s = at + lead;
        let e = s + part.trim().len();
        if e > s && part.trim() != ".." {
            v.push((s as u32, e as u32));
        }
        at += part.len() + 1;
    }
    v
}

// ---------------------------------------------------------------- folding and the outline

/// see the module comment
pub fn folding(src: &str) -> Vec<Fold> {
    let (prog, _) = crate::parse_partial("", src);
    let lines = Lines::new(src);
    let mut v: Vec<Fold> = Vec::new();
    let add = |a: u32, z: u32, kind: FoldKind, v: &mut Vec<Fold>| {
        // a..z: the bytes of the opener and the closer
        let (la, lz) = (lines.line(a), lines.line(z.saturating_sub(1)));
        // (a closer with only blanks or other closers before it keeps its line)
        let t = lines.text(src, lz);
        let col = lines.span(src, z.saturating_sub(1), z).col as usize;
        let alone = t.chars().take(col.saturating_sub(1)).all(|c| c.is_whitespace() || matches!(c, ')' | ']' | '}'));
        let end = if alone { lz.saturating_sub(1) } else { lz };
        if end > la {
            v.push(Fold { line: la, end, kind });
        }
    };
    let mut blocks: Vec<(u32, u32, FoldKind)> = Vec::new();
    fold_all(&prog.body, src, &mut blocks);
    for (a, z, k) in blocks {
        add(a, z, k, &mut v);
    }
    // comments: /* */ over lines, runs of // lines
    let (toks, _) = lex::lex(src);
    let mut run: Option<(u32, u32)> = None;
    for (i, t) in toks.iter().enumerate() {
        if t.kind != TokKind::Comment {
            continue;
        }
        let text = text_of(src, t);
        if text.starts_with("/*") {
            let (la, lz) = (lines.line(t.start), lines.line(t.end.saturating_sub(1)));
            if lz > la {
                v.push(Fold { line: la, end: lz, kind: FoldKind::Comment });
            }
            continue;
        }
        // a line comment alone on its line
        let alone = prev_sig(&toks, i).is_none_or(|p| break_between(&toks, src, p, i));
        let l = lines.line(t.start);
        if !alone {
            continue;
        }
        run = match run {
            Some((a, z)) if z + 1 == l => Some((a, l)),
            Some((a, z)) => {
                if z > a {
                    v.push(Fold { line: a, end: z, kind: FoldKind::Comment });
                }
                Some((l, l))
            }
            None => Some((l, l)),
        };
    }
    if let Some((a, z)) = run {
        if z > a {
            v.push(Fold { line: a, end: z, kind: FoldKind::Comment });
        }
    }
    // one per first line: the longest
    v.sort_by(|a, b| a.line.cmp(&b.line).then(b.end.cmp(&a.end)));
    v.dedup_by_key(|f| f.line);
    v
}

/// the blocks, lists, maps, (S3) comprehensions and argument lists of every statement and
/// expression: (the opener's byte, the closer's end, kind)
fn fold_all(stmts: &[Stmt], src: &str, out: &mut Vec<(u32, u32, FoldKind)>) {
    let block = |b: &Block, out: &mut Vec<(u32, u32, FoldKind)>| out.push((b.span.start, b.span.end, FoldKind::Block));
    visit(stmts, &mut |n| match n {
        Node::Stmt(s) => match &s.kind {
            StmtKind::Fn(f) => block(&f.body, out),
            StmtKind::If { arms, els } => {
                for (_, b) in arms {
                    block(b, out);
                }
                if let Some(b) = els {
                    block(b, out);
                }
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => block(body, out),
            StmtKind::Try { body, catch, .. } => {
                block(body, out);
                block(catch, out);
            }
            StmtKind::Block(b) => block(b, out),
            _ => {}
        },
        Node::Expr(e) => match &e.kind {
            ExprKind::List(_) | ExprKind::Map(_) | ExprKind::Comp(..) => out.push((e.span.start, e.span.end, FoldKind::Literal)),
            ExprKind::Fn(f) => block(&f.body, out),
            ExprKind::Call(f, _) | ExprKind::Method(f, _, _) => {
                // the argument list from its ( on
                let from = match &e.kind {
                    ExprKind::Method(_, n, _) => n.span.end,
                    _ => f.span.end,
                };
                if let Some(k) = src.get(from as usize..e.span.end as usize).and_then(|t| t.find('(')) {
                    let z = e.span.end;
                    if src.as_bytes().get(z as usize - 1) == Some(&b')') {
                        out.push((from + k as u32, z, FoldKind::Literal));
                    }
                }
            }
            _ => {}
        },
        Node::Pattern(_) => {}
    });
}


/// see the module comment
pub fn symbols(src: &str) -> Vec<DocSymbol> {
    let (prog, _) = crate::parse_partial("", src);
    let idx = index_of(&prog, src);
    let mut out: Vec<DocSymbol> = Vec::new();
    for s in &prog.body {
        match &s.kind {
            StmtKind::Let { pat, init } => {
                // (S3) a pattern: each of its names, the fns of the value under the first
                let mut names = Vec::new();
                pat.names(&mut names);
                for (i, name) in names.into_iter().enumerate() {
                    let Some(sym) = idx.symbols.iter().find(|x| x.span == name.span) else { continue };
                    let mut d = DocSymbol { name: name.text.clone(), kind: SymKind::Global, detail: sig_of(sym), span: name.span, range: s.span, children: Vec::new() };
                    if let (0, Some(e)) = (i, init) {
                        nested_expr(e, &idx, &mut d.children);
                    }
                    out.push(d);
                }
            }
            StmtKind::Fn(f) => {
                if let Some(d) = fn_symbol(f, &idx) {
                    out.push(d);
                }
            }
            _ => nested(core::slice::from_ref(s), &idx, &mut out),
        }
    }
    out
}

fn fn_symbol(f: &FnDef, idx: &Index) -> Option<DocSymbol> {
    let n = f.name.as_ref()?;
    let sym = idx.symbols.iter().find(|x| x.span == n.span)?;
    let mut d = DocSymbol { name: n.text.clone(), kind: SymKind::Fn, detail: sig_of(sym), span: n.span, range: f.span, children: Vec::new() };
    nested(&f.body.stmts, idx, &mut d.children);
    Some(d)
}

/// the fn declarations inside statements (not inside other fn declarations: those are theirs)
fn nested(stmts: &[Stmt], idx: &Index, out: &mut Vec<DocSymbol>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Fn(f) => out.extend(fn_symbol(f, idx)),
            StmtKind::Let { init: Some(x), .. } | StmtKind::Throw(x) | StmtKind::Expr(x) | StmtKind::Return(Some(x)) => nested_expr(x, idx, out),
            StmtKind::If { arms, els } => {
                for (c, b) in arms {
                    nested_expr(c, idx, out);
                    nested(&b.stmts, idx, out);
                }
                if let Some(b) = els {
                    nested(&b.stmts, idx, out);
                }
            }
            StmtKind::While { cond, body } => {
                nested_expr(cond, idx, out);
                nested(&body.stmts, idx, out);
            }
            StmtKind::For { src, body, .. } => {
                if let ForSrc::Each(x) = src {
                    nested_expr(x, idx, out);
                }
                nested(&body.stmts, idx, out);
            }
            StmtKind::Try { body, catch, .. } => {
                nested(&body.stmts, idx, out);
                nested(&catch.stmts, idx, out);
            }
            StmtKind::Assign { value, .. } | StmtKind::AssignPat { value, .. } => nested_expr(value, idx, out),
            StmtKind::Block(b) => nested(&b.stmts, idx, out),
            _ => {}
        }
    }
}

fn nested_expr(e: &Expr, idx: &Index, out: &mut Vec<DocSymbol>) {
    match &e.kind {
        ExprKind::Fn(f) => nested(&f.body.stmts, idx, out),
        ExprKind::Interp(v) | ExprKind::List(v) => v.iter().for_each(|x| nested_expr(x, idx, out)),
        ExprKind::Map(kv) => kv.iter().for_each(|(k, v)| {
            nested_expr(k, idx, out);
            nested_expr(v, idx, out);
        }),
        ExprKind::Unary(_, a) | ExprKind::Field(a, _) => nested_expr(a, idx, out),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
            nested_expr(a, idx, out);
            nested_expr(b, idx, out);
        }
        ExprKind::Call(g, args) | ExprKind::Method(g, _, args) => {
            nested_expr(g, idx, out);
            args.iter().for_each(|x| nested_expr(x, idx, out));
        }
        ExprKind::Cond(c, a, b) => {
            nested_expr(c, idx, out);
            nested_expr(a, idx, out);
            nested_expr(b, idx, out);
        }
        ExprKind::Named(_, a) => nested_expr(a, idx, out),
        ExprKind::Comp(x, clauses) => {
            for c in clauses {
                match c {
                    Clause::For { src: ForSrc::Each(s), .. } | Clause::If(s) => nested_expr(s, idx, out),
                    Clause::For { .. } => {}
                }
            }
            nested_expr(x, idx, out);
        }
        ExprKind::Nil | ExprKind::Bool(_) | ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) | ExprKind::Var(_) => {}
    }
}

// ---------------------------------------------------------------- (S2) diagnostics for a host

/// see the module comment: syntax errors; with an Api or HostItems the names against theirs,
/// the built-ins and the library (an unknown one is an error, as compile_with makes it), else
/// against script::GLOBALS as script::check; with an Api the members of known host types,
/// (S3) removed names and with Api::check_calls the number of arguments of calls (warnings:
/// lsapi.rs; the same as script::compile_api gives); sorted by position
pub fn check<'a>(src: &str, cx: impl Into<Context<'a>>) -> Vec<Diagnostic> {
    let cx: Context = cx.into();
    let (prog, mut d) = crate::parse_partial("", src);
    let mut roots: Vec<&str> = Vec::new();
    for h in cx.host {
        let root = h.name.split('.').next().unwrap_or(h.name);
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    match cx.api {
        Some(api) => {
            d.extend(lsapi::check_names(&prog, api, &roots));
            d.extend(lsapi::check_members(&prog, src, api));
        }
        None if roots.is_empty() => d.extend(crate::check::resolve(&prog, crate::GLOBALS)),
        None => d.extend(crate::check::resolve_strict(&prog, &crate::known_names(&roots), None)),
    }
    d.sort_by_key(|x| (x.line, x.col));
    d
}
