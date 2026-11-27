// script -- the JFNetworker scripting language (.jfs): lexer, parser, syntax tree, editor
// diagnostics, compiler, VM with fuel and a garbage collector
//
//   parse(name, src) -> Result<ast::Program, Vec<Diagnostic>>   the syntax tree, or the syntax
//                                errors (the parser goes on at the next statement after an
//                                error and reports the first error of each line)
//   parse_partial(name, src)     the tree of what parsed plus the errors (editor features)
//   check(src)                   syntax errors plus name resolution against GLOBALS: unknown
//                                names (warnings), duplicate declarations, break / continue
//                                outside a loop, return outside a function (errors), a
//                                division of two certain ints (a warning: it truncates;
//                                intdiv.rs); check_with(src, globals) with other predeclared
//                                names
//   resolve(&program, globals)   the resolution part alone (for the compiler)
//   tokens(src)                  (byte start, length, TokKind) of every token, covering every
//                                byte, for syntax highlighting; never fails
//   outline(src)                 the top-level fns and lets with their lines
//   compile(src) -> Result<Program, Vec<Diagnostic>>   bytecode (code.rs, compile.rs), or the
//                                errors of parse() and check() (warnings do not stop it);
//                                compile_named(name, src) names the file; compile_with(name,
//                                src, globals) for a host with other names (the standalone
//                                runner jfs): unknown names are errors there
//   Vm::new(&program, limits)    a VM (vm.rs): run(host, fuel) -> Done | Yielded | Error,
//                                call(name, args), call_value, global / set_global, pin,
//                                output; Value (16 bytes, heap.rs), Host and Cx for native
//                                functions; num: float arithmetic in software;
//                                cx.position() and cx.warn(text) (the script's call that
//                                reached a native, a warning there), vm.take_warnings()
//   library()                    the standard library every host gets (stdlib.rs: str, list,
//                                map, bytes, math, json, int / float.parse, list.range /
//                                linspace; math.rs: exp, log, pow, sin, cos, tan, asin, atan2,
//                                sin_deg .. in software), found by name before the host
//   ls::index(src) / definition / references / rename / complete / hover / signature /
//   folding / symbols            language services for editors (ls.rs: symbols with their
//                                positions and scopes, also on code that does not parse)
//   fmt::format(src)             the formatter (fmt.rs): Result<String, syntax errors>
//   import(path)                 modules (module.rs): another file's top-level names as a map,
//                                read through Host::load, run once per VM
//   Vm::add_script(name, src)    a further script of a VM, as the <script> elements of a page
//                                are (module.rs): their top-level names are one scope, the errors
//                                name each script's file
//   debugger (debug.rs)          Vm::set_breakpoint, request_pause, step(Step), run() ->
//                                Status::Paused(Pause), frames, locals, upvalues,
//                                frame_globals, eval; Program::line_table / break_lines
//   assert(c, msg), assert_eq(a, b, msg)   tests (stdlib.rs; the runner's jfs --test)
//   (S2) the host interface in its second version (docs/HOST.md):
//   host objects                 Value::Host: Cx::new_object(ty, id) (identity, Host::drop when
//                                the collector frees it), Cx::new_value(ty, &data) (a value
//                                type: == by data); obj.name, obj.name = v, obj.m(..), a + b,
//                                -a, type(x), print go to Host::get / set / method / op /
//                                to_str (vm.rs)
//   events                       obj.on("event", f) / obj.off(..) after Host::listen; the VM
//                                keeps the functions: Vm::listeners, Vm::emit (each listener an
//                                invocation of its own), Cx::emit, remove_listeners
//   api::Api                     a host's object model as data (api.rs): global names, types
//                                with properties, methods, events, operators; the language
//                                services use it (ls::Context, ls::check; lsapi.rs infers the
//                                types), Api::markdown / catalogue make the object catalogue
//   (S3) await (task.rs, docs/HOST.md "Long operations"):
//   promises                     Cx::promise() -> Promise { id, value } for a long operation of
//                                the host (a dialog, a download); Vm::resolve / reject /
//                                reject_msg(id, ..) settle it, Vm::promise_state tells; type(p) is
//                                "promise"; Api types "promise<T>"
//   waiting invocations          `await p` of a promise that is not settled: the invocation waits
//                                with its stack, run() gives Status::Done(its own promise) and the
//                                VM is idle (call, emit, add_script go on); Vm::wake() continues
//                                the ones whose promise settled, in that order; is_waiting(p),
//                                waiting(), ready(), cancel_waiting(p); check warns about await
//                                in a callback of list.map .. (awaitchk.rs): it cannot wait there
//   (S3) the gaps the first S2 host (JFLib's modeltool) found:
//   compile_api(name, src, &api) -> Result<Compiled, errors>   compile_with(api.names()) and the
//                                warnings of ls::check(src, &api) from the same parse
//                                (Compiled { program, warnings }); the modules it imports and
//                                the scripts of Vm::add_script get the same checks, their
//                                warnings come through Vm::take_warnings with their files
//   signatures                   optional parameters (`name?: T`), forms (overloads: one name
//                                declared several times), unions (`number | vec`), the type
//                                number; api::bind(&forms, &args) -> Binding: which form fits a
//                                call and which parameter each argument fills (also by name,
//                                for the named arguments of S3); signature help shows the forms,
//                                Api::check_calls lets ls::check count arguments against them
//   messages                     a host object's type by the host's name ("str.split: sep muss
//                                str sein, nicht mesh"), as type(x) gives it
//   suggestions                  "meinst du x?" only for really close names (check::suggest);
//                                Api / Type::removed("geom.move", "m.translate") names what to
//                                take instead of a removed name
//   (S3) the syntax pass (docs/LANGUAGE.md "The syntax of S3"):
//   c ? a : b                    loosest of all operators, right-associative: a ?? b ? c : d is
//                                (a ?? b) ? c : d; a line that starts with ? or : continues
//   fn f(a, b = 2)               a default for a missing argument and for nil, evaluated at each
//                                call in the function's scope (the parameters before it)
//   f(x, width: 2)               named arguments after the positional ones: a script function's
//                                by its parameters' names, a host's function or method by its
//                                description (api::bind_lenient; Host::call gets the values by
//                                position, nil in the gaps), the library's by its parameters
//                                (lsdoc.rs); a name that fits no parameter throws (or is an
//                                error at compile time where the callee is known)
//   let [x, y, ...r] = l         destructuring in let, parameters, for and the for clauses of
//   let {a, b: c, d = 1} = m     comprehensions, nested, with defaults; a missing item or key is
//   [a, b] = [b, a]              nil; a value of the wrong type throws; {..} reads a host
//                                object's properties; an assignment through a list pattern
//   {a, b}                       a map literal's shorthand for {a: a, b: b}
//   [e for x in xs if c]         a list from a loop (several for and if clauses), its variables
//                                local to it
//   fuel per stretch             (S3) Limits::fuel holds for each stretch of an invocation
//                                between two waits in await; call_fuel() counts on over them
//
// Files: Diagnostic::file, Error::file and TraceLine::file name the file of a position (the
// name given to parse / compile_named / compile_with, or a module's path).
// The language: small, dynamically typed, JS/Lua-like.  Values: nil, bool, int (i64), float
// (f64), str (immutable UTF-8), bytes, list, map (insertion ordered; keys str, int, bool),
// fn (closures).  Statements: let x = e, fn name(a, b) { }, if c { } else if c { } else { },
// while c { }, for x in list { }, for k, v in map { }, for i in 0..n { }, break, continue,
// return [e], try { } catch e { }, throw e, assignments = += -= *= /= %= to names, fields
// (m.k) and indexes (a[i]), expression statements; (S3) patterns after let, in parameters
// (with defaults) and after for, [a, b] = v.  Expressions: literals (1_000, 0xFF,
// 0b101, 1.5e3, "text" with \n \t \r \0 \" \\ \$ \u{..} and ${expr}, true, false, nil),
// [a, b], {k: v, "s": v, 1: v, [e]: v, (S3) a}, fn(a) { }, calls (S3 with named arguments),
// .field, [index], methods s.split(","), unary - ! and (S3) await, (S3) [e for x in xs if c],
// operators from loose to tight: (S3) c ? a : b, ?? || && (== !=) (< <= > >=) (+ -) (* / %).
// `await e` gives the value of a promise once it is there (any other value as it is), in any
// function and at the top level.  Comments // and /* */.  A line break ends a statement once
// its expression is complete (parse.rs has the details); a string stays on one line.
// Positions: lines and columns from 1, columns count characters, end_col is exclusive.
// The messages are German.  The scope rules are in ast.rs.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod ast;
pub mod check;
mod intdiv;
pub mod lex;
mod parse;
mod code;
mod compile;
mod heap;
pub mod num;
pub mod math;
pub mod vm;
mod stdlib;
mod strlib;
mod listlib;
mod jsonlib;
// (SK6) language services for editors (ls.rs, the library's descriptions in lsdoc.rs) and the
// formatter (fmt.rs); (S2) a host's object model as data (api.rs) and its types in the
// services (lsapi.rs)
pub mod ls;
mod lsdoc;
pub mod api;
mod lsapi;
pub mod fmt;
mod module;
pub mod debug;
// (S3) await: promises and the invocations that wait for them (task.rs), the warning of check
// for an await that cannot wait (awaitchk.rs)
pub mod task;
mod awaitchk;

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

