// scriptsynt -- (S3) checks of the syntax pass: the conditional c ? a : b, default parameter
// values, named arguments (to script functions, to the host's functions and methods through its
// description, to the library), destructuring (let, parameters, for, assignment; rest, defaults,
// renaming, nesting), the shorthand {a, b} and lists from a loop.  Parsing with its errors and
// recovery, tokens, the scopes of check, the VM's semantics and its errors with positions, slices
// of size 1 against an uninterrupted run, the debugger's locals, the language services, the
// formatter, the int-division warning and the await warning

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use script::api::Type;
use script::ls::{self, Context, ItemKind};
use script::{Api, Cx, Fail, Host, Limits, Object, Status, TokClass, TokKind, Value, Vm};

use crate::T;

pub fn run(t: &mut T) {
    parsing(t);
    recovery(t);
    tokens(t);
    scopes(t);
    semantics(t);
    errors(t);
    named_script(t);
    named_host(t);
    named_library(t);
    slices(t);
    debugger(t);
    services(t);
    formatter(t);
    intdiv(t);
    awaits(t);
}

// ---------------------------------------------------------------- helpers

/// the S-expression of a script, or "ERR" and its errors
fn sx(src: &str) -> String {
    match script::parse("t.jfs", src) {
        Ok(p) => p.sexpr(),
        Err(d) => {
            let v: Vec<String> = d.iter().map(|x| x.to_string()).collect();
            format!("ERR {}", v.join("; "))
        }
    }
}

/// (line, message) of the syntax errors
fn errs(src: &str) -> Vec<(u32, String)> {
    script::parse_partial("t.jfs", src).1.into_iter().map(|d| (d.line, d.msg)).collect()
}

/// the diagnostics of check() as text ("3:5: Warnung: ..")
fn diags(src: &str) -> Vec<String> {
    script::check(src).iter().map(|d| d.to_string()).collect()
}

/// run a script (compile_named) to its end: what it printed, then the error if there is one
fn out(src: &str) -> String {
    let p = match script::compile_named("t.jfs", src) {
        Ok(p) => p,
        Err(d) => return format!("ERR {}", d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("; ")),
    };
    let mut vm = Vm::new(&p, Limits::default());
    loop {
        match vm.run(&mut (), u64::MAX) {
            Status::Yielded | Status::Paused(_) => {}
            Status::Done(_) => return vm.take_output(),
            Status::Error(e) => return format!("{}{}:{}: {}", vm.take_output(), e.line, e.col, e.msg),
        }
    }
}

/// t.eq of a text made and a text expected
fn eqs(t: &mut T, name: &str, got: String, want: &str) {
    t.eq(name, got.as_str(), want);
}

/// the byte offset of the k-th (from 0) occurrence of needle, plus `off`
fn at(src: &str, needle: &str, k: usize, off: usize) -> u32 {
    let mut from = 0;
    for _ in 0..k {
        from = src[from..].find(needle).map_or(src.len(), |i| from + i + 1);
    }
    (src[from..].find(needle).map_or(src.len(), |i| from + i) + off) as u32
}

// ---------------------------------------------------------------- parsing

fn parsing(t: &mut T) {
    let cases: &[(&str, &str)] = &[
        // the conditional: loosest, right-associative, after ??
        ("x = c ? a : b", "(= x (? c a b))"),
        ("x = a ?? b ? c : d", "(= x (? (?? a b) c d))"),
        ("x = a ? b : c ? d : e", "(= x (? a b (? c d e)))"),
        ("x = a ? b ? c : d : e", "(= x (? a (? b c d) e))"),
        ("x = a || b ? 1 : 2 + 3", "(= x (? (|| a b) 1 (+ 2 3)))"),
        ("x = -a ? await b : !c", "(= x (? (- a) (await b) (! c)))"),
        ("let x = c\n  ? a\n  : b", "(let x (? c a b))"),
        ("return1 = f(c ? 1 : 2, w: d ? 3 : 4)", "(= return1 (call f (? c 1 2) (: w (? d 3 4))))"),
        ("x = {k: c ? 1 : 2}", "(= x (map (\"k\" (? c 1 2))))"),
        // defaults
        ("fn f(a, b = 2) {}", "(fn f (a (= b 2)) (block))"),
        ("let g = fn(x = [], {w, h = 1}) {}", "(let g (fn ((= x (list)) {w h:(= h 1)}) (block)))"),
        // named arguments
        ("f(x, width: 2)", "(call f x (: width 2))"),
        ("obj.m(1, b: 2, c: 3)", "(method obj m 1 (: b 2) (: c 3))"),
        ("f(\n  w: 1,\n  h: 2,\n)", "(call f (: w 1) (: h 2))"),
        // destructuring
        ("let [x, y] = p", "(let [x y] p)"),
        ("let {a, b: c, \"d e\": f} = m", "(let {a b:c \"d e\":f} m)"),
        ("let [first, ...rest] = l", "(let [first ...rest] l)"),
        ("let [[a, b], {c: [d]}] = v", "(let [[a b] {c:[d]}] v)"),
        ("let [a = 1, [b] = []] = v", "(let [(= a 1) (= [b] (list))] v)"),
        ("for [i, p] in pairs {}", "(for [i p] pairs (block))"),
        ("for k, {x} in m {}", "(for (k {x}) m (block))"),
        ("fn f([x, y], {w, h}) {}", "(fn f ([x y] {w h}) (block))"),
        ("[a, b] = [b, a]", "(= [a b] (list b a))"),
        ("[m.x, l[0], [p]] = v", "(= [(. m x) (index l 0) [p]] v)"),
        // the shorthand: the same tree as {a: a}
        ("m = {a, b: 1, c}", "(= m (map (\"a\" a) (\"b\" 1) (\"c\" c)))"),
        ("m = {a: a, b: 1, c: c}", "(= m (map (\"a\" a) (\"b\" 1) (\"c\" c)))"),
        // lists from a loop
        ("l = [x * 2 for x in xs if x > 0]", "(= l (comp (* x 2) (for x xs) (if (> x 0))))"),
        ("l = [a + b for a in xs for b in 0..3]", "(= l (comp (+ a b) (for a xs) (for b (.. 0 3))))"),
        ("l = [k for k, v in m if v if k != \"x\"]", "(= l (comp k (for (k v) m) (if v) (if (!= k \"x\"))))"),
        ("l = [p for [a, b] in l]", "(= l (comp p (for [a b] l)))"),
        ("print([\n  x\n  for x in l\n])", "(call print (comp x (for x l)))"),
        ("l = [[j for j in 0..i] for i in 0..3]", "(= l (comp (comp j (for j (.. 0 i))) (for i (.. 0 3))))"),
        // the forms before S3 mean what they meant
        ("x = a ?? b", "(= x (?? a b))"),
        ("x = {a: 1}", "(= x (map (\"a\" 1)))"),
        ("x = [1, 2,]", "(= x (list 1 2))"),
        ("x = f(a, b)", "(= x (call f a b))"),
        ("let x = 1", "(let x 1)"),
        ("for k, v in m {}", "(for (k v) m (block))"),
        ("fn f(a, b) {}", "(fn f (a b) (block))"),
    ];
    for (src, want) in cases {
        t.eq(&format!("parse {}", src), sx(src).as_str(), *want);
    }
    // spans: the conditional from its condition to its end, a named argument from its name
    if let Ok(p) = script::parse("", "x = f(1, width: 22)") {
        let s = &p.body[0];
        if let script::ast::StmtKind::Assign { value, .. } = &s.kind
            && let script::ast::ExprKind::Call(_, args) = &value.kind
        {
            t.eq("span named", (args[1].span.col, args[1].span.end - args[1].span.start), (10, 9));
        } else {
            t.check("named argument in the tree", false);
        }
    }
    // the outline of patterns and defaults
    t.eq("outline", script::outline("let [a, b] = p\nfn f(x, y = 2, [p, q]) {}"), vec![(String::from("let [a, b]"), 1), (String::from("fn f(x, y = …, [p, q])"), 2)]);
}

