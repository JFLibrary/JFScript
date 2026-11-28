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
//   api.validate()         what does not fit: an unknown type, a name twice (texts in the
//                          language of set_language)
//   api.catalogue()        the object catalogue as a tree of Entry, for a UI (jfscript-ide later)
//   api.markdown()         the same as a Markdown page: the app's reference
//   Api::from_items(&items)   the description of today's ls::HostItem list (no types)
//   (S3) api.sigs(name), ty.sigs(name)   the forms (overloads) of a function or method
//   (S3) bind(&forms, &args) -> Binding   which form fits a call and which parameter each
//                          argument fills (Sig::bind for one form; see "Binding" below);
//                          Sig::arity() the fewest and most arguments, count_warning(..) the
//                          message for a number that fits no form; bind_lenient and
//                          named_message for calls with named arguments (the VM's binding)
//   (S3) .removed(name, hint)   a name that is gone and what to take instead (Api and Type):
//                          "geom.move wurde entfernt – nimm m.translate"
//   (S3) .check_calls()    the signatures name every form a call may take: the language
//                          services warn about a call whose arguments fit none of them
//   (S3) .map_docs(f)      every description through the host's own translation (G9.2); the
//                          texts JFScript adds (headings, "Eigenschaft von ..") follow
//                          script::set_language
//
// Types are text: int float str bool bytes list map fn nil any, (S3) number (int or float), the
// name of a type of the description, list<T> (a list of T: `for x in` and `l[i]` give T), (S3)
// promise and promise<T> (what a long operation returns: `await` gives a T; task.rs), (S3) a
// union "number | vec" of those (also in properties and results; "vec | nil" counts as vec where
// a member is looked up); anything else is shown as written and means nothing to the services.
// Signatures are written as in TypeScript (G1 d, G3): "name(a: T, b?: T, c: T = default,
// ...rest: T): R"; a parameter without a type is any, a function without ": R" returns nothing
// known.  (S3) A parameter with `?` or a default may be left out (optional); the others may not
// (the services count that only with check_calls: before S3 a missing argument was simply nil,
// and descriptions of that time name the longest form).  Several signatures of one name are its
// forms (overloads, as TypeScript writes them): .func("vec(x: float, y: float, z: float): vec",
// ..).func("vec(v: vec | list): vec", ..); each has its own description, the first one is what
// global() and find_method() give.  The builders never fail: what does not parse is kept as text;
// validate() tells.
//
// Binding (S3): a call's arguments (Arg: by position or, with the named arguments of S3, by name;
// the type as type(x) gives it or "" when not known) fill a form's parameters thus:
//   1. the arguments by position fill the parameters in order; a rest parameter takes every
//      one from its place on; more of them than parameters: TooMany
//   2. a named argument fills the parameter of its name: none of that name (or a rest
//      parameter): NoParam; a parameter that already has one: Twice
//   3. a parameter that may not be left out (no `?`, no default, no rest) without one: Missing
//   4. every argument's type fits its parameter's (fits(): int fits float and number, list fits
//      list<T>, a union takes each of its types, "" and any fit all; nil fits an optional
//      parameter, nil being what a left-out argument is): else Type
// bind() takes the first form, in the order the host declared them, that fits; Err gives each
// form's first mismatch.  A call by position reaches a host function as the script wrote it,
// and a host binds when it wants to (to pick an overload: Binding::arg("width")).  (S3) A call
// with named arguments (`box(1, d: 3)`) is bound by the VM: bind_lenient takes the first form
// that fits (bind), else the first that takes the arguments by position and name alone
// (Sig::bind_names: rules 1 and 2) and leaves out no parameter that may not be left out (rule
// 3 holds for a host's forms: scale(sx: 2) of scale(sx, sy, sz) is an error, not scale(2)),
// and Host::call gets the values by position in that form's order, nil for a parameter left
// out before the last one given, a rest parameter's in their place (box(w: 2) is box(2) for
// box(w, h = 1, d?), box(1, d: 3) is box(1, nil, 3)); named_message says why none fits.  The
// compiler checks the same where it sees the callee (check.rs).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ls::HostItem;
use crate::vm::Operator;