pub use ast::Span;
pub use check::resolve;
pub use lex::{TokClass, TokKind};
pub use parse::MAX_DEPTH;
pub use code::Program;
pub use heap::{Ref, Value};
pub use vm::{Cx, Error, ErrorKind, Fail, Handled, Host, Limits, Object, Operator, Pin, Position, Status, TraceLine, Vm, Warning, MAX_WARNINGS};
pub use code::LineEntry;
pub use api::Api;
pub use debug::{Breakpoint, FrameInfo, Pause, PauseReason, Step};
pub use task::{Promise, PromiseState};

/// a message for the editor: position (1-based; columns count characters, end_col is
/// exclusive and on the same line), error or warning, German text; file: the name given to
/// parse / compile_named / compile_with ("" from check)
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub error: bool,
    pub msg: String,
    pub file: String,
}

impl core::fmt::Display for Diagnostic {
    /// "3:5: Fehler: Hier fehlt eine schließende Klammer )"
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}:{}: {}: {}", self.line, self.col, if self.error { "Fehler" } else { "Warnung" }, self.msg)
    }
}

/// the names a script may use without declaring them: the host API of the simulator and the
/// standard library (check warns about other unknown names)
pub const GLOBALS: &[&str] = &[
    "print", "log", "len", "str", "int", "float", "type", "math", "json", "bytes", "now", "rand", "config", "timer", "net",
    "dns", "http", "db", "disk", "metrics", "list", "map", "import", "assert", "assert_eq",
];

