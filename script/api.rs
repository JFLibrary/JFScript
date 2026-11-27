// api -- (S2) a host's object model as data (G3 and G8.3 of docs/RICHTUNG.md): its global names
// (functions, values, namespaces) and its types with properties, methods, events and operators,
// each with types and a description.  The language services read it (ls.rs with ls::Context:
// completion after a dot, hover, signature help, unknown members; lsapi.rs infers the types), the
// object catalogue is made from it.  An app writes it in its own language (G9.2), the names in
// English (G9.3); docs/HOST.md shows a whole host.
//
//   Api::new("modeltool").doc("..")
//       .func("vec(x: float, y: float, z: float = 0): vec", "Ein neuer Vektor.")
//       .value("doc: document", "Das offene Dokument.")
//       .namespace("log", "Das Protokoll.").func("log.warn(...values): nil", "..")
//       .typ(Type::new("vec", "Ein Vektor.").value()
//           .prop("readonly x: float", "..").method("length(): float", "..")
//           .op("vec + vec: vec", "..").op("float * vec: vec", "..").op("-vec: vec", ".."))
//       .typ(Type::new("element", "..").event("click: mouse_event", ".."))
//   api.names()            the roots of the global names, for script::compile_with
//   api.validate()         what does not fit: an unknown type, a name twice (German texts)
//   api.catalogue()        the object catalogue as a tree of Entry, for a UI (jfscript-ide later)
//   api.markdown()         the same as a Markdown page: the app's reference
//   Api::from_items(&items)   the description of today's ls::HostItem list (no types)
//
// Types are text: int float str bool bytes list map fn nil any, the name of a type of the
// description, list<T> (a list of T: `for x in` and `l[i]` give T), (S3) promise and promise<T>
// (what a long operation returns: `await` gives a T; task.rs); anything else is shown as written
// and means nothing to the services.  Signatures are written as in TypeScript (G1 d, G3):
// "name(a: T, b: T = default, ...rest: T): R"; a parameter without a type is any, a function
// without ": R" returns nothing known.  The builders never fail: what does not parse is kept as
// text; validate() tells.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ls::HostItem;
use crate::vm::Operator;

/// the types every script knows (not described by a host)
pub(crate) const BUILTIN_TYPES: &[&str] = &["int", "float", "str", "bool", "bytes", "list", "map", "fn", "nil", "any", "promise"];

/// a host's object model (see the module comment)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Api {
    /// the app's name, the catalogue's title ("modeltool")
    pub name: String,
    pub doc: String,
    pub globals: Vec<Global>,
    pub types: Vec<Type>,
}

/// a global name of the host
#[derive(Clone, Debug, PartialEq)]
pub struct Global {
    /// "vec", "doc", "log.warn" (a dotted name is a member of a namespace)
    pub name: String,
    pub kind: GlobalKind,
    pub doc: String,
}

/// what a global name is
#[derive(Clone, Debug, PartialEq)]
pub enum GlobalKind {
    /// a function with its parameters and result
    Func(Sig),
    /// a value of a type (a host object such as the document, a map such as config)
    Value(String),
    /// a namespace for dotted names (log for log.warn)
    Namespace,
}

/// the parameters and the result of a function or method
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sig {
    pub params: Vec<Param>,
    /// the type of the result ("" nothing known)
    pub ret: String,
}

/// a parameter
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Param {
    pub name: String,
    /// "" any
    pub ty: String,
    /// the default as written ("0", "\"x\""); None: none (a missing argument is nil)
    pub default: Option<String>,
    /// ...name: any number of further arguments
    pub rest: bool,
}

/// a type of host objects
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Type {
    /// the name type(x) gives and new_object / new_value use ("vec", "mesh", "shape")
    pub name: String,
    pub doc: String,
    /// a value type: == compares type and data (Cx::new_value); else identity (new_object)
    pub value: bool,
    pub props: Vec<Prop>,
    pub methods: Vec<Method>,
    pub events: Vec<Event>,
    pub ops: Vec<OpDesc>,
}