fn recovery(t: &mut T) {
    let one = |src: &str| errs(src).into_iter().map(|e| e.1).collect::<Vec<_>>();
    t.eq("err cond", one("let x = c ? 1"), vec![String::from("Hier fehlt : mit dem Wert für den anderen Fall (c ? a : b)")]);
    t.eq("err positional after named", one("f(a: 1, 2)"), vec![String::from("Nach einem benannten Argument sind alle weiteren benannt (name: wert)")]);
    t.eq("err pattern without value", one("let [a, b]"), vec![String::from("Hier fehlt = mit dem Wert, den das Muster zerlegt")]);
    t.eq("err empty list pattern", one("let [] = x"), vec![String::from("Ein leeres Muster [] bindet nichts")]);
    t.eq("err empty map pattern", one("let {} = x"), vec![String::from("Ein leeres Muster {} bindet nichts")]);
    t.eq("err rest not last", one("let [...r, a] = x"), vec![String::from("Der Rest ...name steht am Ende des Musters")]);
    t.eq("err spread", one("let w = ...a"), vec![String::from("... gibt es nur vor dem Rest eines Musters: let [a, ...rest] = liste")]);
    t.eq("err target", one("[a, f()] = x"), vec![String::from("Im Muster vor = muss eine Variable, ein Feld, ein Index oder eine Liste davon stehen")]);
    t.eq("err pattern +=", one("[a, b] += x"), vec![String::from("Vor += muss eine Variable, ein Feld oder ein Index stehen")]);
    t.eq("err two before for", one("l = [x, y for x in l]"), vec![String::from("Vor for steht in einer Liste aus einer Schleife genau ein Ausdruck: [x * 2 for x in xs]")]);
    t.eq("err if without cond", one("l = [x for x in l if]"), vec![String::from("Hier fehlt ein Ausdruck")]);
    t.eq("err keyword in pattern", one("let {await} = m"), vec![String::from("await ist ein Schlüsselwort und kann kein Name sein – schreibe await: name")]);
    t.eq("err keyword key renamed", one("let {await: w} = m"), Vec::<String>::new());
    t.eq("err ? alone", one("let x = a ?"), vec![String::from("Hier fehlt ein Ausdruck")]);
    t.eq("err in clause", one("l = [x for x l]"), vec![String::from("Hier fehlt in, z. B. for x in liste { … }")]);
    t.eq("err after clauses", one("l = [x for x in l y]"), vec![String::from("Hier fehlt for, if oder eine schließende eckige Klammer ]")]);
    // what follows a broken statement still parses (the editor's tree)
    let (p, d) = script::parse_partial("", "let [a, b] =\nlet y = c ? 1\nlet z = [q for q in l]\nf(a: 1, 2)\nlet w = 3");
    t.eq("recovery errors", d.len(), 3);
    let names: Vec<String> = p.body.iter().map(|s| s.sexpr()).collect();
    t.check("recovery keeps the rest", names.iter().any(|s| s == "(let z (comp q (for q l)))") && names.iter().any(|s| s == "(let w 3)"));
    // a fn f( { with its ) missing: the body follows (a map pattern needs a key and , : = or })
    t.eq("err fn( {", one("fn f( {\n  print(1)\n}"), vec![String::from("Hier fehlt eine schließende Klammer )")]);
    eqs(t, "map pattern param", sx("fn f({w}) { return w }"), "(fn f ({w}) (block (return w)))");
}

fn tokens(t: &mut T) {
    let src = "x = c ? [a, ...r] : b";
    let toks = script::tokens(src);
    let q = toks.iter().find(|x| x.2 == TokKind::Question).map(|x| (x.0, x.1, x.2.class()));
    t.eq("token ?", q, Some((6, 1, TokClass::Op)));
    let d = toks.iter().find(|x| x.2 == TokKind::DotDotDot).map(|x| (x.0, x.1));
    t.eq("token ...", d, Some((12, 3)));
    let mut pos = 0;
    let mut ok = true;
    for (s, n, _) in &toks {
        ok &= *s == pos && *n > 0;
        pos = s + n;
    }
    t.check("tokens cover", ok && pos as usize == src.len());
    t.eq("?? stays", script::tokens("a ?? b").iter().filter(|x| x.2 == TokKind::Coalesce).count(), 1);
    t.eq(".. stays", script::tokens("0..n").iter().filter(|x| x.2 == TokKind::DotDot).count(), 1);
}

// ---------------------------------------------------------------- scopes