/// the types every script knows (not described by a host); number is int or float
pub(crate) const BUILTIN_TYPES: &[&str] = &["int", "float", "str", "bool", "bytes", "list", "map", "fn", "nil", "any", "number", "promise"];

/// a host's object model (see the module comment)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Api {
    /// the app's name, the catalogue's title ("modeltool")
    pub name: String,
    pub doc: String,
    /// (a name declared several times as a function: its forms, see the module comment)
    pub globals: Vec<Global>,
    pub types: Vec<Type>,
    /// (S3) global names that are gone ("geom.move") with what to take instead
    pub removed: Vec<Removed>,
    /// (S3) the signatures name every form a call may take (overloads, optional parameters):
    /// ls::check and script::compile_api warn about a call of a host function or method whose
    /// arguments fit none of them (by number)
    pub check_calls: bool,
}

/// a global name of the host
#[derive(Clone, Debug, PartialEq)]
pub struct Global {
    /// "vec", "doc", "log.warn" (a dotted name is a member of a namespace)
    pub name: String,
    pub kind: GlobalKind,
    pub doc: String,
}

/// (S3) a name that is gone and the text of what to take instead ("m.translate")
#[derive(Clone, Debug, PartialEq)]
pub struct Removed {
    pub name: String,
    pub hint: String,
}

