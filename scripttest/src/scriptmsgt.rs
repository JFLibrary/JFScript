// scriptmsgt -- (S3, G9 of docs/RICHTUNG.md) the messages in the host's language: the table
// script/locales/messages.tsv against the sources of the crate (every key of t!, tf! and de! has
// a row with an English cell and the same placeholders, every row is used, a literal that reads
// like a German text outside the macros fails), then messages, warnings and the texts of the
// language services in English, and German again as the default.
//
// The check reads the sources as they were compiled (include_str!): a new module of script/
// belongs in SOURCES, and lib.rs's `mod` lines must match it.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use script::api::{EntryKind, Type};
use script::ls::{self, CompleteCx};
use script::{Api, Cx, Fail, Host, Limits, Status, Value, Vm};

use crate::T;

/// the sources of the crate jfscript
const SOURCES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../script/lib.rs")),
    ("api.rs", include_str!("../../script/api.rs")),
    ("ast.rs", include_str!("../../script/ast.rs")),
    ("awaitchk.rs", include_str!("../../script/awaitchk.rs")),
    ("check.rs", include_str!("../../script/check.rs")),
    ("code.rs", include_str!("../../script/code.rs")),
    ("compile.rs", include_str!("../../script/compile.rs")),
    ("debug.rs", include_str!("../../script/debug.rs")),
    ("fmt.rs", include_str!("../../script/fmt.rs")),
    ("heap.rs", include_str!("../../script/heap.rs")),
    ("intdiv.rs", include_str!("../../script/intdiv.rs")),
    ("jsonlib.rs", include_str!("../../script/jsonlib.rs")),
    ("lex.rs", include_str!("../../script/lex.rs")),
    ("listlib.rs", include_str!("../../script/listlib.rs")),
    ("ls.rs", include_str!("../../script/ls.rs")),
    ("lsapi.rs", include_str!("../../script/lsapi.rs")),
    ("lsdoc.rs", include_str!("../../script/lsdoc.rs")),
    ("math.rs", include_str!("../../script/math.rs")),
    ("module.rs", include_str!("../../script/module.rs")),
    ("msg.rs", include_str!("../../script/msg.rs")),
    ("num.rs", include_str!("../../script/num.rs")),
    ("parse.rs", include_str!("../../script/parse.rs")),
    ("stdlib.rs", include_str!("../../script/stdlib.rs")),
    ("strlib.rs", include_str!("../../script/strlib.rs")),
    ("task.rs", include_str!("../../script/task.rs")),
    ("vm.rs", include_str!("../../script/vm.rs")),
];

const TABLE: &str = include_str!("../../script/locales/messages.tsv");

/// literals that read like German but are no messages: the text of a promise value (str(p) and
/// print give the same bits under every host, as the names do)
const NOT_MESSAGES: &[(&str, &str)] = &[("task.rs", "ausstehend"), ("task.rs", "erfüllt"), ("task.rs", "abgelehnt")];

/// keys the German test below cannot tell from English (the self-check of looks_german skips
/// them)
const NEUTRAL_KEYS: &[&str] = &["in {} ({})", "Operator", "Parameter"];

/// composed() in German: what these messages were before the table (G9.1: unchanged without a
/// language)
const COMPOSED_DE: &[&str] = &[
    "1:6: Fehler: Unerwartetes Zeichen & (für „und“ schreibe &&)",
    "1:5: Fehler: Unerwartetes Zeichen $ (${…} gibt es nur in Zeichenketten)",
    "2:4: Fehler: Hier fehlt eine schließende Klammer ) (geöffnet in Zeile 1)",
    "1:1: Warnung: Unbekannter Name prnt – meinst du print?",
    "3:5: Warnung: box hat keinen Parameter widht – meinst du width?",
    "4:1: Warnung: Unbekannter Name xx – eine neue Variable wird mit let xx = … angelegt",
    "m.jfs:1:5: Nach let fehlt ein Name (und 1 weitere Fehler)",
    "2:3: Warnung: vec hat keine Methode lenght – meinst du length?",
    "3:3: Warnung: vec hat keine Eigenschaft y",
    "4:3: Warnung: vec.x ist nur lesbar",
    "5:3: Warnung: length ist eine Methode von vec, keine Eigenschaft – mit Klammern: length(..)",
    "6:3: Warnung: x ist eine Eigenschaft von vec, keine Methode – ohne Klammern: .x",
    "7:3: Warnung: vec hat keine Ereignisse",
    "# geo",
    "## Globale Namen",
    "## Typen",
    "### vec (Werttyp: == vergleicht die Werte)",
    "Eigenschaften:",
    "Methoden:",
];