/// the functions and constants of the standard library ("str.split", "math.pi", "bytes"),
/// for completion in an editor; methods are listed under their type (s.split(",") is
/// str.split(s, ","))
pub fn library() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::with_capacity(stdlib::NAMES.len() + stdlib::CONSTANTS.len() + 10);
    v.extend_from_slice(&["print", "len", "type", "str", "int", "float", "bytes"]);
    v.extend_from_slice(stdlib::NAMES);
    v.extend_from_slice(stdlib::CONSTANTS);
    v.extend_from_slice(&["import", "assert", "assert_eq"]);
    v
}

/// a diagnostic for the first line of a span
pub(crate) fn diag(src: &str, sp: Span, error: bool, msg: String) -> Diagnostic {
    let s = src.get(sp.start as usize..sp.end as usize).unwrap_or("");
    let w = s.split('\n').next().unwrap_or("").chars().count() as u32;
    Diagnostic { line: sp.line, col: sp.col, end_col: sp.col + w.max(1), error, msg, file: String::new() }
}

/// the diagnostics with the file they belong to
fn named(name: &str, mut d: Vec<Diagnostic>) -> Vec<Diagnostic> {
    for x in d.iter_mut() {
        x.file = String::from(name);
    }
    d
}

/// the syntax tree of a script, or its syntax errors; name: a file name for later messages
pub fn parse(name: &str, src: &str) -> Result<ast::Program, Vec<Diagnostic>> {
    let (prog, diags) = parse::parse_program(name, src);
    if diags.is_empty() { Ok(prog) } else { Err(named(name, diags)) }
}

/// the tree of everything that parsed and the syntax errors (empty when parse() succeeds)
pub fn parse_partial(name: &str, src: &str) -> (ast::Program, Vec<Diagnostic>) {
    let (prog, diags) = parse::parse_program(name, src);
    (prog, named(name, diags))
}

/// syntax errors and the findings of resolve() with GLOBALS, sorted by position
pub fn check(src: &str) -> Vec<Diagnostic> {
    check_with(src, GLOBALS)
}

/// check() with other predeclared names
pub fn check_with(src: &str, globals: &[&str]) -> Vec<Diagnostic> {
    let (prog, mut d) = parse::parse_program("", src);
    d.extend(check::resolve(&prog, globals));
    d.sort_by_key(|x| (x.line, x.col));
    d
}