fn scopes(t: &mut T) {
    let none: Vec<String> = Vec::new();
    t.eq("scope let pattern", diags("let [a, b] = [1, 2]\nlet {c, d: e} = {}\nprint(a + b, c, e)\nfn f() { return a }"), none.clone());
    t.eq("scope pattern twice top", diags("let l = []\nlet [a, a] = l"), vec![String::from("2:9: Fehler: a ist schon deklariert (Zeile 2)")]);
    t.eq("scope pattern twice block", diags("fn g(l) {\n  let [a, {b: a}] = l\n}"), vec![String::from("2:15: Fehler: a ist in diesem Block schon deklariert (Zeile 2)")]);
    t.eq("scope params twice", diags("fn f([x, y], {x}) {}"), vec![String::from("1:15: Fehler: Der Parameter x kommt zweimal vor")]);
    t.eq("scope comprehension local", diags("let l = []\nprint([x for x in l])\nprint(x)"), vec![String::from("3:7: Warnung: Unbekannter Name x")]);
    t.eq("scope comprehension src outside", diags("let x = [1]\nprint([x for x in x])"), none.clone());
    t.eq("scope comprehension clauses", diags("let l = []\nprint([b for a in l for b in a if b > a])"), none.clone());
    t.eq("scope default sees earlier", diags("fn f(a, b = a, c = a + b) { return c }"), none.clone());
    t.eq("scope default not later", diags("fn f(a = b, b = 1) { return a }"), vec![String::from("1:10: Warnung: Unbekannter Name b")]);
    t.eq("scope pattern default sees earlier", diags("let {w, h = w} = {}\nprint(h)"), none.clone());
    t.eq("scope range pattern", diags("for [a, b] in 0..3 {}"), vec![String::from("1:5: Fehler: Ein Bereich a..b gibt ganze Zahlen: die lassen sich nicht zerlegen")]);
    t.eq("scope for pattern", diags("for k, [a, b] in {} { print(k, a, b) }"), none.clone());
    t.eq("scope assign pattern", diags("[q, r] = [1, 2]"), vec![String::from("1:2: Warnung: Unbekannter Name q – eine neue Variable wird mit let q = … angelegt"), String::from("1:5: Warnung: Unbekannter Name r – eine neue Variable wird mit let r = … angelegt")]);
    t.eq("scope shorthand", diags("let a = 1\nprint({a, bb})"), vec![String::from("2:11: Warnung: Unbekannter Name bb")]);
    t.eq("scope named label is no use", diags("fn f(width) {}\nf(width: 1)"), none.clone());
    // named arguments: twice, to the host without a description, to the library, to a fn
    t.eq("named twice", diags("print(x: 1, x: 2)"), vec![String::from("1:13: Fehler: Das benannte Argument x kommt zweimal vor")]);
    t.eq("named host", diags("timer.after(10, f: 1)"), vec![String::from("1:7: Fehler: timer.after: benannte Argumente gehen an Funktionen des Hosts nur mit seiner Beschreibung (script::Api, compile_api)")]);
    t.eq("named library", diags("math.clamp(1, low: 0)"), vec![String::from("1:15: Fehler: math.clamp hat keinen Parameter low – meinst du lo?")]);
    t.eq("named library ok", diags("print(math.clamp(5, lo: 0, hi: 3), str.split(\"a b\", sep: \" \"))"), none.clone());
    t.eq("named fn", diags("fn f(a, b) {}\nf(1, c: 2)\nf(1, a: 2)\nf(1, b: 2)"), vec![String::from("2:6: Warnung: f hat keinen Parameter c"), String::from("3:6: Warnung: f: der Parameter a hat schon ein Argument")]);
    t.eq("named fn assigned", diags("fn f(a) {}\nf = fn(c) {}\nf(c: 2)"), none.clone());
    t.eq("named fn local", diags("fn g() {\n  fn h(width) {}\n  h(widht: 1)\n}"), vec![String::from("3:5: Warnung: h hat keinen Parameter widht – meinst du width?")]);
    // strict: comprehension and pattern names are declared
    t.check("strict comprehension", script::compile_with("", "let l = [1]\nlet [a] = [x * 2 for x in l]\nprint(a)", &[]).is_ok());
}

// ---------------------------------------------------------------- semantics

fn semantics(t: &mut T) {
    let cases: &[(&str, &str)] = &[
        // the conditional: truthy as if, one branch runs
        ("print(1 ? \"a\" : \"b\", nil ? \"a\" : \"b\", false ? 1 : 0 ? 2 : 3)", "a b 2\n"),
        ("let n = 0\nfn inc() { n += 1; return n }\nlet v = true ? 1 : inc()\nlet w = nil ? inc() : 2\nprint(v, w, n)", "1 2 0\n"),
        ("let a = nil\nprint(a ?? 0 ? \"x\" : \"y\")", "x\n"),
        // defaults: for a missing argument and for nil, evaluated at each call, in order
        ("fn f(a, b = 2, c = a + b) { return [a, b, c] }\nprint(f(1), f(1, 10), f(1, nil, 0))", "[1, 2, 3] [1, 10, 11] [1, 2, 0]\n"),
        ("fn g(x = 1) { return x }\nprint(g(false), g(nil), g(0), g())", "false 1 0 1\n"),
        ("fn h(l = []) { l.push(1); return len(l) }\nprint(h(), h())", "1 1\n"),
        ("fn k(a, f = fn() { return a }) { return f() }\nprint(k(5))", "5\n"),
        ("fn c(x = 3) { return fn() { return x } }\nprint(c()(), c(4)())", "3 4\n"),
        ("let lit = fn(a, b = a * 2) { return a + b }\nprint(lit(1), lit(1, 1))", "3 2\n"),
        // destructuring: missing items and keys are nil, extra ones are left
        ("let [a, b, c] = [1]\nprint(a, b, c)", "1 nil nil\n"),
        ("let [a] = [1, 2, 3]\nprint(a)", "1\n"),
        ("let [h, ...r] = [1, 2, 3]\nlet [...all] = []\nlet [x, ...e] = []\nprint(h, r, all, x, e)", "1 [2, 3] [] nil []\n"),
        ("let {a, b: x, c = 3, d} = {a: 1, b: 2}\nprint(a, x, c, d)", "1 2 3 nil\n"),
        ("let [[a, b], {c: [d, e]}] = [[1, 2], {c: [3]}]\nprint(a, b, d, e)", "1 2 3 nil\n"),
        ("let [a = 10, [b = 20] = []] = [nil]\nprint(a, b)", "10 20\n"),
        ("let {\"content-type\": ct, await: w} = {\"content-type\": \"x\", await: 1}\nprint(ct, w)", "x 1\n"),
        ("let [g1, g2] = [1, 2]\nfn s() { return g1 + g2 }\nprint(s())", "3\n"),
        ("fn f() { let [a, b] = [1, 2]; let g = fn() { return a + b }; a = 10; return g() }\nprint(f())", "12\n"),
        ("let {w = 1, h = w * 2} = {w: 3}\nprint(w, h)", "3 6\n"),
        // parameters
        ("fn area({w, h = 1}) { return w * h }\nprint(area({w: 3}), area({w: 3, h: 2}))", "3 6\n"),
        ("fn f([x, y] = [1, 2]) { return x + y }\nprint(f(), f([5, 5]))", "3 10\n"),
        ("fn f(a, [b, c], {d}) { return fn() { return a + b + c + d } }\nprint(f(1, [2, 3], {d: 4})())", "10\n"),
        // for
        ("for [i, p] in [[1, \"a\"], [2, \"b\"]] { print(i, p) }", "1 a\n2 b\n"),
        ("for k, [a, b] in {x: [1, 2]} { print(k, a, b) }", "x 1 2\n"),
        ("for i, {n} in [{n: \"a\"}] { print(i, n) }", "0 a\n"),
        ("let fs = []\nfor [a, b] in [[1, 2], [3, 4]] { fs.push(fn() { return a + b }) }\nprint(fs[0](), fs[1]())", "3 7\n"),
        // assignment through a list pattern
        ("let a = 1\nlet b = 2\n[a, b] = [b, a]\nprint(a, b)", "2 1\n"),
        ("let m = {}\nlet l = [0, 0]\n[m.x, l[1]] = [5, 6]\nprint(m, l)", "{x: 5} [0, 6]\n"),
        ("let a = 0\nlet b = 0\n[[a], b] = [[1], 2]\nprint(a, b)", "1 2\n"),
        // the shorthand
        ("let a = 1\nlet b = \"x\"\nprint({a, b, c: 3})", "{a: 1, b: \"x\", c: 3}\n"),
        // lists from a loop
        ("print([x * 2 for x in [1, 2, 3, 4] if x > 1])", "[4, 6, 8]\n"),
        ("print([i for i in 0..4], [c for c in \"ab\"], [k + str(v) for k, v in {a: 1, b: 2}])", "[0, 1, 2, 3] [\"a\", \"b\"] [\"a1\", \"b2\"]\n"),
        ("print([a + b for a in [1, 2] for b in [10, 20]])", "[11, 21, 12, 22]\n"),
        ("print([a * b for [a, b] in [[1, 2], [3, 4]]], [n for {n} in [{n: 1}]])", "[2, 12] [1]\n"),
        ("print([b for a in [[1, 2], [3]] for b in a if b != 2])", "[1, 3]\n"),
        ("print([[j for j in 0..i] for i in 0..3])", "[[], [0], [0, 1]]\n"),
        ("let x = 5\nlet l = [x for x in [1]]\nprint(x, l)", "5 [1]\n"),
        ("let fs = [fn() { return i } for i in 0..3]\nprint(fs[0](), fs[2]())", "0 2\n"),
        ("fn f(l = [x * x for x in 0..3]) { return l }\nprint(f())", "[0, 1, 4]\n"),
        ("fn f(n) { return [i + n for i in 0..n if i % 2 == 0] }\nprint(f(5), f(0))", "[5, 7, 9] []\n"),
        ("print(len([x for x in 0..1000]))", "1000\n"),
        ("let l = [1, 2]\nlet r = [x for x in l if len(l) < 4 && l.push(0) == nil]\nprint(r, l)", "[1, 2] [1, 2, 0, 0]\n"),
    ];
    for (src, want) in cases {
        t.eq(&format!("run {}", src), out(src).as_str(), *want);
    }
}