/// composed() in English
const COMPOSED_EN: &[&str] = &[
    "1:6: error: Unexpected character & (use && for “and”)",
    "1:5: error: Unexpected character $ (${…} only exists inside strings)",
    "2:4: error: Expected a closing parenthesis ) (opened on line 1)",
    "1:1: warning: Unknown name prnt – did you mean print?",
    "3:5: warning: box has no parameter widht – did you mean width?",
    "4:1: warning: Unknown name xx – a new variable is created with let xx = …",
    "m.jfs:1:5: Expected a name after let (and 1 more)",
    "2:3: warning: vec has no method lenght – did you mean length?",
    "3:3: warning: vec has no property y",
    "4:3: warning: vec.x is read-only",
    "5:3: warning: length is a method of vec, not a property – with parentheses: length(..)",
    "6:3: warning: x is a property of vec, not a method – without parentheses: .x",
    "7:3: warning: vec has no events",
    "# geo",
    "## Global names",
    "## Types",
    "### vec (value type: == compares the values)",
    "Properties:",
    "Methods:",
];

pub fn run(t: &mut T) {
    table(t);
    t.eq("composed: German without a language", composed(), COMPOSED_DE.iter().map(|s| String::from(*s)).collect::<Vec<String>>());
    english(t);
    script::set_language("");
    t.eq("composed: German again", composed(), COMPOSED_DE.iter().map(|s| String::from(*s)).collect::<Vec<String>>());
}

// ---------------------------------------------------------------- the sources

/// a string literal of the sources: where, its value, the macro it is the key of
struct Lit {
    file: &'static str,
    line: u32,
    value: String,
    mac: Option<&'static str>,
}