/// (byte start, length, kind) of every token in source order, covering every byte; the
/// `${` and `}` of interpolations are TokKind::Interp, the text around them Str
pub fn tokens(src: &str) -> Vec<(u32, u32, TokKind)> {
    let (toks, _) = lex::lex(src);
    let mut out = Vec::with_capacity(toks.len() + 8);
    for t in &toks {
        let (s, e) = (t.start, t.end);
        match t.kind {
            TokKind::Eof => {}
            TokKind::StrHead => {
                out.push((s, e - 2 - s, TokKind::Str));
                out.push((e - 2, 2, TokKind::Interp));
            }
            TokKind::StrMid => {
                out.push((s, 1, TokKind::Interp));
                if e - 2 > s + 1 {
                    out.push((s + 1, e - 3 - s, TokKind::Str));
                }
                out.push((e - 2, 2, TokKind::Interp));
            }
            TokKind::StrTail => {
                out.push((s, 1, TokKind::Interp));
                if e > s + 1 {
                    out.push((s + 1, e - s - 1, TokKind::Str));
                }
            }
            k => out.push((s, e - s, k)),
        }
    }
    out
}

/// the top-level declarations with their lines: "fn key(id)", "let hits" (also while the
/// script has syntax errors)
pub fn outline(src: &str) -> Vec<(String, u32)> {
    let (prog, _) = parse::parse_program("", src);
    let mut out = Vec::new();
    for s in &prog.body {
        match &s.kind {
            ast::StmtKind::Let { pat, .. } => out.push((format!("let {}", pat.label()), s.span.line)),
            ast::StmtKind::Fn(f) => {
                let params: Vec<String> = f.params.iter().map(|p| p.label()).collect();
                let name = f.name.as_ref().map_or("", |n| n.text.as_str());
                out.push((format!("fn {}({})", name, params.join(", ")), s.span.line));
            }
            _ => {}
        }
    }
    out
}

/// compile a script for the VM; the errors of parse() and check() (not the warnings) when it
/// has some
pub fn compile(src: &str) -> Result<Program, Vec<Diagnostic>> {
    compile_named("", src)
}

/// compile() with a file name (Program::name)
pub fn compile_named(name: &str, src: &str) -> Result<Program, Vec<Diagnostic>> {
    let (prog, mut d) = parse::parse_program(name, src);
    d.extend(check::resolve(&prog, GLOBALS));
    finish(&prog, src, d, None, &[])
}

/// compile_named() for a host with names of its own instead of GLOBALS: `globals` are the
/// names it answers (Host::call, set_global; "log", "timer", "config"), the built-ins and the
/// standard library are known without them.  Unlike compile(), a name that is neither
/// declared nor known is an error ("Unbekannter Name prnt – meinst du print?"), so a typing
/// error stops the script before it runs; the other warnings still do not
pub fn compile_with(name: &str, src: &str, globals: &[&str]) -> Result<Program, Vec<Diagnostic>> {
    let known = known_names(globals);
    let (prog, mut d) = parse::parse_program(name, src);
    d.extend(check::resolve_strict(&prog, &known, None));
    let strict = code::Strict { names: globals.iter().map(|g| String::from(*g)).collect(), api: None };
    finish(&prog, src, d, Some(strict), &[])
}

/// (S3) what compile_api gives: the program and the warnings of the same parse
#[derive(Clone, Debug)]
pub struct Compiled {
    pub program: Program,
    /// the warnings ls::check(src, &api) gives (the file named), by position
    pub warnings: Vec<Diagnostic>,
}

/// (S3) compile_with(name, src, &api.names()) and the warnings of the language services in one
/// parse: Ok with the program and the warnings ls::check(src, &api) gives -- members a known
/// host type does not have, removed names, a division of two ints, a top-level let read before
/// it ran, with Api::check_calls calls whose arguments fit no form --, each with the file;
/// Err: the errors, as compile_with gives them (unknown names are errors; removed ones say what
/// to take instead).  The modules the program imports (Host::load) and the scripts of
/// Vm::add_script are compiled the same way, and their warnings, named by their files, come
/// with the VM's own through Vm::take_warnings
pub fn compile_api(name: &str, src: &str, api: &Api) -> Result<Compiled, Vec<Diagnostic>> {
    compile_api_in(name, src, api, None)
}