/// a property (obj.name, obj.name = v)
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub name: String,
    pub ty: String,
    pub readonly: bool,
    pub doc: String,
}

/// a method (obj.name(..))
#[derive(Clone, Debug, PartialEq)]
pub struct Method {
    pub name: String,
    pub sig: Sig,
    pub doc: String,
}

/// an event (obj.on(name, fn(e) { .. })): e is of type ty
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: String,
    pub ty: String,
    pub doc: String,
}

/// an operator of a type: lhs op rhs gives ret (op Neg: -rhs, lhs "")
#[derive(Clone, Debug, PartialEq)]
pub struct OpDesc {
    pub op: Operator,
    pub lhs: String,
    pub rhs: String,
    pub ret: String,
    pub doc: String,
}

/// what an entry of the catalogue is
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntryKind {
    Function,
    Value,
    Namespace,
    /// a type of objects (identity)
    Type,
    /// a value type
    ValueType,
    Property,
    Method,
    Event,
    Operator,
}

impl EntryKind {
    /// the German word ("Funktion", "Eigenschaft" ..)
    pub fn word(self) -> &'static str {
        match self {
            EntryKind::Function => "Funktion",
            EntryKind::Value => "Wert",
            EntryKind::Namespace => "Namensraum",
            EntryKind::Type => "Typ",
            EntryKind::ValueType => "Werttyp",
            EntryKind::Property => "Eigenschaft",
            EntryKind::Method => "Methode",
            EntryKind::Event => "Ereignis",
            EntryKind::Operator => "Operator",
        }
    }
}

/// an entry of the object catalogue: a global, a type or a member of one
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub kind: EntryKind,
    /// "vec", "x", "log.warn", "+"
    pub name: String,
    /// the declaration: "vec(x: float, y: float): vec", "readonly x: float", "vec + vec: vec"
    pub label: String,
    pub doc: String,
    pub children: Vec<Entry>,
}

// ---------------------------------------------------------------- parsing the texts

/// the parts of s split at `sep` outside brackets ( [ { < and quotes
fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut quote, mut from) = (0i32, false, 0usize);
    let mut prev = ' ';
    for (i, c) in s.char_indices() {
        match c {
            '"' if prev != '\\' => quote = !quote,
            '(' | '[' | '{' | '<' if !quote => depth += 1,
            ')' | ']' | '}' | '>' if !quote => depth -= 1,
            c if c == sep && depth == 0 && !quote => {
                out.push(&s[from..i]);
                from = i + c.len_utf8();
            }
            _ => {}
        }
        prev = c;
    }
    out.push(&s[from..]);
    out
}

/// the first top-level position of c in s
fn find_top(s: &str, c: char) -> Option<usize> {
    let parts = split_top(s, c);
    if parts.len() < 2 { None } else { Some(parts[0].len()) }
}

/// "a: T = d", "...rest: T", "x", ".."
fn parse_param(p: &str) -> Param {
    let p = p.trim();
    if p == ".." || p == "..." {
        return Param { name: String::from(".."), rest: true, ..Default::default() };
    }
    let (p, rest) = match p.strip_prefix("...") {
        Some(r) => (r.trim(), true),
        None => (p, false),
    };
    let (head, default) = match find_top(p, '=') {
        Some(i) => (&p[..i], Some(p[i + 1..].trim().to_string())),
        None => (p, None),
    };
    let (name, ty) = match find_top(head, ':') {
        Some(i) => (head[..i].trim(), head[i + 1..].trim()),
        None => (head.trim(), ""),
    };
    Param { name: String::from(name), ty: String::from(ty), default, rest }
}

/// "a: T, b" (the text inside the parentheses)
fn parse_params(s: &str) -> Vec<Param> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    split_top(s, ',').into_iter().map(parse_param).collect()
}