fn is_ident(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// the string literals of a Rust source outside comments, with the macro (t, tf, de) whose first
/// argument each one is
fn literals(file: &'static str, src: &'static str) -> Vec<Lit> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 1u32);
    while i < b.len() {
        let c = b[i];
        let prev = if i > 0 { b[i - 1] } else { b' ' };
        if c == b'\n' {
            line += 1;
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut depth = 0;
            while i < b.len() {
                if b[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    line += (b[i] == b'\n') as u32;
                    i += 1;
                }
            }
        } else if c == b'\'' {
            // a char literal or a lifetime
            if b.get(i + 1) == Some(&b'\\') {
                i += 2;
                while i < b.len() && b[i] != b'\'' {
                    i += 1;
                }
                i += 1;
            } else {
                let ch = src[i + 1..].chars().next().map_or(1, |c| c.len_utf8());
                i += if b.get(i + 1 + ch) == Some(&b'\'') { ch + 2 } else { 1 };
            }
        } else if (c == b'r' || c == b'b') && !is_ident(prev) && raw_start(&b[i..]).is_some() {
            // r"..", r#".."#, b"..", br".." (no messages: skipped)
            let (open, hashes, raw) = raw_start(&b[i..]).unwrap_or((1, 0, false));
            i += open;
            if !raw {
                // b"..": escapes as in a normal string
                while i < b.len() && b[i] != b'"' {
                    line += (b[i] == b'\n') as u32;
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            } else {
                let mut close = alloc::vec![b'"'];
                close.extend(core::iter::repeat_n(b'#', hashes));
                while i < b.len() && !b[i..].starts_with(&close) {
                    line += (b[i] == b'\n') as u32;
                    i += 1;
                }
                i += close.len();
            }
        } else if c == b'"' {
            let start = i + 1;
            let first = line;
            i = start;
            while i < b.len() && b[i] != b'"' {
                line += (b[i] == b'\n') as u32;
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            out.push(Lit { file, line: first, value: unescape(&src[start..i]), mac: macro_before(b, start - 1) });
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// at b" r" r#" br" br#": (the length up to the text, the number of #, raw)
fn raw_start(b: &[u8]) -> Option<(usize, usize, bool)> {
    if b.starts_with(b"b\"") {
        return Some((2, 0, false));
    }
    let k = if b.starts_with(b"br") {
        2
    } else if b.starts_with(b"r") {
        1
    } else {
        return None;
    };
    let mut h = 0;
    while b.get(k + h) == Some(&b'#') {
        h += 1;
    }
    (b.get(k + h) == Some(&b'"')).then_some((k + h + 1, h, true))
}

/// t, tf or de when the literal opening at q is the first argument of that macro
fn macro_before(b: &[u8], q: usize) -> Option<&'static str> {
    let mut j = q;
    let back = |j: &mut usize| {
        while *j > 0 && b[*j - 1].is_ascii_whitespace() {
            *j -= 1;
        }
    };
    back(&mut j);
    if j == 0 || b[j - 1] != b'(' {
        return None;
    }
    j -= 1;
    back(&mut j);
    if j == 0 || b[j - 1] != b'!' {
        return None;
    }
    j -= 1;
    let end = j;
    while j > 0 && is_ident(b[j - 1]) {
        j -= 1;
    }
    match &b[j..end] {
        b"t" => Some("t"),
        b"tf" => Some("tf"),
        b"de" => Some("de"),
        _ => None,
    }
}

/// the value of a Rust string literal's text
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('x') => {
                let h: String = it.by_ref().take(2).collect();
                out.push(u8::from_str_radix(&h, 16).map_or('?', char::from));
            }
            Some('u') => {
                let mut h = String::new();
                for c in it.by_ref() {
                    match c {
                        '{' => {}
                        '}' => break,
                        c => h.push(c),
                    }
                }
                out.push(u32::from_str_radix(&h, 16).ok().and_then(char::from_u32).unwrap_or('?'));
            }
            Some('\n') | Some('\r') => {
                while it.peek().is_some_and(|c| c.is_whitespace()) {
                    it.next();
                }
            }
            Some(c) => out.push(c),
            None => {}
        }
    }
    out
}

/// whether a text reads like German: an umlaut, ß or German quotes, or one of the words German
/// messages are made of.  The check below proves it on every key of the table
fn looks_german(s: &str) -> bool {
    const WORDS: &[&str] = &[
        "der", "die", "das", "den", "dem", "des", "ein", "eine", "einer", "eines", "einem", "einen", "kein", "keine", "keinen",
        "keiner", "keinem", "nicht", "ist", "sind", "muss", "darf", "hat", "haben", "wird", "wurde", "werden", "und", "oder", "mit",
        "von", "zu", "nach", "vor", "hier", "fehlt", "nur", "noch", "schon", "braucht", "gibt", "sich", "als", "auch", "bei",
        "bis", "ohne", "zum", "zur", "im", "am", "auf", "aus", "wenn", "dass", "sein", "seine", "erst", "etwa", "mehr", "viele",
        "Der", "Die", "Das", "Ein", "Eine", "Kein", "Keine", "Hier", "Nach", "Vor", "Im", "Es", "Zu", "Nur", "Für", "Zum",
        "Fehler", "Warnung", "Zeile", "Spalte", "Index", "Länge", "Wert", "Werte", "Typ", "Typen", "Werttyp", "Funktion",
        "Funktionen", "Methode", "Methoden", "Eigenschaft", "Eigenschaften", "Ereignis", "Ereignisse", "Modul", "Module",
        "Namensraum", "Operatoren", "Schlüssel", "Ausdruck", "Zahl", "Liste", "Variable", "Aufruf", "Aufrufe", "Ausnahme",
        "Argument", "Argumente", "Standardbibliothek", "Objektkatalog", "Schleifenvariable", "Fehlervariable", "Skript",
        "Skripte", "Name", "Namen", "Text", "Bytes", "Annahme", "Division", "Speichergrenze", "Rechenzeit", "Interner",
        "Unbekannt", "Unbekannter", "Unbekannte", "Unbekanntes", "Ungültig", "Ungültige", "Ungültiger", "Ungültiges",
        "Unerwartet", "Unerwartete", "Unerwarteter", "Unerwartetes", "Zyklischer", "Leeres", "Erwartet", "erwartet", "Diesen",
        "Umbenennen", "Listen", "Bereiche", "Vergleiche", "Ganzzahlige", "Globale", "globale", "lokale", "eingebaut", "erfüllt",
        "abgelehnt", "ausstehend", "nichts", "lesbar", "Hauptprogramm", "meinst", "du", "hoch",
    ];
    if s.chars().any(|c| "äöüÄÖÜß„“".contains(c)) {
        return true;
    }
    s.split(|c: char| !c.is_alphabetic()).any(|w| WORDS.contains(&w))
}