impl Removed {
    /// "geom.move wurde entfernt – nimm m.translate" (shown: the name as the message names it)
    pub fn message(&self, shown: &str) -> String {
        if self.hint.is_empty() { tf!("{} wurde entfernt", shown) } else { tf!("{} wurde entfernt – nimm {}", shown, self.hint) }
    }
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
    /// "" any; a union as "number | vec"
    pub ty: String,
    /// the default as written ("0", "\"x\""); None: none (a missing argument is nil)
    pub default: Option<String>,
    /// ...name: any number of further arguments
    pub rest: bool,
    /// (S3) a call may leave it out: written `name?: T`, or with a default
    pub optional: bool,
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
    /// (a name declared several times: its forms, see the module comment)
    pub methods: Vec<Method>,
    pub events: Vec<Event>,
    pub ops: Vec<OpDesc>,
    /// (S3) members that are gone ("move") with what to take instead
    pub removed: Vec<Removed>,
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
    /// the word in the current language ("Funktion", "Eigenschaft" ..; msg.rs)
    pub fn word(self) -> &'static str {
        match self {
            EntryKind::Function => t!("Funktion"),
            EntryKind::Value => t!("Wert"),
            EntryKind::Namespace => t!("Namensraum"),
            EntryKind::Type => t!("Typ"),
            EntryKind::ValueType => t!("Werttyp"),
            EntryKind::Property => t!("Eigenschaft"),
            EntryKind::Method => t!("Methode"),
            EntryKind::Event => t!("Ereignis"),
            EntryKind::Operator => t!("Operator"),
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

/// a type as written, a union with " | " between its types ("number|vec" -> "number | vec")
fn norm_type(t: &str) -> String {
    let t = t.trim();
    let parts = split_top(t, '|');
    if parts.len() < 2 {
        return String::from(t);
    }
    let parts: Vec<&str> = parts.into_iter().map(str::trim).collect();
    parts.join(" | ")
}

/// "a: T = d", "b?: T", "...rest: T", "x", ".."
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
    let (name, q) = match name.strip_suffix('?') {
        Some(n) => (n.trim_end(), true),
        None => (name, false),
    };
    let optional = !rest && (q || default.is_some());
    Param { name: String::from(name), ty: norm_type(ty), default, rest, optional }
}

/// "a: T, b" (the text inside the parentheses)
pub(crate) fn parse_params(s: &str) -> Vec<Param> {
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
            Some(i) => (String::from(s[..i].trim()), Sig { params: Vec::new(), ret: norm_type(&s[i + 1..]) }),
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
    (name, Sig { params, ret: norm_type(ret) })
}

/// "readonly x: T" -> (readonly, name, T)
fn parse_typed(s: &str) -> (bool, String, String) {
    let s = s.trim();
    let (ro, s) = match s.strip_prefix("readonly ") {
        Some(r) => (true, r.trim()),
        None => (false, s),
    };
    match find_top(s, ':') {
        Some(i) => (ro, String::from(s[..i].trim()), norm_type(&s[i + 1..])),
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
    /// "a: T = d", "b?: T", "...rest: T", ".."
    pub fn label(&self) -> String {
        if self.name == ".." {
            return String::from("..");
        }
        let mut s = String::new();
        if self.rest {
            s.push_str("...");
        }
        s.push_str(&self.name);
        if self.optional && self.default.is_none() && !self.rest {
            s.push('?');
        }
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

    /// (S3) a global name that is gone ("geom.move") and what to take instead ("m.translate"):
    /// the services say so where a script still uses it
    pub fn removed(mut self, name: &str, hint: &str) -> Api {
        self.removed.push(Removed { name: String::from(name.trim()), hint: String::from(hint.trim()) });
        self
    }

    /// (S3) the signatures name every form a call may take: count the arguments of calls (see
    /// Api::check_calls)
    pub fn check_calls(mut self) -> Api {
        self.check_calls = true;
        self
    }

    /// (S3, G9.2) every description of the model (the app's, the globals', the types' and their
    /// members') through f, the empty ones left out: a host that writes its descriptions in one
    /// language gives the user's with its own catalogue, `api.map_docs(|d| tr(d))`, and makes the
    /// description again when the language changes.  Names, types and signatures stay as they
    /// are (G9.3)
    pub fn map_docs(mut self, mut f: impl FnMut(&str) -> String) -> Api {
        let mut map = |d: &mut String| {
            if !d.is_empty() {
                *d = f(d);
            }
        };
        map(&mut self.doc);
        self.globals.iter_mut().for_each(|g| map(&mut g.doc));
        for t in &mut self.types {
            map(&mut t.doc);
            t.props.iter_mut().for_each(|p| map(&mut p.doc));
            t.methods.iter_mut().for_each(|m| map(&mut m.doc));
            t.events.iter_mut().for_each(|e| map(&mut e.doc));
            t.ops.iter_mut().for_each(|o| map(&mut o.doc));
        }
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

    /// (S3) a member that is gone ("move") and what to take instead ("translate")
    pub fn removed(mut self, name: &str, hint: &str) -> Type {
        self.removed.push(Removed { name: String::from(name.trim()), hint: String::from(hint.trim()) });
        self
    }

    /// the property of that name
    pub fn find_prop(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.name == name)
    }

    /// the method of that name (its first form)
    pub fn find_method(&self, name: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.name == name)
    }

    /// (S3) every form of the method of that name, in the order they were declared
    pub fn find_methods(&self, name: &str) -> Vec<&Method> {
        self.methods.iter().filter(|m| m.name == name).collect()
    }

    /// (S3) the signatures of the method's forms (for bind)
    pub fn sigs(&self, name: &str) -> Vec<&Sig> {
        self.methods.iter().filter(|m| m.name == name).map(|m| &m.sig).collect()
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

/// (S3) the types of a union ("number | vec" -> number, vec); a type that is none: itself
pub fn alternatives(ty: &str) -> Vec<&str> {
    split_top(ty, '|').into_iter().map(str::trim).collect()
}

/// (S3) the type whose members a value of type ty has: ty itself, or the one type of a union
/// besides nil ("vec | nil" -> vec); another union has none ("")
pub(crate) fn member_type(ty: &str) -> &str {
    let alts = alternatives(ty);
    if alts.len() < 2 {
        return ty.trim();
    }
    let rest: Vec<&str> = alts.into_iter().filter(|t| *t != "nil").collect();
    if rest.len() == 1 { rest[0] } else { "" }
}

/// a value of type `got` fits a parameter or operand of type `want`: the same type, any or ""
/// on either side (not known), int for float or number, float for number, a list for a list<T>
/// and the other way round, (S3) a union of `want` when it fits one of its types, a union as
/// `got` when each of its types fits
pub fn fits(got: &str, want: &str) -> bool {
    let (got, want) = (got.trim(), want.trim());
    if got == want || want.is_empty() || want == "any" || got.is_empty() || got == "any" {
        return true;
    }
    let ga = alternatives(got);
    if ga.len() > 1 {
        return ga.iter().all(|g| fits(g, want));
    }
    if got == "number" {
        return fits("int", want) && fits("float", want);
    }
    let wa = alternatives(want);
    if wa.len() > 1 {
        return wa.iter().any(|w| fits(got, w));
    }
    match (got, want) {
        ("int", "float") | ("int" | "float", "number") => true,
        _ => match (list_of(got), list_of(want)) {
            (Some(g), Some(w)) => fits(g, w),
            (Some(_), None) => want == "list",
            (None, Some(_)) => got == "list",
            (None, None) => false,
        },
    }
}

// ---------------------------------------------------------------- binding a call (S3)

/// an argument of a call as bind() sees it (see the module comment)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Arg<'a> {
    /// the name of a named argument (`width: 2`); None: by position
    pub name: Option<&'a str>,
    /// its type: as type(x) gives it ("int", "vec"), a type of the description, or "" (not
    /// known: it fits every parameter)
    pub ty: &'a str,
}

impl<'a> Arg<'a> {
    /// an argument by position
    pub fn pos(ty: &'a str) -> Arg<'a> {
        Arg { name: None, ty }
    }

    /// a named argument
    pub fn named(name: &'a str, ty: &'a str) -> Arg<'a> {
        Arg { name: Some(name), ty }
    }
}

/// why a form does not fit a call (bind; the indexes are the call's arguments and the form's
/// parameters)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// more arguments by position than the form has parameters (and none of them is a rest
    /// parameter): it takes at most `most`
    TooMany { most: usize },
    /// a parameter that may not be left out got no argument
    Missing { param: usize },
    /// a named argument names no parameter of the form (or its rest parameter)
    NoParam { arg: usize },
    /// a parameter got a second argument: by position and by name, or named twice
    Twice { arg: usize, param: usize },
    /// an argument's type does not fit its parameter's
    Type { arg: usize, param: usize },
}

/// which form fits a call and which parameter each argument fills (bind)
#[derive(Clone, Debug, PartialEq)]
pub struct Binding<'s> {
    /// the form's index among those given to bind
    pub form: usize,
    pub sig: &'s Sig,
    /// for each parameter of sig, the arguments that fill it (indexes into the call's
    /// arguments): none when the call left it out (its default, else nil), one, or for a rest
    /// parameter any number
    pub params: Vec<Vec<usize>>,
}

impl Binding<'_> {
    /// the argument that fills the parameter of that name (None: left out, or no such
    /// parameter); a rest parameter: its first
    pub fn arg(&self, name: &str) -> Option<usize> {
        self.args(name).first().copied()
    }