/// "name(params): ret" -> (name, Sig); without parentheses: no parameters
fn parse_sig(s: &str) -> (String, Sig) {
    let s = s.trim();
    let Some(open) = s.find('(') else {
        return match find_top(s, ':') {
            Some(i) => (String::from(s[..i].trim()), Sig { params: Vec::new(), ret: String::from(s[i + 1..].trim()) }),
            None => (String::from(s), Sig::default()),
        };
    };
    let name = String::from(s[..open].trim());
    // the matching )
    let (mut depth, mut quote, mut close) = (0i32, false, s.len());
    let mut prev = ' ';
    for (i, c) in s[open..].char_indices() {
        match c {
            '"' if prev != '\\' => quote = !quote,
            '(' if !quote => depth += 1,
            ')' if !quote => {
                depth -= 1;
                if depth == 0 {
                    close = open + i;
                    break;
                }
            }
            _ => {}
        }
        prev = c;
    }
    let params = parse_params(s.get(open + 1..close).unwrap_or(""));
    let after = s.get(close + 1..).unwrap_or("").trim();
    let ret = after.strip_prefix(':').map_or("", |r| r.trim());
    (name, Sig { params, ret: String::from(ret) })
}

/// "readonly x: T" -> (readonly, name, T)
fn parse_typed(s: &str) -> (bool, String, String) {
    let s = s.trim();
    let (ro, s) = match s.strip_prefix("readonly ") {
        Some(r) => (true, r.trim()),
        None => (false, s),
    };
    match find_top(s, ':') {
        Some(i) => (ro, String::from(s[..i].trim()), String::from(s[i + 1..].trim())),
        None => (ro, String::from(s), String::new()),
    }
}

/// "vec + vec: vec", "float * vec: vec", "-vec: vec"
fn parse_op(s: &str, doc: &str) -> Option<OpDesc> {
    let s = s.trim();
    let i = s.rfind(':')?;
    let (expr, ret) = (s[..i].trim(), s[i + 1..].trim());
    let parts: Vec<&str> = expr.split_whitespace().collect();
    let (op, lhs, rhs) = match parts.as_slice() {
        [one] if one.starts_with('-') && one.len() > 1 => (Operator::Neg, "", &one[1..]),
        ["-", b] => (Operator::Neg, "", *b),
        [a, o, b] => (Operator::of(o).filter(|o| *o != Operator::Neg)?, *a, *b),
        _ => return None,
    };
    Some(OpDesc { op, lhs: String::from(lhs), rhs: String::from(rhs), ret: String::from(ret), doc: String::from(doc) })
}

// ---------------------------------------------------------------- labels

impl Param {
    /// "a: T = d", "...rest: T", ".."
    pub fn label(&self) -> String {
        if self.name == ".." {
            return String::from("..");
        }
        let mut s = String::new();
        if self.rest {
            s.push_str("...");
        }
        s.push_str(&self.name);
        if !self.ty.is_empty() {
            s.push_str(": ");
            s.push_str(&self.ty);
        }
        if let Some(d) = &self.default {
            s.push_str(" = ");
            s.push_str(d);
        }
        s
    }
}

impl Sig {
    /// "name(a: T, b: T = d): R"
    pub fn label(&self, name: &str) -> String {
        let ps: Vec<String> = self.params.iter().map(|p| p.label()).collect();
        let mut s = format!("{}({})", name, ps.join(", "));
        if !self.ret.is_empty() {
            s.push_str(": ");
            s.push_str(&self.ret);
        }
        s
    }
}

impl Prop {
    /// "readonly x: float"
    pub fn label(&self) -> String {
        let ro = if self.readonly { "readonly " } else { "" };
        if self.ty.is_empty() { format!("{}{}", ro, self.name) } else { format!("{}{}: {}", ro, self.name, self.ty) }
    }
}

impl Event {
    /// "click: mouse_event"
    pub fn label(&self) -> String {
        if self.ty.is_empty() { self.name.clone() } else { format!("{}: {}", self.name, self.ty) }
    }
}