fn errors(t: &mut T) {
    let cases: &[(&str, &str)] = &[
        ("let x = 1\nlet [a] = 5", "2:5: Das Muster [..] braucht eine Liste, nicht int"),
        ("let {a} = \"s\"", "1:6: Das Muster {..} braucht eine Map oder ein Objekt des Hosts, nicht str"),
        ("let [a, ...r] = nil", "1:5: Das Muster [..] braucht eine Liste, nicht nil"),
        ("fn f(x, [y]) { return y }\nf(1, 2)", "1:9: Das Muster [..] braucht eine Liste, nicht int"),
        ("for [a] in [1] {}", "1:5: Das Muster [..] braucht eine Liste, nicht int"),
        ("print([x for x in 5])", "1:19: for braucht list, map, str oder bytes, nicht int"),
        ("print([x for [x] in [nil]])", "1:14: Das Muster [..] braucht eine Liste, nicht nil"),
        ("let f = fn(a) {}\nf(b: 1)", "2:1: f hat keinen Parameter b"),
        ("fn f(a) {}\nlet g = f\ng(1, a: 2)", "3:1: f: der Parameter a hat schon ein Argument"),
        ("fn g(width) {}\nlet h = g\nh(widht: 1)", "3:1: g hat keinen Parameter widht – meinst du width?"),
        ("let o = {m: fn(x) { return x }}\no.m(y: 1)", "2:3: m hat keinen Parameter y"),
        ("[fn(q) {}][0](z: 1)", "1:1: Die Funktion hat keinen Parameter z"),
        ("let a = 1\n[a, b] = [1, 2]", "2:5: b ist nicht deklariert – lege die Variable mit let b = … an"),
    ];
    for (src, want) in cases {
        t.eq(&format!("error {}", src), out(src).as_str(), *want);
    }
    // catchable
    eqs(t, "catch pattern", out("try { let [a] = 1 } catch e { print(\"caught\", e) }"), "caught Das Muster [..] braucht eine Liste, nicht int\n");
    eqs(t, "catch named", out("fn f(a) {}\nlet g = f\ntry { g(b: 1) } catch e { print(e) }"), "f hat keinen Parameter b\n");
    // the trace of an error in a pattern parameter
    let p = script::compile_named("t.jfs", "fn f([x]) { return x }\nf(1)").expect("compiles");
    let mut vm = Vm::new(&p, Limits::default());
    match vm.run(&mut (), u64::MAX) {
        Status::Error(e) => t.eq("trace of a pattern parameter", (e.line, e.col, e.trace.len(), e.trace[0].func.as_str()), (1, 6, 2, "f")),
        s => t.check(&format!("pattern parameter error: {:?}", s), false),
    }
}

// ---------------------------------------------------------------- named arguments

fn named_script(t: &mut T) {
    let base = "fn f(a, b, c) { return [a, b, c] }\nfn g(a, b = 2, c = 3) { return [a, b, c] }\n";
    let cases: &[(&str, &str)] = &[
        ("print(f(1, c: 3), f(c: 3, a: 1), f(b: 2))", "[1, nil, 3] [1, nil, 3] [nil, 2, nil]\n"),
        ("print(g(1, c: 30), g(a: 0), g(b: nil, a: 1))", "[1, 2, 30] [0, 2, 3] [1, 2, 3]\n"),
        ("print(f(1, 2, 3, 4))\nprint(f(1, 2, 3, 4, c: 5))", "[1, 2, 3]\n4:7: f: der Parameter c hat schon ein Argument"),
        ("let o = {m: fn(x, y = 1) { return x * y }}\nprint(o.m(x: 3, y: 4), o.m(3))", "12 3\n"),
        ("fn apply(h) { return h(y: 2, x: 1) }\nprint(apply(fn(x, y) { return x - y }))", "-1\n"),
        ("fn p([a], b) { return [a, b] }\nprint(p([1], b: 2))", "[1, 2]\n"),
        ("fn fact(n, acc = 1) { return n <= 1 ? acc : fact(n - 1, acc: acc * n) }\nprint(fact(5))", "120\n"),
        ("fn h(a, b) { return b }\nlet r = h(b: [x for x in 0..2], a: nil)\nprint(r)", "[0, 1]\n"),
    ];
    for (src, want) in cases {
        let got = out(&format!("{}{}", base, src));
        t.eq(&format!("named {}", src), got.as_str(), *want);
    }
    // the order of evaluation is the source's
    eqs(t, "named order", out("let log = []\nfn v(x) { log.push(x); return x }\nfn f(a, b, c) { return [a, b, c] }\nprint(f(v(1), c: v(3), b: v(2)), log)"), "[1, 2, 3] [1, 3, 2]\n");
}

/// the host of the named-argument checks: what Host::call and Host::method get, objects of
/// mesh (identity) and vec (value type)
#[derive(Default)]
struct Geo {
    log: Vec<String>,
}

impl Host for Geo {
    fn call(&mut self, cx: &mut Cx, name: &str, args: &[Value]) -> Result<Value, Fail> {
        let shown: Vec<String> = args.iter().map(|v| cx.display(*v)).collect();
        self.log.push(format!("{}({})", name, shown.join(", ")));
        match name {
            "box" | "geom.cyl" => cx.new_object("mesh", self.log.len() as u64),
            "vec" => {
                let data: Vec<Value> = match args.first().and_then(|v| cx.list(*v)) {
                    Some(l) => l.to_vec(),
                    None => args.to_vec(),
                };
                cx.new_value("vec", &data)
            }
            "note" => Ok(Value::Nil),
            _ => Err(Fail::Unknown),
        }
    }

    fn knows(&self, name: &str) -> bool {
        name == "box"
    }

    fn method(&mut self, cx: &mut Cx, obj: Object, name: &str, args: &[Value]) -> Result<Value, Fail> {
        let shown: Vec<String> = args.iter().map(|v| cx.display(*v)).collect();
        self.log.push(format!("{}.{}({})", obj.ty, name, shown.join(", ")));
        match (obj.ty, name) {
            ("mesh", "bevel" | "scale") => cx.new_object("mesh", 100 + self.log.len() as u64),
            _ => Err(Fail::Unknown),
        }
    }