    /// the arguments that fill the parameter of that name (a rest parameter's, in order)
    pub fn args(&self, name: &str) -> &[usize] {
        match self.sig.params.iter().position(|p| p.name == name) {
            Some(i) => &self.params[i],
            None => &[],
        }
    }
}

impl Sig {
    /// (S3) the fewest and the most arguments a call may give (None: any number, a rest
    /// parameter)
    pub fn arity(&self) -> (usize, Option<usize>) {
        let min = self.params.iter().filter(|p| !p.optional && !p.rest).count();
        let max = if self.params.iter().any(|p| p.rest) { None } else { Some(self.params.len()) };
        (min, max)
    }

    /// (S3) which parameter each argument fills, by the rules of the module comment: for each
    /// parameter the arguments that fill it; Err: the first rule the call breaks
    pub fn bind(&self, args: &[Arg]) -> Result<Vec<Vec<usize>>, Mismatch> {
        let filled = self.bind_names(args)?;
        let ps = &self.params;
        // 3. what may not be left out
        if let Some(p) = (0..ps.len()).find(|&j| !ps[j].optional && !ps[j].rest && filled[j].is_empty()) {
            return Err(Mismatch::Missing { param: p });
        }
        // 4. the types
        for (j, p) in ps.iter().enumerate() {
            for &i in &filled[j] {
                let ty = args[i].ty.trim();
                if !(fits(ty, &p.ty) || (ty == "nil" && p.optional)) {
                    return Err(Mismatch::Type { arg: i, param: j });
                }
            }
        }
        Ok(filled)
    }