impl OpDesc {
    /// "vec + vec: vec", "-vec: vec"
    pub fn label(&self) -> String {
        let e = if self.op == Operator::Neg { format!("-{}", self.rhs) } else { format!("{} {} {}", self.lhs, self.op.text(), self.rhs) };
        if self.ret.is_empty() { e } else { format!("{}: {}", e, self.ret) }
    }
}

// ---------------------------------------------------------------- building

impl Api {
    /// an empty description named for the catalogue
    pub fn new(name: &str) -> Api {
        Api { name: String::from(name), ..Default::default() }
    }

    /// the description of the whole (the catalogue's first paragraph)
    pub fn doc(mut self, doc: &str) -> Api {
        self.doc = String::from(doc);
        self
    }

    /// a global function: "vec(x: float, y: float = 0): vec", also a member of a namespace
    /// ("log.warn(...values)")
    pub fn func(mut self, sig: &str, doc: &str) -> Api {
        let (name, sig) = parse_sig(sig);
        self.globals.push(Global { name, kind: GlobalKind::Func(sig), doc: String::from(doc) });
        self
    }

    /// a global value: "doc: document", "config: map"
    pub fn value(mut self, decl: &str, doc: &str) -> Api {
        let (_, name, ty) = parse_typed(decl);
        self.globals.push(Global { name, kind: GlobalKind::Value(ty), doc: String::from(doc) });
        self
    }

    /// a namespace for dotted names ("log" for log.warn)
    pub fn namespace(mut self, name: &str, doc: &str) -> Api {
        self.globals.push(Global { name: String::from(name.trim()), kind: GlobalKind::Namespace, doc: String::from(doc) });
        self
    }

    /// a type of host objects
    pub fn typ(mut self, t: Type) -> Api {
        self.types.push(t);
        self
    }

    /// the description of HostItems (ls::HostItem, the services' form before S2): a function
    /// for an item with parameters (no types: "port, handlers"), a namespace for a name others
    /// are dotted under, else a value; namespaces of dotted names without an item of their own
    /// are added
    pub fn from_items(items: &[HostItem]) -> Api {
        let mut api = Api::default();
        for h in items {
            let kind = match h.params {
                Some(p) => GlobalKind::Func(Sig { params: parse_params(p), ret: String::new() }),
                None if items.iter().any(|x| x.name.len() > h.name.len() && x.name.starts_with(h.name) && x.name.as_bytes()[h.name.len()] == b'.') => GlobalKind::Namespace,
                None => GlobalKind::Value(String::new()),
            };
            api.globals.push(Global { name: String::from(h.name), kind, doc: String::from(h.doc) });
        }
        let mut roots: Vec<String> = Vec::new();
        for g in &api.globals {
            if let Some((root, _)) = g.name.split_once('.')
                && !api.globals.iter().any(|x| x.name == root)
                && !roots.iter().any(|r| r == root)
            {
                roots.push(String::from(root));
            }
        }
        for r in roots {
            api.globals.push(Global { name: r, kind: GlobalKind::Namespace, doc: String::new() });
        }
        api
    }
}

impl Type {
    /// a type of objects with identity (new_object): an element, a mesh, a shape
    pub fn new(name: &str, doc: &str) -> Type {
        Type { name: String::from(name), doc: String::from(doc), ..Default::default() }
    }

    /// make it a value type (new_value): == compares the data
    pub fn value(mut self) -> Type {
        self.value = true;
        self
    }

    /// a property: "x: float", "readonly id: int"
    pub fn prop(mut self, decl: &str, doc: &str) -> Type {
        let (readonly, name, ty) = parse_typed(decl);
        self.props.push(Prop { name, ty, readonly, doc: String::from(doc) });
        self
    }

    /// a method: "bevel(width: float, segments: int = 1): mesh"
    pub fn method(mut self, sig: &str, doc: &str) -> Type {
        let (name, sig) = parse_sig(sig);
        self.methods.push(Method { name, sig, doc: String::from(doc) });
        self
    }

    /// an event and the type of its event object: "click: mouse_event"
    pub fn event(mut self, decl: &str, doc: &str) -> Type {
        let (_, name, ty) = parse_typed(decl);
        self.events.push(Event { name, ty, doc: String::from(doc) });
        self
    }