/// the placeholders of a format text: the positions (by order and by index) and the names it
/// fills; None for one tf! cannot fill as format! does: {} mixed with {name}, a {:spec}
fn placeholders(s: &str) -> Option<(Vec<usize>, Vec<String>)> {
    let (mut pos, mut names) = (Vec::new(), Vec::new());
    let mut next = 0;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"{{") || b[i..].starts_with(b"}}") {
            i += 2;
            continue;
        }
        if b[i] == b'{' {
            let e = s[i..].find('}').map_or(s.len(), |e| i + e);
            let inner = &s[i + 1..e.min(s.len())];
            if inner.is_empty() {
                pos.push(next);
                next += 1;
            } else if let Ok(k) = inner.parse::<usize>() {
                pos.push(k);
            } else if inner.bytes().all(|c| c == b'_' || c.is_ascii_alphanumeric()) {
                names.push(String::from(inner));
            } else {
                return None;
            }
            i = e + 1;
            continue;
        }
        i += 1;
    }
    pos.sort_unstable();
    pos.dedup();
    names.sort();
    names.dedup();
    if !pos.is_empty() && !names.is_empty() {
        return None;
    }
    Some((pos, names))
}

fn braces(s: &str) -> (usize, usize) {
    (s.matches('{').count(), s.matches('}').count())
}

/// the table: key -> (line, cells after the key)
fn rows(t: &mut T) -> BTreeMap<&'static str, (usize, Vec<&'static str>)> {
    let mut out = BTreeMap::new();
    let mut lines = TABLE.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l));
    let head: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    t.check("messages.tsv: the first row names de and en", head.len() >= 2 && head[0] == "de" && head[1] == "en");
    for (n, line) in lines.enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut cells = line.split('\t');
        let key = cells.next().unwrap_or("");
        let rest: Vec<&str> = cells.collect();
        if out.insert(key, (n + 2, rest)).is_some() {
            t.check(&format!("messages.tsv:{}: the row {:?} comes twice", n + 2, key), false);
        }
    }
    out
}