    /// (S3) rules 1 and 2 of bind alone: the arguments by position and by name, without asking
    /// whether a parameter is left out or what types they have (the VM's binding of a call with
    /// named arguments: a host function gets nil for what is left out and checks the rest
    /// itself, as for a call by position)
    pub fn bind_names(&self, args: &[Arg]) -> Result<Vec<Vec<usize>>, Mismatch> {
        let ps = &self.params;
        let mut filled: Vec<Vec<usize>> = alloc::vec![Vec::new(); ps.len()];
        // 1. by position
        let mut next = 0usize;
        for i in (0..args.len()).filter(|&i| args[i].name.is_none()) {
            match ps.get(next) {
                Some(p) if p.rest => filled[next].push(i),
                Some(_) => {
                    filled[next].push(i);
                    next += 1;
                }
                None => return Err(Mismatch::TooMany { most: ps.len() }),
            }
        }
        // 2. by name
        for (i, a) in args.iter().enumerate() {
            let Some(n) = a.name else { continue };
            let Some(p) = ps.iter().position(|p| p.name == n && !p.rest) else {
                return Err(Mismatch::NoParam { arg: i });
            };
            if !filled[p].is_empty() {
                return Err(Mismatch::Twice { arg: i, param: p });
            }
            filled[p].push(i);
        }
        Ok(filled)
    }
}

/// (S3) the form a call with named arguments goes to (the VM's binding, and the check of the
/// compiler): the first form that fits it (bind), else the first whose parameters take its
/// arguments by position and name (Sig::bind_names: a parameter left out is nil, the types are
/// the host's to check).  host: the forms are a host's description, which marks what may be
/// left out (`?`, a default): there a form that leaves out a parameter that may not be left out
/// is no fallback (Missing), for the host could not tell such a call from a shorter form's
/// (scale(sx: 2) is not scale(2) of scale(s)); the library's parameters (lsdoc.rs) mark nothing,
/// so there it is.  Err: the first such form's mismatch and that form (None: no forms)
pub fn bind_lenient<'s>(forms: &[&'s Sig], args: &[Arg], host: bool) -> Result<Binding<'s>, Option<(&'s Sig, Mismatch)>> {
    if let Ok(b) = bind(forms, args) {
        return Ok(b);
    }
    let mut first = None;
    for (k, s) in forms.iter().enumerate() {
        match s.bind_names(args) {
            Ok(params) => {
                let ps = &s.params;
                match (0..ps.len()).find(|&j| host && !ps[j].optional && !ps[j].rest && params[j].is_empty()) {
                    Some(param) => {
                        first.get_or_insert((*s, Mismatch::Missing { param }));
                    }
                    None => return Ok(Binding { form: k, sig: s, params }),
                }
            }
            Err(m) => {
                first.get_or_insert((*s, m));
            }
        }
    }
    Err(first)
}