    /// an operator: "vec + vec: vec", "vec * float: vec", "float * vec: vec", "-vec: vec",
    /// "vec < vec: bool"; one that does not parse is left out (validate() does not see it)
    pub fn op(mut self, decl: &str, doc: &str) -> Type {
        if let Some(o) = parse_op(decl, doc) {
            self.ops.push(o);
        }
        self
    }

    /// the property of that name
    pub fn find_prop(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.name == name)
    }

    /// the method of that name
    pub fn find_method(&self, name: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.name == name)
    }

    /// the event of that name
    pub fn find_event(&self, name: &str) -> Option<&Event> {
        self.events.iter().find(|e| e.name == name)
    }
}

// ---------------------------------------------------------------- looking up

/// the element type of "list<T>"
pub(crate) fn list_of(ty: &str) -> Option<&str> {
    ty.trim().strip_prefix("list<")?.strip_suffix('>').map(str::trim)
}

/// (S3) the type of the value of "promise<T>" (what await gives)
pub(crate) fn promise_of(ty: &str) -> Option<&str> {
    ty.trim().strip_prefix("promise<")?.strip_suffix('>').map(str::trim)
}

/// an argument of type `got` fits a parameter or operand of type `want`
pub(crate) fn fits(got: &str, want: &str) -> bool {
    got == want || want.is_empty() || want == "any" || (want == "float" && got == "int") || (want == "list" && list_of(got).is_some())
}

impl Api {
    /// the global of that name ("vec", "log.warn")
    pub fn global(&self, name: &str) -> Option<&Global> {
        self.globals.iter().find(|g| g.name == name)
    }

    /// the type of that name
    pub fn get_type(&self, name: &str) -> Option<&Type> {
        self.types.iter().find(|t| t.name == name)
    }

    /// the roots of the global names ("log" for log.warn), each once, in order: the names to
    /// give script::compile_with (and Vm::set_global for the values)
    pub fn names(&self) -> Vec<&str> {
        let mut v: Vec<&str> = Vec::new();
        for g in &self.globals {
            let root = g.name.split('.').next().unwrap_or(&g.name);
            if !v.contains(&root) {
                v.push(root);
            }
        }
        v
    }

    /// whether name is a namespace: declared as one, or the root of dotted names
    pub(crate) fn is_namespace(&self, name: &str) -> bool {
        self.globals.iter().any(|g| (g.name == name && g.kind == GlobalKind::Namespace) || (g.name.len() > name.len() && g.name.starts_with(name) && g.name.as_bytes()[name.len()] == b'.'))
    }

    /// the result of a op b for these operand types, from the operators of either type
    pub(crate) fn op_result(&self, op: Operator, a: &str, b: &str) -> Option<&str> {
        for t in [a, b] {
            let Some(ty) = self.get_type(t) else { continue };
            for o in &ty.ops {
                let hit = if op == Operator::Neg { o.op == op && fits(b, &o.rhs) } else { o.op == op && fits(a, &o.lhs) && fits(b, &o.rhs) };
                if hit {
                    return Some(o.ret.as_str());
                }
            }
        }
        None
    }