    fn get(&mut self, cx: &mut Cx, obj: Object, name: &str) -> Result<Value, Fail> {
        match (obj.ty, name) {
            ("vec", "x") => Ok(cx.data(obj.value).and_then(|d| d.first().copied()).unwrap_or(Value::Nil)),
            ("vec", "y") => Ok(cx.data(obj.value).and_then(|d| d.get(1).copied()).unwrap_or(Value::Nil)),
            ("mesh", "faces") => Ok(Value::Int(6)),
            _ => Err(Fail::Unknown),
        }
    }
}

fn geo_api() -> Api {
    Api::new("geo")
        .func("box(w: float, h: float = 1, d?: float): mesh", "Ein Quader.")
        .func("vec(x: float, y: float, z?: float): vec", "Ein Vektor aus Zahlen.")
        .func("vec(v: list): vec", "Ein Vektor aus einer Liste.")
        .func("note(...values): nil", "Eine Notiz.")
        .func("pts(): list<vec>", "Punkte.")
        .namespace("geom", "Formen.")
        .func("geom.cyl(r: float, h: float): mesh", "Ein Zylinder.")
        .typ(
            Type::new("mesh", "Ein Netz.")
                .prop("readonly faces: int", "Flächen.")
                .method("bevel(width: float, segments: int = 1): mesh", "Abgeschrägt.")
                .method("scale(sx: float, sy: float, sz: float): mesh", "Je Achse skaliert.")
                .method("scale(s: float): mesh", "In allen Achsen skaliert."),
        )
        .typ(Type::new("vec", "Ein Vektor.").value().prop("readonly x: float", "x").prop("readonly y: float", "y"))
}

/// compile with the description and run: (the host's log, the output or the error)
fn geo_run(src: &str) -> (Vec<String>, String) {
    let api = geo_api();
    let c = match script::compile_api("g.jfs", src, &api) {
        Ok(c) => c,
        Err(d) => return (Vec::new(), format!("ERR {}", d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("; "))),
    };
    let mut vm = Vm::new(&c.program, Limits::default());
    let mut h = Geo::default();
    let r = loop {
        match vm.run(&mut h, u64::MAX) {
            Status::Yielded | Status::Paused(_) => {}
            Status::Done(_) => break vm.take_output(),
            Status::Error(e) => break format!("{}{}:{}: {}", vm.take_output(), e.line, e.col, e.msg),
        }
    };
    (h.log, r)
}

fn named_host(t: &mut T) {
    let s = |v: &[&str]| v.iter().map(|x| String::from(*x)).collect::<Vec<_>>();
    // the host gets the values by position in the form's order, nil in the gaps
    t.eq("host by name", geo_run("box(1, d: 3)\nbox(w: 2)\nbox(d: 1, w: 2)\nbox(1, 2, 3)").0, s(&["box(1, nil, 3)", "box(2)", "box(2, nil, 1)", "box(1, 2, 3)"]));
    t.eq("host namespace", geo_run("geom.cyl(h: 2, r: 1)").0, s(&["geom.cyl(1, 2)"]));
    t.eq("host forms", geo_run("vec(v: [1, 2])\nvec(y: 2, x: 1)\nvec(1, 2, z: 3)").0, s(&["vec([1, 2])", "vec(1, 2)", "vec(1, 2, 3)"]));
    // a gap is nil only where the parameter may be left out: y of vec(x, y, z?) may not
    eqs(t, "host gap required", geo_run("vec(1, z: 3)").1, "ERR 1:1: Fehler: vec: der Parameter y fehlt");
    t.eq("host rest", geo_run("note(1, 2)").0, s(&["note(1, 2)"]));
    t.eq("host method", geo_run("let m = box(1)\nm.bevel(width: 0.1)\nm.bevel(segments: 3, width: 0.2)").0, s(&["box(1)", "mesh.bevel(0.1)", "mesh.bevel(0.2, 3)"]));
    t.eq("host value of a name", geo_run("let b = box\nb(d: 5, w: 1)").0, s(&["box(1, nil, 5)"]));
    // destructuring a host object reads its properties
    eqs(t, "host pattern", geo_run("let {x, y} = vec(y: 2, x: 1)\nlet {faces} = box(1)\nprint(x, y, faces)").1, "1 2 6\n");
    eqs(t, "host pattern unknown", geo_run("let {q} = box(1)").1, "1:6: mesh hat keine Eigenschaft q");
    // compile errors where the description shows them
    eqs(t, "host no param", geo_run("box(dd: 1)").1, "ERR 1:5: Fehler: box hat keinen Parameter dd");
    eqs(t, "host twice", geo_run("box(1, w: 2)").1, "ERR 1:8: Fehler: box: der Parameter w hat schon ein Argument");
    eqs(t, "host too many", geo_run("geom.cyl(1, 2, 3, h: 1)").1, "ERR 1:6: Fehler: geom.cyl: 3 Argumente vor den benannten, nimmt aber höchstens 2");
    // a method of a known type: the services warn (compile_api's warnings)
    let api = geo_api();
    let w = script::compile_api("g.jfs", "let m = box(1)\nm.bevel(widht: 1)", &api).map(|c| c.warnings.iter().map(|d| d.to_string()).collect::<Vec<_>>());
    t.eq("host method warning", w, Ok(s(&["2:9: Warnung: mesh.bevel hat keinen Parameter widht – meinst du width?"])));
    // at run time: a method's name that no form takes
    eqs(t, "host method run", geo_run("let m = [box(1)][0]\nm.bevel(widht: 1)").1, "2:3: mesh.bevel hat keinen Parameter widht – meinst du width?");
    // a host's form leaves out no parameter that may not be left out: scale(sx: 2) of
    // scale(sx, sy, sz) would reach the host as scale(2), the other form's call
    t.eq("host forms by name", geo_run("let m = box(1)\nm.scale(s: 2)\nm.scale(sz: 3, sx: 1, sy: 2)").0, s(&["box(1)", "mesh.scale(2)", "mesh.scale(1, 2, 3)"]));
    eqs(t, "host required left out", geo_run("geom.cyl(r: 1)").1, "ERR 1:6: Fehler: geom.cyl: der Parameter h fehlt");
    let w = script::compile_api("g.jfs", "let m = box(1)\nm.scale(sx: 2)", &api).map(|c| c.warnings.iter().map(|d| d.to_string()).collect::<Vec<_>>());
    t.eq("host method required warning", w, Ok(s(&["2:3: Warnung: mesh.scale: der Parameter sy fehlt"])));
    eqs(t, "host method required run", geo_run("let m = [box(1)][0]\nm.scale(sx: 2)").1, "2:3: mesh.scale: der Parameter sy fehlt");
    // without a description: an error at compile time where the call shows it, else at run time
    let plain = script::compile_with("p.jfs", "box(w: 1)", &["box"]);
    t.eq("no description", plain.err().map(|d| d[0].to_string()), Some(String::from("1:1: Fehler: box: benannte Argumente gehen an Funktionen des Hosts nur mit seiner Beschreibung (script::Api, compile_api)")));
    let p = script::compile_with("p.jfs", "let b = box\nlet m = b(1)\ntry { b(w: 1) } catch e { print(e) }\ntry { m.bevel(width: 1) } catch e { print(e) }", &["box"]).expect("compiles");
    let mut vm = Vm::new(&p, Limits::default());
    let mut h = Geo::default();
    let st = vm.run(&mut h, u64::MAX);
    t.check("no description runs", matches!(st, Status::Done(_)));
    t.eq(
        "no description at run time",
        vm.take_output(),
        String::from("box: benannte Argumente gehen an Funktionen des Hosts nur mit seiner Beschreibung (script::Api, compile_api)\nmesh.bevel: benannte Argumente gehen an Methoden des Hosts nur mit seiner Beschreibung (script::Api, compile_api)\n"),
    );
    // on / off by their names
    eqs(t, "on by name", geo_run("let m = box(1)\ntry { m.on(f: fn(e) {}, event: \"x\") } catch e { print(e) }").1, "mesh hat kein Ereignis x\n");
}