/// (S3) the message for a call with named arguments that fits no form (bind_lenient's
/// mismatch and its form): "geom.box hat keinen Parameter widht – meinst du width?",
/// "geom.box: der Parameter w hat schon ein Argument", "geom.box: 4 Argumente vor den
/// benannten, nimmt aber höchstens 3"; shown: how it names the function
pub fn named_message(shown: &str, sig: &Sig, args: &[Arg], m: Mismatch) -> String {
    let name_of = |p: usize| sig.params.get(p).map_or("", |x| x.name.as_str());
    match m {
        Mismatch::NoParam { arg } => {
            let n = args.get(arg).and_then(|a| a.name).unwrap_or("");
            let mut s = tf!("{} hat keinen Parameter {}", shown, n);
            let names = sig.params.iter().filter(|p| !p.rest).map(|p| p.name.as_str());
            if let Some(c) = crate::check::suggest_member(n, names) {
                s = tf!("{} – meinst du {}?", s, c);
            }
            s
        }
        Mismatch::Twice { param, .. } => tf!("{}: der Parameter {} hat schon ein Argument", shown, name_of(param)),
        Mismatch::TooMany { most } => {
            let given = args.iter().filter(|a| a.name.is_none()).count();
            tf!("{}: {} vor den benannten, nimmt aber höchstens {}", shown, count_text(given), most)
        }
        Mismatch::Missing { param } => tf!("{}: der Parameter {} fehlt", shown, name_of(param)),
        Mismatch::Type { arg, param } => {
            let ty = args.get(arg).map_or("", |a| a.ty);
            tf!("{}: {} muss {} sein, nicht {}", shown, name_of(param), sig.params.get(param).map_or("", |p| p.ty.as_str()), ty)
        }
    }
}

/// (S3) the first of the forms (the overloads of one name in the order the host declared them:
/// Api::sigs, Type::sigs) that fits the call's arguments, and which parameter each argument
/// fills; Err: each form's first mismatch, in their order (empty for no forms)
pub fn bind<'s>(forms: &[&'s Sig], args: &[Arg]) -> Result<Binding<'s>, Vec<Mismatch>> {
    let mut why = Vec::with_capacity(forms.len());
    for (k, s) in forms.iter().enumerate() {
        match s.bind(args) {
            Ok(params) => return Ok(Binding { form: k, sig: s, params }),
            Err(m) => why.push(m),
        }
    }
    Err(why)
}

/// "kein Argument", "1 Argument", "3 Argumente"
fn count_text(n: usize) -> String {
    match n {
        0 => String::from(t!("kein Argument")),
        1 => String::from(t!("1 Argument")),
        _ => tf!("{} Argumente", n),
    }
}

/// "w, h, d", "m, ...cuts": the names of a form's parameters
fn param_names(s: &Sig) -> String {
    let ps: Vec<String> = s.params.iter().map(|p| if p.rest && p.name != ".." { format!("...{}", p.name) } else { p.name.clone() }).collect();
    ps.join(", ")
}

/// (S3) the warning of ls::check (with Api::check_calls) for a call with `given` arguments when
/// their number fits none of the forms (None: one fits, or there are none); shown: how the
/// message names the function ("geom.box", "mesh.move").  A host may give it at run time too
/// (Cx::warn): "geom.box: 4 Argumente, nimmt aber höchstens 3 (w, h, d)", "vec: 2 Argumente
/// passen zu keiner Form (vec(x, y, z), vec(v))"
pub fn count_warning(shown: &str, forms: &[&Sig], given: usize) -> Option<String> {
    let fits = |s: &&Sig| {
        let (min, max) = s.arity();
        given >= min && max.is_none_or(|m| given <= m)
    };
    if forms.is_empty() || forms.iter().any(fits) {
        return None;
    }
    if let [s] = forms {
        let (min, max) = s.arity();
        let names = param_names(s);
        return Some(match max {
            Some(0) => tf!("{}: {}, nimmt aber keine", shown, count_text(given)),
            Some(m) if given > m => tf!("{}: {}, nimmt aber höchstens {} ({})", shown, count_text(given), m, names),
            _ => tf!("{}: {}, braucht aber mindestens {} ({})", shown, count_text(given), min, names),
        });
    }
    let all: Vec<String> = forms.iter().map(|s| format!("{}({})", shown, param_names(s))).collect();
    Some(match given {
        0 => tf!("{}: ohne Argumente passt keine Form ({})", shown, all.join(", ")),
        1 => tf!("{}: 1 Argument passt zu keiner Form ({})", shown, all.join(", ")),
        n => tf!("{}: {} Argumente passen zu keiner Form ({})", shown, n, all.join(", ")),
    })
}

