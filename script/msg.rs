// msg -- (S3, G9 of docs/RICHTUNG.md) the messages and the texts of the language services in the
// host's language
//
//   set_language(code) -> code   the language of every text JFScript makes from now on, for the
//                                whole process (as JFLib's UI language is): "de", "en" or a BCP 47
//                                tag ("en-GB" takes "en"); a code the table has no column for
//                                takes English, "" German.  Gives the code of the column taken
//   language()                   the code of the current column ("de" until set_language)
//   languages()                  the codes of the table's columns ("de", "en")
//   t!("Text")                   the text in the current language (&'static str); the German
//                                text is the key
//   tf!("Text {} {}", a, b)      the text in the current language with its placeholders filled
//                                (String), as format! fills them; also with named arguments:
//                                tf!("{x} / {r}", x = .., r = ..); at least one argument (a
//                                text without one is t!), no implicit captures, no {:spec}
//   de!("Text")                  the German text as it is: a key in a table or a value passed
//                                along (also in hot paths and constants); tr() translates it
//                                where it becomes part of a message
//   tr(text)                     such a key in the current language
//
// The table is locales/messages.tsv, in the form of JFLib's translations (ui::i18n, the apps'
// locales/ui.tsv): tab-separated UTF-8, the first row the language codes ("de\ten"), then one
// row per text with the German text first and the other languages after it; empty lines and
// lines starting with # are skipped, the cells are taken as they are (no escapes).  A cell that is
// missing or empty shows the English text (in a third language), else the German one, as
// JFLib's t() does.  Placeholders: {} in order, {0} {1} by position (for a language that needs
// another order), {name} by name; {{ and }} are braces.  The arguments themselves (names, types,
// values, a host's texts) are never translated.
//
// Without set_language (or with "de" or "") every text is its German key, filled by the same
// rules format! has, so the messages are byte for byte what they were before G9.  Translation
// costs one atomic load where a message is made; the VM's loop makes none.  The table is parsed
// once, at the first translated text: a hash index of the rows in atomics (no lock, no unsafe;
// two threads that build it at once write the same values).
//
// scripttest (group scriptmsg) reads the sources: every key of t!, tf! and de! needs a row with
// an English cell and the same placeholders, every row a key, and a literal that reads like a
// German text outside these macros fails the check.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{Display, Write};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// the table (see the module comment)
pub(crate) const TABLE: &str = include_str!("locales/messages.tsv");

/// the text in the current language: a literal German text is the key
macro_rules! t {
    ($de:literal) => {
        $crate::msg::tr($de)
    };
}

/// format! with the text in the current language: tf!("Unbekannter Name {}", name),
/// tf!("{x} / {r}", x = a, r = b); the arguments are Display and are not translated
macro_rules! tf {
    ($de:literal, $($name:ident = $arg:expr),+ $(,)?) => {{
        if false {
            let _ = ::core::format_args!($de, $($name = $arg),+);
        }
        $crate::msg::fill_named($de, &[$((::core::stringify!($name), &$arg as &dyn ::core::fmt::Display)),+])
    }};
    ($de:literal $(, $arg:expr)+ $(,)?) => {{
        if false {
            let _ = ::core::format_args!($de $(, $arg)+);
        }
        $crate::msg::fill($de, &[$(&$arg as &dyn ::core::fmt::Display),+])
    }};
}

/// a German text as it is, for a table or a value passed along: tr() translates it later
macro_rules! de {
    ($de:literal) => {
        $de
    };
}

// the column of the current language: 0 is German, the keys themselves
static LANG: AtomicUsize = AtomicUsize::new(0);

/// set the language of JFScript's messages and of the texts of its language services for the
/// whole process: "de", "en" or a BCP 47 tag ("en-US" takes "en"); a code the table has no
/// column for takes English, "" German (as without a call).  Gives the code of the column taken.
/// Names, keywords and a host's own texts stay as they are
pub fn set_language(code: &str) -> &'static str {
    let k = column(code);
    LANG.store(k, Ordering::Relaxed);
    languages_iter().nth(k).unwrap_or("de")
}

/// the code of the current language ("de" without set_language)
pub fn language() -> &'static str {
    languages_iter().nth(LANG.load(Ordering::Relaxed)).unwrap_or("de")
}

/// the codes of the languages JFScript has texts in ("de", "en")
pub fn languages() -> Vec<&'static str> {
    languages_iter().collect()
}

fn languages_iter() -> impl Iterator<Item = &'static str> {
    let head = TABLE.split('\n').next().unwrap_or("de");
    head.split('\t').map(|c| c.trim())
}