fn named_library(t: &mut T) {
    let cases: &[(&str, &str)] = &[
        ("print(math.clamp(5, lo: 0, hi: 3), math.clamp(x: -1, hi: 9, lo: 0))", "3 0\n"),
        ("print(\"a,b,c\".split(sep: \",\", n: 2), str.split(\"a b\", sep: \" \"))", "[\"a\", \"b,c\"] [\"a\", \"b\"]\n"),
        ("print(list.range(0, 10, step: 3), [3, 1, 2].sort(cmp: fn(a, b) { return b - a }))", "[0, 3, 6, 9] [3, 2, 1]\n"),
        ("print({a: 1}.get(k: \"b\", default: 0), len(x: [1, 2]), \"x\".repeat(n: 3))", "0 2 xxx\n"),
        ("print(json.encode({a: 1}, indent: 1))", "{\n \"a\": 1\n}\n"),
        ("let s = \"a-b\"\ntry { s.split(sp: \"-\") } catch e { print(e) }", "str.split hat keinen Parameter sp – meinst du sep?\n"),
        ("let c = math.clamp\ntry { c(1, low: 0) } catch e { print(e) }", "math.clamp hat keinen Parameter low – meinst du lo?\n"),
        ("let l = [1]\nl.push(x: 2)\nprint(l)", "[1, 2]\n"),
        ("let mn = math.min\ntry { mn(1, a: 2) } catch e { print(e) }", "math.min: der Parameter a hat schon ein Argument\n"),
        ("math.min(1, a: 2)", "ERR 1:13: Fehler: math.min: der Parameter a hat schon ein Argument"),
    ];
    for (src, want) in cases {
        t.eq(&format!("library {}", src), out(src).as_str(), *want);
    }
    // a map's own function wins over the library's method, as without names
    eqs(t, "library map field", out("let m = {get: fn(key) { return key }}\nprint(m.get(key: 7))"), "7\n");
}

// ---------------------------------------------------------------- slices, the debugger

const SLICED: &str = r#"fn f(a, b = 2, {c, d = [1]} = {c: 3}) { return [a, b, c, d] }
let [p, q, ...r] = [1, 2, 3, 4, 5]
let {x, y: z = 9} = {x: p}
let m = {p, q, r}
let l = [i * j for i in 0..6 if i % 2 == 0 for j in [p, q] if j != 0]
let fs = [fn() { return k } for [k] in [[7], [8]]]
let sum = 0
for [u, v] in [[1, 2], [3, 4]] { sum += u * v }
[p, q] = [q, p]
let c = sum > 10 ? "big" : sum > 5 ? "mid" : "small"
print(f(1), f(0, nil, {c: 1, d: 2}), f(b: 5, a: 1), x, z, m, l, fs[1](), sum, p, q, c)
print(math.clamp(7, lo: 0, hi: 5), "a b".split(sep: " "), [s.upper() for s in ["x", "y"]])
"#;

const SLICED_OUT: &str = "[1, 2, 3, [1]] [0, 2, 1, 2] [1, 5, 3, [1]] 1 9 {p: 1, q: 2, r: [3, 4, 5]} [0, 0, 2, 4, 4, 8] 8 14 2 1 big\n5 [\"a\", \"b\"] [\"X\", \"Y\"]\n";

fn sliced(vm: &mut Vm, h: &mut dyn Host, slice: u64) -> (String, u64) {
    loop {
        match vm.run(h, slice) {
            Status::Yielded | Status::Paused(_) => {}
            Status::Done(_) => return (vm.take_output(), vm.call_fuel()),
            Status::Error(e) => return (format!("{}{}", vm.take_output(), e), vm.call_fuel()),
        }
    }
}

fn slices(t: &mut T) {
    let p = script::compile_named("s.jfs", SLICED).expect("compiles");
    let mut vm = Vm::new(&p, Limits::default());
    let whole = sliced(&mut vm, &mut (), u64::MAX);
    t.eq("sliced: output", whole.0.as_str(), SLICED_OUT);
    for slice in [1, 2, 3, 7] {
        let mut vm = Vm::new(&p, Limits::default());
        t.eq(&format!("sliced: slices of {}", slice), sliced(&mut vm, &mut (), slice), whole.clone());
    }
    // named arguments to the host through its description, sliced
    let src = "let m = box(1, d: 3)\nlet n = m.bevel(segments: 2, width: 0.5)\nlet {x, y} = vec(y: 2, x: 1)\nprint(x, y, [vec(v: [i, i]) for i in 0..2])";
    let c = script::compile_api("g.jfs", src, &geo_api()).expect("compiles");
    let mut vm = Vm::new(&c.program, Limits::default());
    let mut h = Geo::default();
    let whole = sliced(&mut vm, &mut h, u64::MAX);
    t.eq("sliced host: output", whole.0.as_str(), "1 2 [vec(0, 0), vec(1, 1)]\n");
    let log = h.log.clone();
    let mut vm = Vm::new(&c.program, Limits::default());
    let mut h = Geo::default();
    t.eq("sliced host: slices of 1", (sliced(&mut vm, &mut h, 1), h.log), (whole, log));
}

fn debugger(t: &mut T) {
    let src = "fn f([a, b], {c}, d = 4) {\n  let [e, ...g] = [a, b, c]\n  return [e, g, d]\n}\nprint(f([1, 2], {c: 3}))";
    let p = script::compile_named("d.jfs", src).expect("compiles");
    let mut vm = Vm::new(&p, Limits::default());
    vm.set_breakpoint("d.jfs", 3, None).expect("breakpoint");
    match vm.run(&mut (), u64::MAX) {
        Status::Paused(_) => {
            let mut names: Vec<(String, String)> = vm.locals(0).into_iter().map(|(n, v)| (n, vm.display(v))).collect();
            names.sort();
            let want: Vec<(String, String)> = [("a", "1"), ("b", "2"), ("c", "3"), ("d", "4"), ("e", "1"), ("g", "[2, 3]")].iter().map(|(n, v)| (String::from(*n), String::from(*v))).collect();
            t.eq("debugger: destructured locals", names, want);
            t.eq("debugger: eval", vm.eval(0, "e > 0 ? d : -d").map(|v| vm.display(v)), Ok(String::from("4")));
        }
        s => t.check(&format!("debugger: pauses ({:?})", s), false),
    }
    vm.clear_breakpoints();
    t.check("debugger: ends", matches!(vm.run(&mut (), u64::MAX), Status::Done(_)));
    t.eq("debugger: output", vm.take_output(), String::from("[1, [2, 3], 4]\n"));
}