/// compile_api; shared: the description as the program has it (a module's: no copy)
fn compile_api_in(name: &str, src: &str, api: &Api, shared: Option<&Arc<Api>>) -> Result<Compiled, Vec<Diagnostic>> {
    let (prog, mut d) = parse::parse_program(name, src);
    d.extend(lsapi::check_names(&prog, api, &[]));
    if !d.iter().any(|x| x.error) {
        d.extend(lsapi::check_members(&prog, src, api));
    }
    d.sort_by_key(|x| (x.line, x.col));
    let warnings = named(name, d.iter().filter(|x| !x.error).cloned().collect());
    let api = shared.cloned().unwrap_or_else(|| Arc::new(api.clone()));
    let strict = code::Strict { names: api.names().into_iter().map(String::from).collect(), api: Some(api) };
    let program = finish(&prog, src, d, Some(strict), &[])?;
    Ok(Compiled { program, warnings })
}

/// the names compile_with knows: the built-ins, the standard library, the host's
pub(crate) fn known_names<'a>(globals: &[&'a str]) -> Vec<&'a str> {
    let mut known: Vec<&'a str> = Vec::with_capacity(globals.len() + 16);
    for n in library() {
        let root = n.split('.').next().unwrap_or(n);
        if !known.contains(&root) {
            known.push(root);
        }
    }
    known.extend_from_slice(globals);
    known
}

/// a module the VM imports, compiled the way its program was (compile_named, compile_with with
/// the same names, compile_api with the same description) -> the program and (compile_api) its
/// warnings
pub(crate) fn compile_module(path: &str, src: &str, strict: Option<&code::Strict>) -> Result<(Program, Vec<Diagnostic>), Vec<Diagnostic>> {
    match strict {
        Some(code::Strict { api: Some(api), .. }) => compile_api_in(path, src, api, Some(api)).map(|c| (c.program, c.warnings)),
        Some(s) => {
            let g: Vec<&str> = s.names.iter().map(|s| s.as_str()).collect();
            compile_with(path, src, &g).map(|p| (p, Vec::new()))
        }
        None => compile_named(path, src).map(|p| (p, Vec::new())),
    }
}

/// a further script of a VM (Vm::add_script), compiled the way its program was (strict: the host's
/// names of compile_with, the description of compile_api); `shared` are the top-level names of
/// the scripts before it -> the program and (compile_api) its warnings
pub(crate) fn compile_shared(name: &str, src: &str, strict: Option<&code::Strict>, shared: &[String]) -> Result<(Program, Vec<Diagnostic>), Vec<Diagnostic>> {
    let (prog, mut d) = parse::parse_program(name, src);
    let names: Vec<&str> = shared.iter().map(|s| s.as_str()).collect();
    match strict {
        Some(code::Strict { api: Some(api), .. }) => {
            d.extend(lsapi::check_names(&prog, api, &names));
            if !d.iter().any(|x| x.error) {
                d.extend(lsapi::check_members(&prog, src, api));
            }
        }
        Some(s) => {
            let g: Vec<&str> = s.names.iter().map(|s| s.as_str()).collect();
            let mut known = known_names(&g);
            known.extend_from_slice(&names);
            d.extend(check::resolve_strict(&prog, &known, None));
        }
        None => {
            let mut known = names;
            known.extend_from_slice(GLOBALS);
            d.extend(check::resolve(&prog, &known));
        }
    }
    let warnings = match strict {
        Some(code::Strict { api: Some(_), .. }) => {
            d.sort_by_key(|x| (x.line, x.col));
            named(name, d.iter().filter(|x| !x.error).cloned().collect())
        }
        _ => Vec::new(),
    };
    finish(&prog, src, d, strict.cloned(), shared).map(|p| (p, warnings))
}

/// the program, or the errors among the diagnostics of parsing and resolving (with the file);
/// strict: the host's names of compile_with (and the description of compile_api), for the
/// modules the program imports
fn finish(prog: &ast::Program, src: &str, mut d: Vec<Diagnostic>, strict: Option<code::Strict>, shared: &[String]) -> Result<Program, Vec<Diagnostic>> {
    d.retain(|x| x.error);
    if !d.is_empty() {
        d.sort_by_key(|x| (x.line, x.col));
        return Err(named(&prog.name, d));
    }
    let mut code = compile::generate(prog, src, shared);
    code.strict = strict;
    Ok(Program { code: Arc::new(code) })
}