    /// what does not fit the description (German), empty when all is well: types named
    /// nowhere, names twice, operators of other types
    pub fn validate(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let known = |t: &str| -> bool {
            let t = t.trim();
            let t = list_of(t).or_else(|| promise_of(t)).unwrap_or(t);
            t.is_empty() || BUILTIN_TYPES.contains(&t) || self.get_type(t).is_some()
        };
        let mut check = |ty: &str, at: &str, out: &mut Vec<String>| {
            if !known(ty) {
                out.push(format!("{}: unbekannter Typ {}", at, ty));
            }
        };
        let sig = |s: &Sig, at: &str, out: &mut Vec<String>, check: &mut dyn FnMut(&str, &str, &mut Vec<String>)| {
            for p in &s.params {
                check(&p.ty, &format!("{} ({})", at, p.name), out);
            }
            check(&s.ret, at, out);
        };
        for (i, g) in self.globals.iter().enumerate() {
            if self.globals[..i].iter().any(|x| x.name == g.name) {
                out.push(format!("{}: der Name kommt zweimal vor", g.name));
            }
            match &g.kind {
                GlobalKind::Func(s) => sig(s, &g.name, &mut out, &mut check),
                GlobalKind::Value(t) => check(t, &g.name, &mut out),
                GlobalKind::Namespace => {}
            }
        }
        for (i, t) in self.types.iter().enumerate() {
            if self.types[..i].iter().any(|x| x.name == t.name) {
                out.push(format!("{}: der Typ kommt zweimal vor", t.name));
            }
            if BUILTIN_TYPES.contains(&t.name.as_str()) {
                out.push(format!("{}: ein Name der Sprache, kein Typ des Hosts", t.name));
            }
            let mut names: Vec<&str> = Vec::new();
            for n in t.props.iter().map(|p| p.name.as_str()).chain(t.methods.iter().map(|m| m.name.as_str())) {
                if names.contains(&n) {
                    out.push(format!("{}.{}: der Name kommt zweimal vor", t.name, n));
                }
                names.push(n);
                if n == "on" || n == "off" {
                    out.push(format!("{}.{}: on und off sind die Ereignisse der VM", t.name, n));
                }
            }
            for p in &t.props {
                check(&p.ty, &format!("{}.{}", t.name, p.name), &mut out);
            }
            for m in &t.methods {
                sig(&m.sig, &format!("{}.{}", t.name, m.name), &mut out, &mut check);
            }
            for e in &t.events {
                check(&e.ty, &format!("{} Ereignis {}", t.name, e.name), &mut out);
            }
            for o in &t.ops {
                let at = format!("{} {}", t.name, o.label());
                if o.lhs != t.name && o.rhs != t.name {
                    out.push(format!("{}: keiner der Operanden ist {}", at, t.name));
                }
                for x in [&o.lhs, &o.rhs, &o.ret] {
                    check(x, &at, &mut out);
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------- the catalogue

impl Api {
    /// the object catalogue (G8.3): the global names (a namespace with its dotted members as
    /// children), then the types with their properties, methods, events and operators; names
    /// in alphabetical order, operators as described
    pub fn catalogue(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = Vec::new();
        let mut globals: Vec<&Global> = self.globals.iter().collect();
        globals.sort_by(|a, b| a.name.cmp(&b.name));
        let entry = |g: &Global| -> Entry {
            let (kind, label) = match &g.kind {
                GlobalKind::Func(s) => (EntryKind::Function, s.label(&g.name)),
                GlobalKind::Value(t) if t.is_empty() => (EntryKind::Value, g.name.clone()),
                GlobalKind::Value(t) => (EntryKind::Value, format!("{}: {}", g.name, t)),
                GlobalKind::Namespace => (EntryKind::Namespace, g.name.clone()),
            };
            Entry { kind, name: g.name.clone(), label, doc: g.doc.clone(), children: Vec::new() }
        };
        for g in &globals {
            if g.name.contains('.') {
                continue;
            }
            let mut e = entry(g);
            let pre = format!("{}.", g.name);
            for m in globals.iter().filter(|m| m.name.starts_with(&pre)) {
                e.children.push(entry(m));
            }
            if !e.children.is_empty() && e.kind == EntryKind::Value {
                e.kind = EntryKind::Namespace;
            }
            out.push(e);
        }
        // dotted names without their namespace
        for g in &globals {
            if let Some((root, _)) = g.name.split_once('.')
                && !globals.iter().any(|x| x.name == root)
            {
                match out.iter_mut().find(|e| e.name == root) {
                    Some(e) => e.children.push(entry(g)),
                    None => out.push(Entry { kind: EntryKind::Namespace, name: String::from(root), label: String::from(root), doc: String::new(), children: alloc::vec![entry(g)] }),
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        let mut types: Vec<&Type> = self.types.iter().collect();
        types.sort_by(|a, b| a.name.cmp(&b.name));
        for t in types {
            let mut kids: Vec<Entry> = Vec::new();
            let mut props: Vec<&Prop> = t.props.iter().collect();
            props.sort_by(|a, b| a.name.cmp(&b.name));
            for p in props {
                kids.push(Entry { kind: EntryKind::Property, name: p.name.clone(), label: p.label(), doc: p.doc.clone(), children: Vec::new() });
            }
            let mut methods: Vec<&Method> = t.methods.iter().collect();
            methods.sort_by(|a, b| a.name.cmp(&b.name));
            for m in methods {
                kids.push(Entry { kind: EntryKind::Method, name: m.name.clone(), label: m.sig.label(&m.name), doc: m.doc.clone(), children: Vec::new() });
            }
            let mut events: Vec<&Event> = t.events.iter().collect();
            events.sort_by(|a, b| a.name.cmp(&b.name));
            for e in events {
                kids.push(Entry { kind: EntryKind::Event, name: e.name.clone(), label: e.label(), doc: e.doc.clone(), children: Vec::new() });
            }
            for o in &t.ops {
                kids.push(Entry { kind: EntryKind::Operator, name: String::from(o.op.text()), label: o.label(), doc: o.doc.clone(), children: Vec::new() });
            }
            let kind = if t.value { EntryKind::ValueType } else { EntryKind::Type };
            out.push(Entry { kind, name: t.name.clone(), label: t.name.clone(), doc: t.doc.clone(), children: kids });
        }
        out
    }

    /// the catalogue as a Markdown page: the title, the description, "Globale Namen", then a
    /// section per type with its properties, methods, events and operators (German headings;
    /// the texts as the host wrote them)
    pub fn markdown(&self) -> String {
        let mut o = String::new();
        o.push_str(&format!("# {}\n\n", if self.name.is_empty() { "Objektkatalog" } else { &self.name }));
        if !self.doc.is_empty() {
            o.push_str(&format!("{}\n\n", self.doc));
        }
        let cat = self.catalogue();
        let line = |o: &mut String, e: &Entry, indent: &str| {
            o.push_str(&format!("{}- `{}`", indent, e.label));
            if !e.doc.is_empty() {
                o.push_str(&format!(" – {}", e.doc.replace('\n', " ")));
            }
            o.push('\n');
        };
        let globals: Vec<&Entry> = cat.iter().filter(|e| matches!(e.kind, EntryKind::Function | EntryKind::Value | EntryKind::Namespace)).collect();
        if !globals.is_empty() {
            o.push_str("## Globale Namen\n\n");
            for e in globals {
                line(&mut o, e, "");
                for c in &e.children {
                    line(&mut o, c, "  ");
                }
            }
            o.push('\n');
        }
        let types: Vec<&Entry> = cat.iter().filter(|e| matches!(e.kind, EntryKind::Type | EntryKind::ValueType)).collect();
        if !types.is_empty() {
            o.push_str("## Typen\n\n");
        }
        for t in types {
            let what = if t.kind == EntryKind::ValueType { " (Werttyp: == vergleicht die Werte)" } else { "" };
            o.push_str(&format!("### {}{}\n\n", t.name, what));
            if !t.doc.is_empty() {
                o.push_str(&format!("{}\n\n", t.doc));
            }
            for (kind, head) in [(EntryKind::Property, "Eigenschaften"), (EntryKind::Method, "Methoden"), (EntryKind::Event, "Ereignisse (obj.on(name, fn(e) { .. }))"), (EntryKind::Operator, "Operatoren")] {
                let kids: Vec<&Entry> = t.children.iter().filter(|c| c.kind == kind).collect();
                if kids.is_empty() {
                    continue;
                }
                o.push_str(&format!("{}:\n\n", head));
                for c in kids {
                    line(&mut o, c, "");
                }
                o.push('\n');
            }
        }
        while o.ends_with("\n\n") {
            o.pop();
        }
        o
    }
}