impl Api {
    /// the global of that name ("vec", "log.warn"); a function's first form
    pub fn global(&self, name: &str) -> Option<&Global> {
        self.globals.iter().find(|g| g.name == name)
    }

    /// (S3) every form of the global function of that name, in the order they were declared
    pub fn funcs(&self, name: &str) -> Vec<&Global> {
        self.globals.iter().filter(|g| g.name == name && matches!(g.kind, GlobalKind::Func(_))).collect()
    }

    /// (S3) the signatures of the function's forms (for bind)
    pub fn sigs(&self, name: &str) -> Vec<&Sig> {
        self.globals
            .iter()
            .filter(|g| g.name == name)
            .filter_map(|g| match &g.kind {
                GlobalKind::Func(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// (S3) the removed global name of that name
    pub fn find_removed(&self, name: &str) -> Option<&Removed> {
        self.removed.iter().find(|r| r.name == name)
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

    /// the result of a op b for these operand types, from the operators of either type (none
    /// for an operand whose type is not known)
    pub(crate) fn op_result(&self, op: Operator, a: &str, b: &str) -> Option<&str> {
        let unknown = |t: &str| t.is_empty() || t == "any";
        if unknown(b) || (op != Operator::Neg && unknown(a)) {
            return None;
        }
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

    /// what does not fit the description (msg.rs: German by default), empty when all is well: types named
    /// nowhere, names twice (forms of a function or method are no name twice, the same form
    /// twice is), a rest parameter that is not the last, operators of other types, removed
    /// names that are still described
    pub fn validate(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        fn known(api: &Api, t: &str) -> bool {
            let t = t.trim();
            if t.is_empty() {
                return true;
            }
            let alts = alternatives(t);
            if alts.len() > 1 {
                return alts.iter().all(|a| !a.is_empty() && known(api, a));
            }
            match list_of(t).or_else(|| promise_of(t)) {
                Some(inner) => known(api, inner),
                None => BUILTIN_TYPES.contains(&t) || api.get_type(t).is_some(),
            }
        }
        let mut check = |ty: &str, at: &str, out: &mut Vec<String>| {
            if !known(self, ty) {
                out.push(tf!("{}: unbekannter Typ {}", at, ty));
            }
        };
        let sig = |s: &Sig, at: &str, out: &mut Vec<String>, check: &mut dyn FnMut(&str, &str, &mut Vec<String>)| {
            for (i, p) in s.params.iter().enumerate() {
                check(&p.ty, &format!("{} ({})", at, p.name), out);
                if p.rest && i + 1 < s.params.len() {
                    out.push(tf!("{}: ...{} muss der letzte Parameter sein", at, p.name));
                }
            }
            check(&s.ret, at, out);
        };
        for (i, g) in self.globals.iter().enumerate() {
            let before: Vec<&Global> = self.globals[..i].iter().filter(|x| x.name == g.name).collect();
            if !before.is_empty() {
                match &g.kind {
                    // another form of a function
                    GlobalKind::Func(s) if before.iter().all(|x| matches!(x.kind, GlobalKind::Func(_))) => {
                        if before.iter().any(|x| x.kind == g.kind) {
                            out.push(tf!("{}: die Form {} kommt zweimal vor", g.name, s.label(&g.name)));
                        }
                    }
                    _ => out.push(tf!("{}: der Name kommt zweimal vor", g.name)),
                }
            }
            match &g.kind {
                GlobalKind::Func(s) => sig(s, &g.name, &mut out, &mut check),
                GlobalKind::Value(t) => check(t, &g.name, &mut out),
                GlobalKind::Namespace => {}
            }
        }
        for r in &self.removed {
            if self.global(&r.name).is_some() || self.is_namespace(&r.name) {
                out.push(tf!("{}: ist als entfernt angegeben und zugleich beschrieben", r.name));
            }
        }
        for (i, t) in self.types.iter().enumerate() {
            if self.types[..i].iter().any(|x| x.name == t.name) {
                out.push(tf!("{}: der Typ kommt zweimal vor", t.name));
            }
            if BUILTIN_TYPES.contains(&t.name.as_str()) {
                out.push(tf!("{}: ein Name der Sprache, kein Typ des Hosts", t.name));
            }
            // (name, the method's index; None: a property)
            let mut names: Vec<(&str, Option<usize>)> = Vec::new();
            let members = t.props.iter().map(|p| (p.name.as_str(), None)).chain(t.methods.iter().enumerate().map(|(k, m)| (m.name.as_str(), Some(k))));
            for (n, m) in members {
                let before: Vec<Option<usize>> = names.iter().filter(|x| x.0 == n).map(|x| x.1).collect();
                match m {
                    // another form of a method
                    Some(k) if !before.is_empty() && before.iter().all(|b| b.is_some()) => {
                        if before.iter().flatten().any(|&b| t.methods[b].sig == t.methods[k].sig) {
                            out.push(tf!("{}.{}: die Form {} kommt zweimal vor", t.name, n, t.methods[k].sig.label(n)));
                        }
                    }
                    _ if !before.is_empty() => out.push(tf!("{}.{}: der Name kommt zweimal vor", t.name, n)),
                    _ if n == "on" || n == "off" => out.push(tf!("{}.{}: on und off sind die Ereignisse der VM", t.name, n)),
                    _ => {}
                }
                names.push((n, m));
            }
            for r in &t.removed {
                if t.find_prop(&r.name).is_some() || t.find_method(&r.name).is_some() {
                    out.push(tf!("{}.{}: ist als entfernt angegeben und zugleich beschrieben", t.name, r.name));
                }
            }
            for p in &t.props {
                check(&p.ty, &format!("{}.{}", t.name, p.name), &mut out);
            }
            for m in &t.methods {
                sig(&m.sig, &format!("{}.{}", t.name, m.name), &mut out, &mut check);
            }
            for e in &t.events {
                check(&e.ty, &tf!("{} Ereignis {}", t.name, e.name), &mut out);
            }
            for o in &t.ops {
                let at = format!("{} {}", t.name, o.label());
                if o.lhs != t.name && o.rhs != t.name {
                    out.push(tf!("{}: keiner der Operanden ist {}", at, t.name));
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
    /// in alphabetical order, (S3) the forms of a function or method one entry each in their
    /// order, operators as described
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
        for (i, g) in globals.iter().enumerate() {
            if g.name.contains('.') {
                continue;
            }
            let mut e = entry(g);
            let pre = format!("{}.", g.name);
            // (the dotted members under the first entry of the name, not under each form)
            let first = !globals[..i].iter().any(|x| x.name == g.name);
            for m in globals.iter().filter(|m| first && m.name.starts_with(&pre)) {
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
    /// section per type with its properties, methods, events and operators (headings in the
    /// language of set_language; the texts as the host wrote them)
    pub fn markdown(&self) -> String {
        let mut o = String::new();
        o.push_str(&format!("# {}\n\n", if self.name.is_empty() { t!("Objektkatalog") } else { &self.name }));
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
            o.push_str(&format!("## {}\n\n", t!("Globale Namen")));
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
            o.push_str(&format!("## {}\n\n", t!("Typen")));
        }
        for t in types {
            if t.kind == EntryKind::ValueType {
                o.push_str(&format!("### {} ({})\n\n", t.name, t!("Werttyp: == vergleicht die Werte")));
            } else {
                o.push_str(&format!("### {}\n\n", t.name));
            }
            if !t.doc.is_empty() {
                o.push_str(&format!("{}\n\n", t.doc));
            }
            let heads = [
                (EntryKind::Property, t!("Eigenschaften")),
                (EntryKind::Method, t!("Methoden")),
                (EntryKind::Event, t!("Ereignisse (obj.on(name, fn(e) { .. }))")),
                (EntryKind::Operator, t!("Operatoren")),
            ];
            for (kind, head) in heads {
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