fn table(t: &mut T) {
    // the modules of lib.rs are the files read here
    let lib = SOURCES[0].1;
    for l in lib.lines() {
        let l = l.trim();
        let m = l.strip_prefix("pub mod ").or_else(|| l.strip_prefix("mod ")).and_then(|m| m.strip_suffix(';'));
        if let Some(m) = m {
            let file = format!("{}.rs", m);
            t.check(&format!("scriptmsgt::SOURCES has script/{}", file), SOURCES.iter().any(|s| s.0 == file));
        }
    }
    let rows = rows(t);
    // the keys of the sources and the macros they are used with
    let mut keys: BTreeMap<String, (&'static str, &'static str, u32)> = BTreeMap::new();
    let mut loose = 0;
    for &(file, src) in SOURCES {
        if file == "msg.rs" {
            continue;
        }
        for l in literals(file, src) {
            match l.mac {
                Some(m) => {
                    let kind = if m == "tf" { "tf" } else { "t" };
                    if let Some(&(k, f, n)) = keys.get(&l.value)
                        && k != kind
                    {
                        t.check(&format!("{}:{} and {}:{}: {:?} is a key of tf! and of t!/de!", f, n, l.file, l.line, l.value), false);
                    }
                    keys.entry(l.value).or_insert((kind, l.file, l.line));
                }
                None if looks_german(&l.value) && !NOT_MESSAGES.contains(&(l.file, l.value.as_str())) => {
                    loose += 1;
                    t.check(&format!("{}:{}: {:?} reads like a message: t!(..), tf!(..) or de!(..) (msg.rs)", l.file, l.line, l.value), false);
                }
                None => {}
            }
        }
    }
    t.eq("messages: German literals outside t!, tf! and de!", loose, 0);
    // every key has a row with English and the same placeholders
    let mut missing = 0;
    for (key, &(kind, file, line)) in &keys {
        t.check(&format!("{}:{}: {:?} reads like German (looks_german)", file, line, key), looks_german(key) || NEUTRAL_KEYS.contains(&key.as_str()));
        t.check(&format!("{}:{}: a key without a line break, tab or # in front", file, line), !key.contains(['\n', '\t', '\r']) && !key.starts_with('#') && !key.is_empty());
        t.check(&format!("{}:{}: {:?} without spaces at the ends", file, line, key), key.trim() == key);
        let Some((n, cells)) = rows.get(key.as_str()) else {
            missing += 1;
            t.check(&format!("{}:{}: no row in messages.tsv for {:?}", file, line, key), false);
            continue;
        };
        let en = cells.first().copied().unwrap_or("");
        t.check(&format!("messages.tsv:{}: {:?} has no English cell", n, key), !en.is_empty());
        t.check(&format!("messages.tsv:{}: the English of {:?} without spaces at the ends", n, key), en.trim() == en);
        for (k, cell) in cells.iter().enumerate().filter(|(_, c)| !c.is_empty()) {
            let same = if kind == "tf" {
                let want = placeholders(key);
                t.check(&format!("{}:{}: {:?}: tf! fills {{}}, {{0}} or {{name}}, not mixed, no {{:spec}}", file, line, key), want.is_some());
                want.is_some() && placeholders(cell) == want
            } else {
                braces(cell) == braces(key)
            };
            t.check(&format!("messages.tsv:{}: column {} of {:?} has other placeholders: {:?}", n, k + 2, key, cell), same);
        }
    }
    t.eq("messages: keys without a row", missing, 0);
    // every row is used
    let mut unused = 0;
    for (key, (n, _)) in &rows {
        if !keys.contains_key(*key) {
            unused += 1;
            t.check(&format!("messages.tsv:{}: {:?} is no key of the sources any more", n, key), false);
        }
    }
    t.eq("messages: rows no source uses", unused, 0);
    t.check("messages: the table has rows", rows.len() > 400);
}

// ---------------------------------------------------------------- English

/// a host without names of its own
struct NoHost;

impl Host for NoHost {
    fn call(&mut self, _: &mut Cx, _: &str, _: &[Value]) -> Result<Value, Fail> {
        Err(Fail::Unknown)
    }
}

/// a host whose modules do not compile (two errors)
struct BrokenModules;

impl Host for BrokenModules {
    fn call(&mut self, _: &mut Cx, _: &str, _: &[Value]) -> Result<Value, Fail> {
        Err(Fail::Unknown)
    }
    fn load(&mut self, _: &mut Cx, _: &str) -> Result<String, String> {
        Ok(String::from("let = 1\nlet = 2"))
    }
}

/// the message a script catches when it imports a module that does not compile
fn broken_import() -> String {
    let Ok(p) = script::compile("try { import(\"m.jfs\") } catch e { print(e) }") else { return String::from("ERR") };
    let mut vm = Vm::new(&p, Limits::default());
    match vm.run(&mut BrokenModules, u64::MAX) {
        Status::Done(_) => String::from(vm.output().trim_end()),
        _ => String::from("?"),
    }
}