// the column for a code: the same tag, else the same language, else English; "" German
fn column(code: &str) -> usize {
    fn primary(t: &str) -> &str {
        t.split(['-', '_']).next().unwrap_or(t)
    }
    let code = code.trim();
    if code.is_empty() {
        return 0;
    }
    let norm = |s: &str| String::from(s).replace('_', "-");
    let want = norm(code);
    if let Some(k) = languages_iter().position(|c| norm(c).eq_ignore_ascii_case(&want)) {
        return k;
    }
    if let Some(k) = languages_iter().position(|c| primary(c).eq_ignore_ascii_case(primary(code))) {
        return k;
    }
    english()
}

fn english() -> usize {
    languages_iter().position(|c| c == "en").unwrap_or(0)
}

/// a key (a German text) in the current language
#[inline]
pub(crate) fn tr(de: &'static str) -> &'static str {
    let l = LANG.load(Ordering::Relaxed);
    if l == 0 { de } else { tr_in(de, l) }
}

#[cold]
#[inline(never)]
fn tr_in(de: &'static str, l: usize) -> &'static str {
    let Some(row) = row(de) else { return de };
    cell(row, l).or_else(|| cell(row, english())).unwrap_or(de)
}

// cell k of a row (0 is the key); None when missing or empty
fn cell(row: &'static str, k: usize) -> Option<&'static str> {
    row.split('\t').nth(k).filter(|c| !c.is_empty())
}

/// (tf!) the text in the current language with its {} filled by position
#[inline(never)]
pub(crate) fn fill(de: &'static str, args: &[&dyn Display]) -> String {
    expand(tr(de), args, &[])
}

/// (tf!) the text in the current language with its {name} filled by name
#[inline(never)]
pub(crate) fn fill_named(de: &'static str, args: &[(&str, &dyn Display)]) -> String {
    expand(tr(de), &[], args)
}

// the placeholders of a text filled: {} the next argument, {0} by position, {name} by name, {{
// and }} braces; a placeholder without an argument stays as it is
fn expand(text: &str, args: &[&dyn Display], named: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(text.len() + 16 * (args.len() + named.len()));
    let mut next = 0;
    let mut rest = text;
    while let Some(p) = rest.find(['{', '}']) {
        out.push_str(&rest[..p]);
        let r = &rest[p..];
        if r.starts_with("{{") || r.starts_with("}}") {
            out.push_str(&r[..1]);
            rest = &r[2..];
            continue;
        }
        if let Some(after) = r.strip_prefix('}') {
            out.push('}');
            rest = after;
            continue;
        }
        let Some(e) = r.find('}') else {
            out.push_str(r);
            rest = "";
            break;
        };
        let name = &r[1..e];
        let arg = if name.is_empty() {
            next += 1;
            args.get(next - 1).copied()
        } else if let Ok(k) = name.parse::<usize>() {
            args.get(k).copied()
        } else {
            named.iter().find(|(n, _)| *n == name).map(|(_, a)| *a)
        };
        match arg {
            Some(a) => {
                let _ = write!(out, "{}", a);
            }
            None => out.push_str(&r[..=e]),
        }
        rest = &r[e + 1..];
    }
    out.push_str(rest);
    out
}

// ---- the index: row offsets + 1 in a hash table of atomics (0: empty)

const SLOTS: usize = 4096;
static SLOT: [AtomicU32; SLOTS] = [const { AtomicU32::new(0) }; SLOTS];
static BUILT: AtomicBool = AtomicBool::new(false);

fn hash(s: &str) -> usize {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h as usize & (SLOTS - 1)
}

// the row of a key: its line without the line break
fn row(key: &str) -> Option<&'static str> {
    if !BUILT.load(Ordering::Acquire) {
        build();
    }
    let mut i = hash(key);
    for _ in 0..SLOTS {
        let v = SLOT[i].load(Ordering::Relaxed) as usize;
        if v == 0 {
            return None;
        }
        let line = line_at(v - 1);
        if line.split('\t').next() == Some(key) {
            return Some(line);
        }
        i = (i + 1) & (SLOTS - 1);
    }
    None
}

fn line_at(start: usize) -> &'static str {
    let rest = TABLE.get(start..).unwrap_or("");
    let line = rest.split('\n').next().unwrap_or("");
    line.strip_suffix('\r').unwrap_or(line)
}

#[cold]
fn build() {
    let mut start = 0;
    for (n, raw) in TABLE.split_inclusive('\n').enumerate() {
        let at = start;
        start += raw.len();
        let line = raw.trim_end_matches(['\n', '\r']);
        if n == 0 || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let key = line.split('\t').next().unwrap_or("");
        let v = at as u32 + 1;
        let mut i = hash(key);
        for _ in 0..SLOTS {
            match SLOT[i].compare_exchange(0, v, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(cur) if cur == v => break,
                Err(cur) if line_at(cur as usize - 1).split('\t').next() == Some(key) => break,
                Err(_) => i = (i + 1) & (SLOTS - 1),
            }
        }
    }
    BUILT.store(true, Ordering::Release);
}