// ---------------------------------------------------------------- the language services

fn services(t: &mut T) {
    let cx = ls::CompleteCx::default();
    // references and rename of destructured and comprehension names
    let src = "let [a, b] = [1, 2]\nprint(a)\nfn f() { return a + [a for a in [b]][0] }";
    let r = ls::references(src, at(src, "a", 0, 0));
    t.eq("refs destructured", r.iter().map(|s| (s.line, s.col)).collect::<Vec<_>>(), vec![(1, 6), (2, 7), (3, 17)]);
    let r = ls::references(src, at(src, "a for", 0, 0));
    t.eq("refs comprehension", r.iter().map(|s| (s.line, s.col)).collect::<Vec<_>>(), vec![(3, 22), (3, 28)]);
    let e = ls::rename(src, at(src, "a for", 0, 0), "z").map(|e| ls::apply(src, &e));
    t.eq("rename comprehension", e.as_deref(), Ok("let [a, b] = [1, 2]\nprint(a)\nfn f() { return a + [z for z in [b]][0] }"));
    // a shorthand keeps its key
    let src = "let {w, h} = {w: 1, h: 2}\nlet a = 1\nprint({a, w})";
    let e = ls::rename(src, at(src, "w", 0, 0), "width").map(|e| ls::apply(src, &e));
    t.eq("rename map pattern shorthand", e.as_deref(), Ok("let {w: width, h} = {w: 1, h: 2}\nlet a = 1\nprint({a, w: width})"));
    let e = ls::rename(src, at(src, "a = 1", 0, 0), "b").map(|e| ls::apply(src, &e));
    t.eq("rename map literal shorthand", e.as_deref(), Ok("let {w, h} = {w: 1, h: 2}\nlet b = 1\nprint({a: b, w})"));
    // a parameter and the named arguments of calls of its fn
    let src = "fn f(width, h = 1) { return width * h }\nprint(f(width: 2), f(1, h: 3))";
    let e = ls::rename(src, at(src, "width", 0, 0), "w").map(|e| ls::apply(src, &e));
    t.eq("rename parameter", e.as_deref(), Ok("fn f(w, h = 1) { return w * h }\nprint(f(w: 2), f(1, h: 3))"));
    let e = ls::rename(src, at(src, "h: 3", 0, 0), "height").map(|e| ls::apply(src, &e));
    t.eq("rename from a named argument", e.as_deref(), Ok("fn f(width, height = 1) { return width * height }\nprint(f(width: 2), f(1, height: 3))"));
    let d = ls::definition(src, at(src, "width: 2", 0, 1), &cx);
    t.eq("definition of a named argument", d.map(|l| (l.span.line, l.span.col)), Some((1, 6)));
    // not the parameter of a fn literal inside it; a let of a fn literal is its fn
    let src = "fn f(a) { return g(fn(width) { return width }) }\nf(width: 1)\nlet h = fn(size) { return size }\nh(size: 2)";
    t.eq("refs of an inner literal's parameter", ls::references(src, at(src, "width", 0, 0)).len(), 2);
    t.eq("refs of a let fn's parameter", ls::references(src, at(src, "size", 0, 0)).iter().map(|s| s.line).collect::<Vec<_>>(), vec![3, 3, 4]);
    // hover
    let src = "let [a, b] = [1, 2]\nlet l = [x * 2 for x in [a]]\nprint(math.clamp(1, lo: 0))";
    t.eq("hover destructured", ls::hover(src, at(src, "a", 0, 0), &cx).map(|h| (h.sig, h.kind)), Some((String::from("let a"), String::from("globale Variable"))));
    t.eq("hover comprehension", ls::hover(src, at(src, "x * 2", 0, 0), &cx).map(|h| (h.sig, h.kind)), Some((String::from("for x"), String::from("Schleifenvariable"))));
    t.eq("hover named library", ls::hover(src, at(src, "lo:", 0, 1), &cx).map(|h| (h.sig, h.kind)), Some((String::from("lo"), String::from("Parameter von math.clamp"))));
    let src = "fn f(a, b = 2, [c, d]) {}\nf(1, b: 3)";
    t.eq("hover fn with defaults", ls::hover(src, at(src, "f(", 0, 0), &cx).map(|h| h.sig), Some(String::from("fn f(a, b = 2, [c, d])")));
    t.eq("hover named script", ls::hover(src, at(src, "b: 3", 0, 0), &cx).map(|h| (h.sig, h.kind)), Some((String::from("b"), String::from("Parameter von f"))));
    // completion of parameter names in a call
    let names = |src: &str, pos: u32, cx: &Context| ls::complete(src, pos, *cx).items.iter().filter(|i| i.kind == ItemKind::Param && i.label.ends_with(':')).map(|i| i.label.clone()).collect::<Vec<_>>();
    let plain = Context::default();
    let src = "fn f(width, height) {}\nf(";
    t.eq("complete params", names(src, src.len() as u32, &plain), vec![String::from("height:"), String::from("width:")]);
    let src = "fn f(width, height) {}\nf(1, ";
    t.eq("complete params after one", names(src, src.len() as u32, &plain), vec![String::from("height:")]);
    let src = "fn f(width, height) {}\nf(height: 1, w";
    t.eq("complete params named", names(src, src.len() as u32, &plain), vec![String::from("width:")]);
    let src = "print(math.clamp(1, ";
    t.eq("complete params library", names(src, src.len() as u32, &plain), vec![String::from("hi:"), String::from("lo:")]);
    let api = geo_api();
    let acx = Context::new(&api);
    let src = "box(";
    t.eq("complete params api", names(src, src.len() as u32, &acx), vec![String::from("d:"), String::from("h:"), String::from("w:")]);
    let src = "let m = box(1)\nm.bevel(";
    t.eq("complete params method", names(src, src.len() as u32, &acx), vec![String::from("segments:"), String::from("width:")]);
    // signature help: the active parameter of a named argument
    let src = "fn f(width, height, depth) {}\nf(1, depth: ";
    t.eq("signature named", ls::signature(src, src.len() as u32, &cx).map(|s| s.active), Some(2));
    let src = "box(d: ";
    t.eq("signature named api", ls::signature(src, src.len() as u32, acx).map(|s| (s.active, s.label)), Some((2, String::from("box(w: float, h: float = 1, d?: float): mesh"))));
    let src = "vec(v: ";
    t.eq("signature named picks the form", ls::signature(src, src.len() as u32, acx).map(|s| (s.form, s.active)), Some((1, 0)));
    // types through destructuring and comprehensions
    let src = "let {x, y} = vec(1, 2)\nlet [m] = [box(1)]\nlet ms = [box(i) for i in 0..3]\nfor {x: px} in pts() { print(px) }";
    t.eq("type let map pattern", ls::hover(src, at(src, "x", 0, 0), acx).map(|h| h.sig), Some(String::from("let x: float")));
    t.eq("type comprehension", ls::hover(src, at(src, "ms", 0, 0), acx).map(|h| h.sig), Some(String::from("let ms: list<mesh>")));
    t.eq("type for pattern", ls::hover(src, at(src, "px", 0, 0), acx).map(|h| h.sig), Some(String::from("for px: float")));
    let src = "let m2 = box(1)\n[m2] = [5]\nprint(m2)";
    t.eq("type lost through a pattern assignment", ls::hover(src, at(src, "m2", 0, 0), acx).map(|h| h.sig), Some(String::from("let m2")));
    let src = "let ms = [box(i) for i in 0..3]\nms[0].";
    let c = ls::complete(src, src.len() as u32, acx);
    t.check("complete through a comprehension", c.items.iter().any(|i| i.label == "bevel"));
    // ls::check with a description: a named argument of a function is an error, of a method a warning
    let d: Vec<String> = ls::check("box(dd: 1)\nlet m = box(1)\nm.bevel(segs: 2)", acx).iter().map(|x| x.to_string()).collect();
    t.eq("check api", d, vec![String::from("1:5: Fehler: box hat keinen Parameter dd"), String::from("3:9: Warnung: mesh.bevel hat keinen Parameter segs")]);
    // the outline and folding
    let s = ls::symbols("let [a, b] = [1, 2]\nfn f() {}");
    t.eq("symbols patterns", s.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["a", "b", "f"]);
    let f = ls::folding("let l = [\n  x\n  for x in xs\n]");
    t.check("folding comprehension", f.iter().any(|x| x.line == 1 && x.end == 3));
}