/// the messages of the places whose German text is put together from parts: they must stay as
/// they were before the table
fn composed() -> Vec<String> {
    let mut v = syntax("if a & b { }");
    v.extend(syntax("x = $a"));
    v.extend(syntax("let x = foo(1,\n  2\nlet y = 3"));
    v.extend(checked("prnt(1)\nfn box(width) { }\nbox(widht: 2)\nxx = 1"));
    v.push(broken_import());
    let api = Api::new("geo").func("vec(x: float): vec", "").typ(Type::new("vec", "").value().prop("readonly x: float", "").method("length(): float", ""));
    v.extend(ls::check("let v = vec(1)\nv.lenght()\nv.y = 1\nv.x = 2\nv.length = 3\nv.x()\nv.on(\"a\", fn(e) { })", &api).iter().map(|d| d.to_string()));
    let md = api.markdown();
    v.extend(md.lines().filter(|l| l.starts_with('#') || l.ends_with(':')).map(String::from));
    v
}

/// the syntax errors of a script, as Display shows them
fn syntax(src: &str) -> Vec<String> {
    script::parse_partial("t.jfs", src).1.iter().map(|d| d.to_string()).collect()
}

/// the warnings and errors of check()
fn checked(src: &str) -> Vec<String> {
    script::check(src).iter().map(|d| d.to_string()).collect()
}

/// the error of a run as Display shows it (with its trace), or "ok"
fn run_err(src: &str, lim: Limits) -> String {
    let p = match script::compile_named("t.jfs", src) {
        Ok(p) => p,
        Err(d) => return d.iter().map(|x| x.to_string()).collect::<Vec<String>>().join("; "),
    };
    let mut vm = Vm::new(&p, lim);
    match vm.run(&mut NoHost, u64::MAX) {
        Status::Error(e) => e.to_string(),
        _ => String::from("ok"),
    }
}

/// what a script prints (the caught messages)
fn printed(src: &str) -> String {
    let Ok(p) = script::compile(src) else { return String::from("ERR") };
    let mut vm = Vm::new(&p, Limits::default());
    match vm.run(&mut NoHost, u64::MAX) {
        Status::Done(_) => String::from(vm.output().trim_end()),
        Status::Error(e) => e.to_string(),
        _ => String::from("?"),
    }
}

fn eqs(t: &mut T, name: &str, got: String, want: &str) {
    t.eq(name, got.as_str(), want);
}

fn english(t: &mut T) {
    // the codes: a column of the table, else English; "" German
    t.eq("set_language en", script::set_language("en-GB"), "en");
    t.eq("set_language de-AT", script::set_language("de-AT"), "de");
    t.eq("set_language fr", script::set_language("fr"), "en");
    t.eq("language", script::language(), "en");
    t.eq("set_language empty", script::set_language(""), "de");
    t.eq("language default", script::language(), "de");
    t.eq("languages", script::languages(), alloc::vec!["de", "en"]);
    let german = (syntax("let = 3"), checked("prnt(1)"), run_err("let x = 1 / 0", Limits::default()));
    script::set_language("en");

    // syntax errors
    t.eq("en: missing name", syntax("let = 3"), alloc::vec![String::from("1:5: error: Expected a name after let")]);
    t.eq("en: unexpected char", syntax("if a & b { }"), alloc::vec![String::from("1:6: error: Unexpected character & (use && for “and”)")]);
    t.eq("en: missing bracket", syntax("let x = foo(1,\n  2\nlet y = 3"), alloc::vec![String::from("2:4: error: Expected a closing parenthesis ) (opened on line 1)")]);
    t.eq("en: chained comparison", syntax("x = a < b < c"), alloc::vec![String::from("1:11: error: Comparisons cannot be chained – write e.g. a < b && b < c")]);
    t.eq("en: too big", syntax("x = 99999999999999999999"), alloc::vec![String::from("1:5: error: The number 99999999999999999999 is too large (integers at most 9223372036854775807)")]);
    // check: unknown names with a suggestion, declarations, int / int
    t.eq("en: did you mean", checked("prnt(1)"), alloc::vec![String::from("1:1: warning: Unknown name prnt – did you mean print?")]);
    t.eq("en: declared twice", checked("let a = 1\nlet a = 2"), alloc::vec![String::from("2:5: error: a is already declared (line 1)")]);
    t.eq("en: break outside", checked("break"), alloc::vec![String::from("1:1: error: break is outside a loop")]);
    t.eq(
        "en: int division",
        checked("let a = 7\nlet b = a / 2"),
        alloc::vec![String::from("2:9: warning: Integer division: a / 2 truncates (7 / 2 is 3) – float(a) / 2 keeps the fraction, int(a / 2) truncates explicitly")],
    );
    // run-time errors with their trace, fuel, a caught message
    eqs(t, "en: division by 0", run_err("let x = 1 / 0", Limits::default()), "1:11: error: Division by 0\n  in the main program (1:11)");
    eqs(
        t,
        "en: trace",
        run_err("fn half(x) {\n  return x / 0\n}\nhalf(3)", Limits::default()),
        "2:12: error: Division by 0\n  in half (2:12)\n  in the main program (4:1)",
    );
    eqs(
        t,
        "en: fuel",
        run_err("while true { }", Limits { fuel: 1000, ..Limits::default() }),
        "1:7: error: Out of fuel: more than 1000 steps in one call – an endless loop?\n  in the main program (1:7)",
    );
    eqs(t, "en: caught", printed("try { [1][5] } catch e { print(e) }"), "Index 5 is outside the list (length 1)");
    eqs(t, "en: library", printed("try { str.repeat(\"a\", -1) } catch e { print(e) }"), "str.repeat: n must be at least 0, not -1");
    eqs(t, "en: assert_eq", printed("try { assert_eq(1, 2) } catch e { print(e) }"), "assert_eq: expected 2, got 1");
    eqs(t, "en: operator", printed("try { 1 + \"a\" } catch e { print(e) }"), "The operator + does not apply to int and str");
    // (S3) named arguments and patterns
    t.eq("en: named compile", checked("fn box(w, h) { }\nbox(1, d: 2)"), alloc::vec![String::from("2:8: warning: box has no parameter d")]);
    t.eq("en: named suggestion", checked("fn box(width) { }\nbox(widht: 2)"), alloc::vec![String::from("2:5: warning: box has no parameter widht – did you mean width?")]);
    eqs(t, "en: named run", printed("let f = fn(a) { a }\ntry { f(b: 1) } catch e { print(e) }"), "f has no parameter b");
    eqs(t, "en: named anonymous", printed("let fs = [fn(a) { a }]\ntry { fs[0](b: 1) } catch e { print(e) }"), "The function has no parameter b");
    eqs(t, "en: named twice", printed("fn g(a) { a }\nlet h = g\ntry { h(1, a: 2) } catch e { print(e) }"), "g: the parameter a already has an argument");
    eqs(t, "en: list pattern", printed("try { let [a, b] = 5 } catch e { print(e) }"), "The pattern [..] needs a list, not int");
    eqs(t, "en: map pattern", printed("try { let {a} = 5 } catch e { print(e) }"), "The pattern {..} needs a map or a host object, not int");
    t.eq("en: named after", syntax("f(a: 1, 2)"), alloc::vec![String::from("1:9: error: After a named argument, all further ones must be named (name: value)")]);
    // the language services: hover, completion, the library's descriptions, the catalogue
    let src = "/// doubles\nfn twice(x) { return x * 2 }\nlet s = \"a,b\"\nprint(twice(2), s.split(\",\"))";
    let cx = CompleteCx::default();
    let pos = |needle: &str, off: usize| (src.find(needle).unwrap_or(0) + off) as u32;
    t.eq("en: hover fn", ls::hover(src, pos("twice(2", 1), &cx).map(|h| h.text()), Some(String::from("fn twice(x)\nFunction\ndoubles")));
    t.eq("en: hover param", ls::hover(src, pos("x * 2", 0), &cx).map(|h| h.kind), Some(String::from("Parameter of twice")));
    t.eq("en: hover global", ls::hover(src, pos("s.split", 0), &cx).map(|h| h.kind), Some(String::from("Global variable")));
    t.eq(
        "en: hover builtin",
        ls::hover(src, pos("print", 1), &cx).map(|h| h.text()),
        Some(String::from("print(x, ..)\nBuilt-in\nWrites the values, separated by spaces, as one line to the output.")),
    );
    t.eq("en: hover method", ls::hover(src, pos("split", 1), &cx).map(|h| (h.kind, h.doc)), Some((String::from("Standard library"), String::from("Splits the text at sep into a list of at most n parts; without sep at whitespace."))));
    let c = ls::complete("math.", 5, &cx);
    let sqrt = c.items.iter().find(|i| i.label == "sqrt");
    t.eq("en: complete detail", sqrt.map(|i| (i.detail.as_str(), i.doc.as_str())), Some(("math.sqrt(x)", "The square root.")));
    let c = ls::complete("", 0, &cx);
    t.eq("en: complete module", c.items.iter().find(|i| i.label == "json").map(|i| i.doc.as_str()), Some("Standard library module"));
    t.eq("en: library_doc", ls::library_doc("list.push"), Some((Some("x, .."), "Appends the values at the end.")));
    let api = Api::new("geo").typ(Type::new("vec", "A vector.").value().prop("readonly x: float", "").method("length(): float", ""));
    let md = api.markdown();
    t.check("en: catalogue", md.contains("## Types") && md.contains("### vec (value type: == compares the values)") && md.contains("Properties:") && md.contains("Methods:"));
    t.eq("en: catalogue word", EntryKind::Property.word(), "Property");
    let d = ls::check("let v = vec\nv.y = 1", &api);
    t.check("en: ls check", d.iter().all(|x| !x.msg.contains("hat")));
    // a module's warning through take_warnings (compile_api), the heap limit's text of a VM made now
    let api2 = Api::new("t");
    let warned = match script::compile_api("main.jfs", "let a = 7\nprint(a / 2)", &api2) {
        Ok(c) => c.warnings.first().map(|w| w.to_string()),
        Err(_) => None,
    };
    t.check("en: compile_api warning", warned.is_some_and(|w| w.starts_with("2:7: warning: Integer division: a / 2 truncates")));
    if let Ok(p) = script::compile("1") {
        let vm = Vm::new(&p, Limits::default());
        t.eq("en: limit text", vm.str(vm.limit_text()).map(String::from), Some(String::from("Memory limit reached")));
    }

    t.eq("en: composed", composed(), COMPOSED_EN.iter().map(|s| String::from(*s)).collect::<Vec<String>>());
    // (G9.2) a host's descriptions in its own language: map_docs through its catalogue
    let host = Api::new("app").doc("Die App").func("open(path: str)", "Öffnet eine Datei").map_docs(|d| {
        String::from(match d {
            "Die App" => "The app",
            "Öffnet eine Datei" => "Opens a file",
            other => other,
        })
    });
    let h = ls::hover("open(\"a\")", 1, &host);
    t.eq("en: map_docs hover", h.map(|h| h.text()), Some(String::from("open(path: str)\nHost name\nOpens a file")));
    t.check("en: map_docs catalogue", host.markdown().starts_with("# app\n\nThe app\n\n## Global names"));

    // German again: byte for byte as without a language
    script::set_language("de");
    t.eq("de again: syntax", syntax("let = 3"), german.0.clone());
    t.eq("de again: check", checked("prnt(1)"), german.1.clone());
    eqs(t, "de again: run", run_err("let x = 1 / 0", Limits::default()), &german.2);
    t.eq("de: syntax", german.0, alloc::vec![String::from("1:5: Fehler: Nach let fehlt ein Name")]);
    t.eq("de: check", german.1, alloc::vec![String::from("1:1: Warnung: Unbekannter Name prnt – meinst du print?")]);
    eqs(t, "de: run", german.2, "1:11: Fehler: Division durch 0\n  im Hauptprogramm (1:11)");
}