// ---------------------------------------------------------------- the formatter

fn formatter(t: &mut T) {
    let cases: &[(&str, &str)] = &[
        ("let x=c?a:b", "let x = c ? a : b"),
        ("let y = a??b ? c : d", "let y = a ?? b ? c : d"),
        ("let z = c\n? a\n: b", "let z = c\n  ? a\n  : b"),
        ("let v=a?b?1:2:-3", "let v = a ? b ? 1 : 2 : -3"),
        ("f(x,width:2)", "f(x, width: 2)"),
        ("f(\nw:1,\nh:c?1:2\n)", "f(\n  w: 1,\n  h: c ? 1 : 2\n)"),
        ("fn f(a,b=2,[x,y],{w,h=1}){}", "fn f(a, b = 2, [x, y], {w, h = 1}) {}"),
        ("let [a,...r]=l", "let [a, ...r] = l"),
        ("let {a,b:c,\"k\":[d]}=m", "let {a, b: c, \"k\": [d]} = m"),
        ("for [i,p] in pairs{print(i)}", "for [i, p] in pairs {\n  print(i)\n}"),
        ("for k,{x} in m{}", "for k, {x} in m {}"),
        ("[a,b]=[b,a]", "[a, b] = [b, a]"),
        ("let m={a,b,c:1}", "let m = {a, b, c: 1}"),
        ("let l=[x*2 for x in xs if x>0]", "let l = [x * 2 for x in xs if x > 0]"),
        ("let n=[a+b for a in xs for b in 0..3]", "let n = [a + b for a in xs for b in 0..3]"),
        ("let m={k:c?1:-2}", "let m = {k: c ? 1 : -2}"),
        ("print(c ? {a: 1} : f(x: 1))", "print(c ? {a: 1} : f(x: 1))"),
        ("let s=\"${c?1:2}\"", "let s = \"${c ? 1 : 2}\""),
        ("let l = [ // all\n  x for x in xs // here\n]", "let l = [ // all\n  x for x in xs // here\n]"),
        ("fn f(cb=fn(){return 1}){}", "fn f(cb = fn() {\n  return 1\n}) {}"),
        ("let x = c /* a */ ? 1 /* b */ : 2 // c", "let x = c /* a */ ? 1 /* b */ : 2 // c"),
        ("fn f(a, // first\nb = /* two */ 2 // zwei\n) {}", "fn f(a, // first\n  b = /* two */ 2 // zwei\n) {}"),
        ("let {a, // eins\nb: [c, ...d]} = m", "let {a, // eins\n  b: [c, ...d]} = m"),
        ("f(\n// vorher\nw: 1 // w\n)", "f(\n  // vorher\n  w: 1 // w\n)"),
    ];
    for (i, (src, want)) in cases.iter().enumerate() {
        let got = script::fmt::format(src);
        t.eq(&format!("format s3 {}", i), got.as_deref(), Ok(*want));
        if let Ok(g) = &got {
            t.eq(&format!("format s3 idempotent {}", i), script::fmt::format(g).as_deref(), Ok(g.as_str()));
        }
    }
    let f = script::fmt::format(SLICED);
    t.check("format s3 program", f.as_deref().is_ok_and(|g| script::parse("", g).map(|p| p.sexpr()) == script::parse("", SLICED).map(|p| p.sexpr())));
}

// ---------------------------------------------------------------- int division, await

fn intdiv(t: &mut T) {
    let warns = |src: &str| script::check(src).iter().filter(|d| d.msg.starts_with("Ganzzahlige Division")).map(|d| d.line).collect::<Vec<_>>();
    t.eq("intdiv cond", warns("let c = true\nlet n = c ? 7 : 3\nprint(n / 2)"), vec![3]);
    t.eq("intdiv cond float", warns("let c = true\nlet n = c ? 7 : 3.0\nprint(n / 2)"), Vec::<u32>::new());
    t.eq("intdiv cond direct", warns("let c = true\nprint((c ? 7 : 3) / 2)"), vec![2]);
    t.eq("intdiv pattern", warns("let [a, b] = [7, 2]\nprint(a / b)"), Vec::<u32>::new());
    t.eq("intdiv map pattern", warns("let {a = 7} = {}\nprint(a / 2)"), Vec::<u32>::new());
    t.eq("intdiv comprehension range", warns("print([i / 2 for i in 0..4])"), vec![1]);
    t.eq("intdiv comprehension list", warns("print([x / 2 for x in [1, 2]])"), Vec::<u32>::new());
    t.eq("intdiv default", warns("fn f(a = 1) { return a / 2 }"), Vec::<u32>::new());
    t.eq("intdiv assigned by pattern", warns("let n = 7\n[n] = [2.5]\nprint(n / 2)"), Vec::<u32>::new());
    t.eq("intdiv int named", warns("let n = 7\nprint(int(x: n / 2))"), Vec::<u32>::new());
    t.eq("intdiv comprehension var assigned", warns("let fs = [fn() { i = 1.5; return i / 2 } for i in 0..3]"), Vec::<u32>::new());
    t.eq("intdiv default divides", warns("let n = 7\nfn f(a = n / 2) { return a }"), vec![2]);
}

fn awaits(t: &mut T) {
    let warns = |src: &str| script::check(src).iter().filter(|d| d.msg.contains("await")).map(|d| (d.line, d.col)).collect::<Vec<_>>();
    t.eq("await in a comprehension", warns("fn load(x) { return x }\nlet l = [await load(x) for x in [1]]"), Vec::<(u32, u32)>::new());
    t.eq("await callback named", warns("let l = [1].map(f: fn(x) { return await x })"), vec![(1, 35)]);
    t.eq("await callback default", warns("let l = [1].map(fn(x, y = await x) { return y })"), vec![(1, 27)]);
    // a comprehension waits with its invocation
    let src = "let l = [await x for x in [1, 2]]\nprint(l)";
    eqs(t, "await comprehension runs", out(src), "[1, 2]\n");

}
