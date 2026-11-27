// scriptawaitt -- (S3) checks of await (script/task.rs, docs/HOST.md "Long operations"): the
// syntax (precedence, a keyword, tokens, the formatter, completion, the warning of check for an
// await in a callback of list.map ..; promise<T> in a host's description), and the VM with a
// host whose natives start long operations (later(tag): a promise the checks settle by its id):
// await at the top level, in functions and modules, of values that are no promise, of settled
// promises; rejections caught and uncaught with their position; awaits in a row and of a promise
// that gives a promise; several waiting invocations woken in the order their promises were
// settled; an event handler and other calls while an invocation waits; slices of size 1 against
// an uninterrupted run; a clone that goes on with another value; the collector under pressure
// while invocations wait; the debugger's step across an await; await in a native's callback
// (an error with its position); add_script; fuel over the wait; cancel_waiting; the invocation's
// own promise that another one awaits

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use script::api::Type;
use script::ls::{self, CompleteCx, Context, ItemKind};
use script::{Api, Cx, ErrorKind, Fail, Host, Limits, Object, PauseReason, PromiseState, Status, Step, TokClass, TokKind, Value, Vm};

use crate::T;

/// the host of the checks (see the module comment)
#[derive(Clone, Default)]
struct Ops {
    /// later(tag): (the promise's id, the tag) in the order they came
    jobs: Vec<(u64, String)>,
    log: Vec<String>,
    /// files for import
    files: Vec<(String, String)>,
}

impl Host for Ops {
    fn call(&mut self, cx: &mut Cx, name: &str, args: &[Value]) -> Result<Value, Fail> {
        let a = |i: usize| args.get(i).copied().unwrap_or(Value::Nil);
        match name {
            // a long operation: the checks settle it
            "later" => {
                let p = cx.promise()?;
                self.jobs.push((p.id, cx.display(a(0))));
                Ok(p.value)
            }
            // one that is done at once
            "ready" => {
                let p = cx.promise()?;
                cx.resolve(p.id, a(0));
                Ok(p.value)
            }
            "failed" => {
                let p = cx.promise()?;
                let m = cx.display(a(0));
                let v = cx.new_str(&m)?;
                cx.reject(p.id, v);
                Ok(p.value)
            }
            "log" => {
                let parts: Vec<String> = args.iter().map(|v| cx.display(*v)).collect();
                self.log.push(parts.join(" "));
                Ok(Value::Nil)
            }
            // a host function that calls its function back to its end
            "apply" => cx.call(self, a(0), &[a(1)]),
            _ => Err(Fail::Unknown),
        }
    }

    fn load(&mut self, _: &mut Cx, path: &str) -> Result<String, String> {
        self.files.iter().find(|f| f.0 == path).map(|f| f.1.clone()).ok_or_else(|| String::from("die Datei gibt es nicht"))
    }

    fn listen(&mut self, _: &mut Cx, obj: Object, _: &str, _: bool) -> Result<(), Fail> {
        if obj.ty == "button" { Ok(()) } else { Err(Fail::Unknown) }
    }
}

fn vm_of(src: &str, lim: Limits) -> Vm {
    match script::compile_named("main.jfs", src) {
        Ok(p) => Vm::new(&p, lim),
        Err(d) => {
            plat::write_stdout(format!("FAIL scriptawait: does not compile: {:?}\n", d).as_bytes());
            Vm::new(&script::compile("").expect("empty"), lim)
        }
    }
}

/// run the pending invocation until it ends or waits (yields and pauses go on)
fn go(vm: &mut Vm, h: &mut Ops, slice: u64) -> Status {
    loop {
        match vm.run(h, slice) {
            Status::Yielded | Status::Paused(_) => {}
            s => return s,
        }
    }
}

/// the id of the job with this tag
fn job(h: &Ops, tag: &str) -> u64 {
    h.jobs.iter().find(|j| j.1 == tag).map_or(0, |j| j.0)
}

/// a str made between runs
fn s(vm: &mut Vm, t: &str) -> Value {
    vm.new_str(t).expect("str")
}

pub fn run(t: &mut T) {
    syntax(t);
    tools(t);
    basics(t);
    rejections(t);
    order(t);
    meanwhile(t);
    slices(t);
    clone(t);
    collector(t);
    debugger(t);
    callbacks(t);
    scripts_and_modules(t);
    fuel_and_cancel(t);
    own_promise(t);
}

// ---------------------------------------------------------------- the syntax and the tools

fn sx(src: &str) -> String {
    match script::parse("", src) {
        Ok(p) => p.sexpr(),
        Err(d) => format!("ERR {}", d[0]),
    }
}

fn syntax(t: &mut T) {
    t.eq("await prefix", sx("await x"), String::from("(await x)"));
    t.eq("await binds tight", sx("await a + b"), String::from("(+ (await a) b)"));
    t.eq("await of a chain", sx("await f(x).y"), String::from("(await (. (call f x) y))"));
    t.eq("minus await", sx("-await x"), String::from("(- (await x))"));
    t.eq("await await", sx("await await p"), String::from("(await (await p))"));
    t.eq("await minus", sx("let v = await -x"), String::from("(let v (await (- x)))"));
    t.eq("await in a condition", sx("if await ok() { }"), String::from("(if (await (call ok)) (block))"));
    t.eq("await not", sx("!await p"), String::from("(! (await p))"));
    t.eq("await is a keyword", sx("let await = 1"), String::from("ERR 1:5: Fehler: await ist ein Schlüsselwort und kann kein Name sein"));
    t.eq("await without a value", sx("let v = await"), String::from("ERR 1:14: Fehler: Hier fehlt ein Ausdruck"));
    t.eq("await as a map key", sx("let m = {await: 1}"), String::from("(let m (map (\"await\" 1)))"));
    t.eq("await as a field", sx("m.await"), String::from("(. m await)"));
    t.eq("await on the next line", sx("let v = 1\nawait p"), String::from("(let v 1)\n(await p)"));
    let toks = script::tokens("await x");
    t.check("await token", toks.first().is_some_and(|x| *x == (0, 5, TokKind::Await)));
    t.eq("await is highlighted as a keyword", TokKind::Await.class(), TokClass::Keyword);
    t.eq("await keyword", TokKind::keyword("await"), Some(TokKind::Await));
    t.eq("outline", script::outline("fn f() { return await g() }\nlet x = await f()"), vec![(String::from("fn f()"), 1), (String::from("let x"), 2)]);
    t.eq("disasm", script::compile("let v = await 1").map(|p| p.disasm().contains("Await")), Ok(true));
}

fn tools(t: &mut T) {
    // the formatter keeps await, a blank after it, none after a unary minus
    t.eq("fmt", script::fmt::format("let a=await  f( 1 )\nprint(-await a, await -a)\n"), Ok(String::from("let a = await f(1)\nprint(-await a, await -a)\n")));
    t.eq("fmt idempotent", script::fmt::format("let a = await f(1)\n"), Ok(String::from("let a = await f(1)\n")));
    // completion: a keyword where a value may stand
    let src = "let x = aw";
    let c = ls::complete(src, src.len() as u32, &CompleteCx { host: &[], load: None });
    t.check("completion has await", c.items.iter().any(|i| i.label == "await" && i.kind == ItemKind::Keyword));
    // check: await in a callback of list.map, list.sort, list.filter ..
    let warns = |src: &str| -> Vec<String> {
        script::check(src).into_iter().filter(|d| !d.error && d.msg.contains("await")).map(|d| format!("{}:{}-{} {}", d.line, d.col, d.end_col, d.msg)).collect()
    };
    let w = warns("let xs = [1]\nxs.map(fn(x) { return await x })");
    t.eq("warn map", w.len(), 1);
    t.check("warn map text", w.first().is_some_and(|m| m.starts_with("2:23-28 await im Rückruf von map: der läuft ohne Unterbrechung zu Ende")));
    t.eq("warn sort", warns("let xs = [1]\nxs.sort(fn(a, b) {\n  return await a\n})").len(), 1);
    let w = warns("let xs = [1]\nlist.filter(xs, fn(x) { await x })");
    t.check("warn list.filter", w.first().is_some_and(|m| m.contains("von list.filter")));
    let w = warns("fn f(x) { return await x }\nlet xs = [1]\nprint(xs.map(f))");
    t.eq("warn a named fn", w, vec![String::from(
        "3:14-15 f wartet mit await, map ruft es aber ohne Unterbrechung auf: ein Wert, der noch aussteht, gibt dort einen Fehler – warte außerhalb, etwa in einer for-Schleife",
    )]);
    t.eq("no warn: listener", warns("let b = {}\nb.on(\"click\", fn(e) { await e })"), Vec::<String>::new());
    t.eq("no warn: loop", warns("for x in [1] { await x }"), Vec::<String>::new());
    t.eq("no warn: nested fn", warns("let xs = [1]\nxs.map(fn(x) { return fn() { await x } })"), Vec::<String>::new());
    t.eq("no warn: timer", warns("timer.after(1, fn() { await now() })"), Vec::<String>::new());
    t.eq("no warn: shadowed name", warns("fn f(x) { return await x }\nfn g(f) { return [1].map(f) }"), Vec::<String>::new());
    // a host's description: promise<T> and what await gives
    let api = Api::new("t")
        .func("fetch(url: str): promise<response>", "Lädt eine Seite.")
        .typ(Type::new("response", "Eine Antwort.").prop("readonly status: int", "Der Status."));
    t.eq("api valid", api.validate(), Vec::<String>::new());
    let cx = Context::new(&api);
    let src = "let r = await fetch(\"u\")\nr.";
    let c = ls::complete(src, src.len() as u32, &cx);
    t.check("await promise<T> gives T", c.items.iter().any(|i| i.label == "status"));
    let h = ls::hover(src, 5, &cx);
    t.eq("hover of an awaited let", h.map(|h| h.sig), Some(String::from("let r: response")));
    let src = "let p = fetch(\"u\")\np.";
    let c = ls::complete(src, src.len() as u32, &cx);
    t.check("a promise has no status", !c.items.iter().any(|i| i.label == "status"));
    let bad = Api::new("t").typ(Type::new("promise", "x"));
    t.check("promise is the language's", !bad.validate().is_empty());
    t.eq("api promise alone", Api::new("t").func("wait(): promise", "").validate(), Vec::<String>::new());
}

// ---------------------------------------------------------------- the VM

fn basics(t: &mut T) {
    // values that are no promise, settled promises: no wait
    let mut h = Ops::default();
    let mut vm = vm_of("log(await 1, await \"s\", await nil, await [1][0], await ready(5), type(ready(1)))", Limits::default());
    t.check("no wait: done", matches!(go(&mut vm, &mut h, u64::MAX), Status::Done(Value::Nil)));
    t.eq("no wait: log", h.log.clone(), vec![String::from("1 s nil 1 5 promise")]);
    t.eq("no wait: nothing waits", vm.waiting(), 0);
    // the top level waits, the VM is idle, the host settles, wake goes on
    let mut h = Ops::default();
    let mut vm = vm_of("log(\"a\")\nlet v = await later(\"x\")\nlog(\"b\", v)\nv", Limits::default());
    let st = go(&mut vm, &mut h, u64::MAX);
    let Status::Done(p) = st else {
        t.check("top level waits", false);
        return;
    };
    t.eq("waits: its promise", vm.promise_state(p), Some(PromiseState::Waiting));
    t.check("waits: is_waiting", vm.is_waiting(p) && !vm.is_waiting(Value::Int(1)));
    t.eq("waits: type", vm.object(p).map(|o| o.ty), Some("promise"));
    t.check("waits: idle", !vm.busy());
    t.eq("waits: one", (vm.waiting(), vm.ready()), (1, 0));
    t.eq("waits: log", h.log.clone(), vec![String::from("a")]);
    t.eq("wake: nothing ready", vm.wake(), None);
    t.check("run while it waits", matches!(vm.run(&mut h, 10), Status::Error(e) if e.kind == ErrorKind::Usage));
    let id = job(&h, "x");
    let v = s(&mut vm, "wert");
    t.check("resolve", vm.resolve(id, v));
    t.check("resolve twice", !vm.resolve(id, Value::Nil));
    t.check("resolve unknown", !vm.resolve(9999, Value::Nil));
    t.eq("ready", (vm.waiting(), vm.ready()), (1, 1));
    t.eq("wake gives the same promise", vm.wake(), Some(p));
    t.check("busy after wake", vm.busy() && vm.wake().is_none());
    t.check("goes on", matches!(go(&mut vm, &mut h, u64::MAX), Status::Done(Value::Nil)));
    t.eq("goes on: log", h.log.clone(), vec![String::from("a"), String::from("b wert")]);
    t.eq("its promise settles", vm.promise_state(p).map(|s| matches!(s, PromiseState::Resolved(Value::Nil))), Some(true));
    t.eq("nothing waits", (vm.waiting(), vm.ready()), (0, 0));
    t.check("its promise no longer waits", !vm.is_waiting(p));
    t.eq("global", vm.global("v").and_then(|v| vm.str(v).map(String::from)), Some(String::from("wert")));
    // a function that returns a promise of the host without await: Done with it, nothing waits
    let mut h = Ops::default();
    let mut vm = vm_of("fn f() { return later(\"r\") }", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    vm.call("f", &[]).expect("f");
    match go(&mut vm, &mut h, u64::MAX) {
        Status::Done(v) => t.check("returned promise", vm.promise_state(v) == Some(PromiseState::Waiting) && !vm.is_waiting(v) && vm.waiting() == 0),
        s => t.check(&format!("returned promise {:?}", s), false),
    }
    // functions inside functions keep their locals
    let mut h = Ops::default();
    let mut vm = vm_of("fn g(t) {\n  let a = [t]\n  let r = await later(t)\n  return a[0] + r\n}\nfn f() { return g(\"x\") + \"!\" }\nlog(f())", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    let (id, v) = (job(&h, "x"), s(&mut vm, "y"));
    vm.resolve(id, v);
    vm.wake();
    t.check("nested: done", matches!(go(&mut vm, &mut h, u64::MAX), Status::Done(_)));
    t.eq("nested: log", h.log.clone(), vec![String::from("xy!")]);
    // awaits in a row; a promise that gives a promise; the text of promises
    let mut h = Ops::default();
    let mut vm = vm_of("let p = later(\"outer\")\nlet q = await p\nlog(type(q), await q, p)\nlet n = 0\nfor i in 0..3 { n += await later(\"r${i}\") }\nlog(n)", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    let inner = vm.new_promise().expect("promise");
    t.eq("promise text", vm.display(inner.value), format!("<promise {} ausstehend>", inner.id));
    vm.resolve(job(&h, "outer"), inner.value);
    vm.wake();
    go(&mut vm, &mut h, u64::MAX);
    t.eq("outer gave a promise: still waiting", (vm.waiting(), h.log.len()), (1, 0));
    vm.resolve(inner.id, Value::Int(3));
    vm.wake();
    go(&mut vm, &mut h, u64::MAX);
    for i in 0..3 {
        let id = job(&h, &format!("r{}", i));
        vm.resolve(id, Value::Int(10 + i));
        t.check("in a loop: wakes", vm.wake().is_some());
        go(&mut vm, &mut h, u64::MAX);
    }
    t.eq("in a row: log", h.log.clone(), vec![format!("promise 3 <promise {} erfüllt: <promise {} erfüllt: 3>>", job(&h, "outer"), inner.id), String::from("33")]);
}

fn rejections(t: &mut T) {
    // caught: the reason is what catch gets
    let mut h = Ops::default();
    let mut vm = vm_of("try {\n  await later(\"x\")\n  log(\"not here\")\n} catch e {\n  log(\"caught\", e)\n}\ntry { await failed(\"gleich\") } catch e { log(\"caught\", e) }", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    t.check("reject_msg", vm.reject_msg(job(&h, "x"), "kaputt"));
    t.check("reject twice", !vm.reject_msg(job(&h, "x"), "noch mal"));
    vm.wake();
    t.check("caught: done", matches!(go(&mut vm, &mut h, u64::MAX), Status::Done(_)));
    t.eq("caught: log", h.log.clone(), vec![String::from("caught kaputt"), String::from("caught gleich")]);
    // a map as the reason
    let mut h = Ops::default();
    let mut vm = vm_of("try { await later(\"x\") } catch e { log(e.code) }", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    let m = vm.new_map().expect("map");
    let k = s(&mut vm, "code");
    vm.map_set(m, k, Value::Int(404)).expect("set");
    vm.reject(job(&h, "x"), m);
    vm.wake();
    go(&mut vm, &mut h, u64::MAX);
    t.eq("reason map", h.log.clone(), vec![String::from("404")]);
    // uncaught: the error is at the await, with the calls
    let mut h = Ops::default();
    let mut vm = vm_of("fn f() {\n  let x = 1\n  return await later(\"x\")\n}\nlog(f())", Limits::default());
    let Status::Done(p) = go(&mut vm, &mut h, u64::MAX) else { return t.check("uncaught: waits", false) };
    vm.reject_msg(job(&h, "x"), "Datei fehlt");
    vm.wake();
    match go(&mut vm, &mut h, u64::MAX) {
        Status::Error(e) => {
            t.eq("uncaught: where", (e.kind, e.line, e.col, e.msg.as_str()), (ErrorKind::Uncaught, 3, 10, "Datei fehlt"));
            t.eq("uncaught: trace", e.trace.iter().map(|x| (x.func.clone(), x.line)).collect::<Vec<_>>(), vec![(String::from("f"), 3), (String::new(), 5)]);
        }
        s => t.check(&format!("uncaught: {:?}", s), false),
    }
    t.eq("uncaught: its promise", vm.promise_state(p).and_then(|s| match s {
        PromiseState::Rejected(v) => vm.str(v).map(String::from),
        _ => None,
    }), Some(String::from("Datei fehlt")));
    t.check("uncaught: the VM goes on", vm.call("f", &[]).is_ok());
}

fn order(t: &mut T) {
    // several invocations wait; they go on in the order their promises were settled
    let src = "fn job(t) {\n  log(\"start \" + t)\n  let v = await later(t)\n  log(\"end \" + t + \" \" + str(v))\n  return v\n}";
    let mut h = Ops::default();
    let mut vm = vm_of(src, Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    let mut ps = Vec::new();
    for tag in ["a", "b", "c"] {
        let a = s(&mut vm, tag);
        vm.call("job", &[a]).expect("call");
        match go(&mut vm, &mut h, u64::MAX) {
            Status::Done(p) => ps.push(p),
            s => t.check(&format!("order: waits {:?}", s), false),
        }
    }
    t.eq("order: three wait", (vm.waiting(), vm.ready()), (3, 0));
    t.check("order: three promises", ps.len() == 3 && ps[0] != ps[1] && ps[1] != ps[2]);
    for (tag, v) in [("c", 3), ("a", 1)] {
        vm.resolve(job(&h, tag), Value::Int(v));
    }
    t.eq("order: two ready", vm.ready(), 2);
    let mut woke = Vec::new();
    while let Some(p) = vm.wake() {
        woke.push(ps.iter().position(|x| *x == p).unwrap_or(9));
        match go(&mut vm, &mut h, u64::MAX) {
            Status::Done(v) => t.eq("order: the result", vm.promise_state(p), Some(PromiseState::Resolved(v))),
            s => t.check(&format!("order: done {:?}", s), false),
        }
        if woke.len() == 1 {
            // settled while another runs: after the ones before
            vm.resolve(job(&h, "b"), Value::Int(2));
        }
    }
    t.eq("order: woken", woke, vec![2, 0, 1]);
    t.eq("order: log", h.log[3..].to_vec(), vec![String::from("end c 3"), String::from("end a 1"), String::from("end b 2")]);
    // two invocations wait for the same promise: in the order they began to wait
    let mut h = Ops::default();
    let mut vm = vm_of("let shared = later(\"s\")\nfn w(n) { log(n, await shared) }", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    for n in [1, 2] {
        vm.call("w", &[Value::Int(n)]).expect("call");
        go(&mut vm, &mut h, u64::MAX);
    }
    vm.resolve(job(&h, "s"), Value::Int(7));
    t.eq("shared: both ready", vm.ready(), 2);
    while vm.wake().is_some() {
        go(&mut vm, &mut h, u64::MAX);
    }
    t.eq("shared: log", h.log.clone(), vec![String::from("1 7"), String::from("2 7")]);
}

fn meanwhile(t: &mut T) {
    // a macro waits for a dialog; clicks and other calls run meanwhile
    let src = "let clicks = 0\n\
        btn.on(\"click\", fn(e) { clicks += 1; log(\"click \" + str(clicks)) })\n\
        btn.on(\"press\", fn(e) { log(\"press \" + await later(\"press\")) })\n\
        fn macro() {\n  log(\"macro start\")\n  let f = await later(\"dialog\")\n  log(\"macro got \" + f + \" after \" + str(clicks) + \" clicks\")\n}\n\
        fn count() { return clicks }";
    let mut h = Ops::default();
    let mut vm = vm_of(src, Limits::default());
    let b = vm.new_object("button", 1).expect("button");
    vm.set_global("btn", b);
    t.check("meanwhile: top level", matches!(go(&mut vm, &mut h, u64::MAX), Status::Done(_)));
    vm.call("macro", &[]).expect("macro");
    t.check("meanwhile: macro waits", matches!(go(&mut vm, &mut h, u64::MAX), Status::Done(_)) && vm.waiting() == 1);
    for _ in 0..2 {
        let r = vm.emit(&mut h, b, "click", &[]).expect("emit");
        t.check("meanwhile: click", r.len() == 1 && r[0].error.is_none());
    }
    vm.call("count", &[]).expect("count");
    t.eq("meanwhile: another call", go(&mut vm, &mut h, u64::MAX), Status::Done(Value::Int(2)));
    // a listener that waits: emit goes on, the listener later
    let r = vm.emit(&mut h, b, "press", &[]).expect("emit");
    t.check("meanwhile: press waits", r.len() == 1 && r[0].error.is_none() && vm.waiting() == 2);
    let f = s(&mut vm, "plan.vsdx");
    vm.resolve(job(&h, "dialog"), f);
    let k = s(&mut vm, "kurz");
    vm.resolve(job(&h, "press"), k);
    while vm.wake().is_some() {
        go(&mut vm, &mut h, u64::MAX);
    }
    t.eq(
        "meanwhile: log",
        h.log.clone(),
        vec![
            String::from("macro start"),
            String::from("click 1"),
            String::from("click 2"),
            String::from("macro got plan.vsdx after 2 clicks"),
            String::from("press kurz"),
        ],
    );
}

/// the program of slices() with the host settling every job as soon as nothing runs: tags with
/// "x" reject, the others resolve with the tag and "!" -> (the log, the fuel of every end or
/// wait, the result of each end)
fn settle_all(src: &str, slice: u64) -> (Vec<String>, Vec<u64>, Vec<String>) {
    let mut h = Ops::default();
    let mut vm = vm_of(src, Limits::default());
    let (mut fuel, mut ends) = (Vec::new(), Vec::new());
    let mut done = 0;
    loop {
        match go(&mut vm, &mut h, slice) {
            Status::Done(v) => ends.push(vm.display(v)),
            Status::Error(e) => ends.push(format!("ERR {}", e)),
            _ => {}
        }
        fuel.push(vm.call_fuel());
        if vm.wake().is_some() {
            continue;
        }
        let Some((id, tag)) = h.jobs.get(done).cloned() else { break };
        done += 1;
        if tag.contains('x') {
            vm.reject_msg(id, &format!("nein {}", tag));
        } else {
            let v = s(&mut vm, &format!("{}!", tag));
            vm.resolve(id, v);
        }
        if vm.wake().is_none() {
            break;
        }
    }
    (h.log, fuel, ends)
}

fn slices(t: &mut T) {
    let src = "fn step(t) {\n  let parts = []\n  for i in 0..3 {\n    parts.push(await later(t + str(i)))\n  }\n  return \", \".join(parts)\n}\n\
        log(step(\"a\"))\n\
        try { log(await later(\"x1\")) } catch e { log(\"caught\", e) }\n\
        let m = {k: await ready(1), l: await later(\"b\")}\n\
        log(m, await 2 + await ready(3))\n\
        let s = 0\nfor i in 0..50 { s += i * 2 }\nlog(s)\n\
        log(await later(\"x2\"))";
    let (log, fuel, ends) = settle_all(src, u64::MAX);
    t.eq(
        "slices: log",
        log.clone(),
        vec![String::from("a0!, a1!, a2!"), String::from("caught nein x1"), String::from("{k: 1, l: \"b!\"} 5"), String::from("2450")],
    );
    t.check("slices: ends in an error", ends.last().is_some_and(|e| e.starts_with("ERR 15:5: Fehler: nein x2")));
    for slice in [1, 2, 3, 7] {
        let other = settle_all(src, slice);
        t.eq(&format!("slices of {}", slice), other, (log.clone(), fuel.clone(), ends.clone()));
    }
}

fn clone(t: &mut T) {
    let mut h = Ops::default();
    let mut vm = vm_of("let a = [1, 2]\nlet v = await later(\"x\")\nlog(\"got\", v, a)", Limits::default());
    let Status::Done(p) = go(&mut vm, &mut h, u64::MAX) else { return t.check("clone: waits", false) };
    let mut c = vm.clone();
    let mut hc = h.clone();
    let id = job(&h, "x");
    t.eq("clone: the same promise", (c.waiting(), c.object(p).map(|o| o.id)), (1, vm.object(p).map(|o| o.id)));
    vm.resolve(id, Value::Int(1));
    t.eq("clone: its own", (vm.ready(), c.ready()), (1, 0));
    c.resolve(id, Value::Int(2));
    vm.wake();
    go(&mut vm, &mut h, u64::MAX);
    c.wake();
    go(&mut c, &mut hc, u64::MAX);
    t.eq("clone: original", h.log.clone(), vec![String::from("got 1 [1, 2]")]);
    t.eq("clone: clone", hc.log.clone(), vec![String::from("got 2 [1, 2]")]);
    t.eq("clone: same fuel", vm.call_fuel(), c.call_fuel());
}

fn collector(t: &mut T) {
    let src = "fn hold(n) {\n  let keep = []\n  for i in 0..n { keep.push(\"k\" + str(i)) }\n  let m = {list: keep}\n  let v = await later(\"h\" + str(n))\n  let total = 0\n  for x in m.list { total += len(x) }\n  return total + v\n}\n\
        fn churn() {\n  let s = 0\n  for i in 0..20000 { let t = \"x\" + str(i) + \"y\"; s += len(t) }\n  return s\n}\n\
        later(\"lost\")";
    let mut h = Ops::default();
    let mut vm = vm_of(src, Limits { heap: 4 << 20, ..Limits::default() });
    go(&mut vm, &mut h, u64::MAX);
    let mut ps = Vec::new();
    for n in [100, 200] {
        vm.call("hold", &[Value::Int(n)]).expect("hold");
        if let Status::Done(p) = go(&mut vm, &mut h, u64::MAX) {
            ps.push(p);
        }
    }
    let before = vm.collections();
    for _ in 0..3 {
        vm.call("churn", &[]).expect("churn");
        go(&mut vm, &mut h, u64::MAX);
    }
    vm.collect();
    t.check("collector: collected", vm.collections() > before + 1);
    // the promise nobody holds stays until it is settled
    t.check("collector: lost one settles", vm.resolve(job(&h, "lost"), Value::Int(0)));
    for n in [200, 100] {
        vm.resolve(job(&h, &format!("h{}", n)), Value::Int(1000));
        vm.collect();
    }
    let mut res = Vec::new();
    while let Some(p) = vm.wake() {
        if let Status::Done(Value::Int(x)) = go(&mut vm, &mut h, u64::MAX) {
            res.push((ps.iter().position(|q| *q == p).unwrap_or(9), x));
        }
        vm.collect();
    }
    // "k0".."k99": 10 * 2 + 90 * 3; "k0".."k199": that + 100 * 4
    t.eq("collector: what they held", res, vec![(1, 290 + 400 + 1000), (0, 290 + 1000)]);
}

fn debugger(t: &mut T) {
    let src = "fn f() {\n  let a = 1\n  let b = await later(\"x\")\n  let c = a + b\n  return c\n}\nfn g() {\n  let z = 2\n  return z\n}\nlog(f())";
    let mut h = Ops::default();
    let mut vm = vm_of(src, Limits::default());
    t.check("debugger: breakpoint", vm.set_breakpoint("main.jfs", 3, None).is_ok());
    match vm.run(&mut h, u64::MAX) {
        Status::Paused(p) => t.eq("debugger: at the await's line", (p.line, p.reason), (3, PauseReason::Breakpoint)),
        s => t.check(&format!("debugger: paused {:?}", s), false),
    }
    vm.step(Step::Over);
    t.check("debugger: waits", matches!(vm.run(&mut h, u64::MAX), Status::Done(_)) && vm.waiting() == 1);
    // another invocation meanwhile: the step is not its
    vm.call("g", &[]).expect("g");
    t.eq("debugger: g runs through", vm.run(&mut h, u64::MAX), Status::Done(Value::Int(2)));
    vm.resolve(job(&h, "x"), Value::Int(5));
    vm.wake();
    match vm.run(&mut h, u64::MAX) {
        Status::Paused(p) => t.eq("debugger: the step goes on", (p.line, p.reason), (4, PauseReason::Step)),
        s => t.check(&format!("debugger: step {:?}", s), false),
    }
    t.eq("debugger: frames", vm.frames().iter().map(|f| (f.func.clone(), f.line)).collect::<Vec<_>>(), vec![(String::from("f"), 4), (String::new(), 11)]);
    t.eq("debugger: locals", vm.locals(0), vec![(String::from("a"), Value::Int(1)), (String::from("b"), Value::Int(5))]);
    t.eq("debugger: eval", vm.eval(0, "a + b"), Ok(Value::Int(6)));
    t.eq("debugger: eval await", vm.eval(0, "await b"), Ok(Value::Int(5)));
    vm.step(Step::Over);
    match vm.run(&mut h, u64::MAX) {
        Status::Paused(p) => t.eq("debugger: next", p.line, 5),
        s => t.check(&format!("debugger: next {:?}", s), false),
    }
    t.check("debugger: to the end", matches!(vm.run(&mut h, u64::MAX), Status::Done(_)));
    t.eq("debugger: log", h.log.clone(), vec![String::from("6")]);
    // eval of a promise that is not settled does not wait
    let mut h = Ops::default();
    let mut vm = vm_of("let p = later(\"y\")\nlet q = 1\nlog(q)", Limits::default());
    vm.set_breakpoint("main.jfs", 3, None).expect("bp");
    vm.run(&mut h, u64::MAX);
    t.eq("debugger: eval waiting", vm.eval(0, "await p"), Err(String::from("Der Wert steht noch aus – im Ausdruck wartet await nicht")));
}

fn callbacks(t: &mut T) {
    // a callback of a native cannot wait: a catchable error with its position
    let mut h = Ops::default();
    let src = "try {\n  apply(fn(x) { return await later(x) }, \"t\")\n} catch e {\n  log(e)\n}\n\
        log([1, 2].map(fn(x) { return await ready(x * 10) }))\n\
        log(apply(fn(x) { return await x }, 3))\n\
        [1].map(fn(x) {\n  return await later(\"m\")\n})";
    let mut vm = vm_of(src, Limits::default());
    match go(&mut vm, &mut h, u64::MAX) {
        Status::Error(e) => {
            t.check("callback: message", e.msg.starts_with("await kann in einem Rückruf, den eine Funktion aufruft"));
            t.eq("callback: where", (e.line, e.col), (9, 10));
            t.eq("callback: trace", e.trace.iter().map(|x| x.func.clone()).collect::<Vec<_>>(), vec![String::from("fn"), String::new()]);
        }
        s => t.check(&format!("callback: error {:?}", s), false),
    }
    t.eq("callback: log", h.log.len(), 3);
    t.check("callback: caught", h.log.first().is_some_and(|l| l.starts_with("await kann in einem Rückruf")));
    t.eq("callback: settled ones work", h.log[1..].to_vec(), vec![String::from("[10, 20]"), String::from("3")]);
    t.eq("callback: nothing waits", vm.waiting(), 0);
}

fn scripts_and_modules(t: &mut T) {
    // add_script: a script's top-level code waits, the next one runs
    let mut h = Ops::default();
    let mut vm = Vm::new(&script::compile_named("page", "").expect("stub"), Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    t.check("scripts: a", vm.add_script("a.jfs", "let x = await later(\"a\")\nlog(\"a \" + str(x))").is_ok());
    t.check("scripts: a waits", matches!(go(&mut vm, &mut h, u64::MAX), Status::Done(_)) && vm.waiting() == 1);
    t.check("scripts: b", vm.add_script("b.jfs", "log(\"b \" + str(x))").is_ok());
    go(&mut vm, &mut h, u64::MAX);
    vm.resolve(job(&h, "a"), Value::Int(1));
    vm.wake();
    go(&mut vm, &mut h, u64::MAX);
    t.eq("scripts: log", h.log.clone(), vec![String::from("b nil"), String::from("a 1")]);
    // a module's top-level code waits with the invocation that imports it
    let mut h = Ops { files: vec![(String::from("m.jfs"), String::from("let v = await later(\"m\")\nfn get() { return v }"))], ..Ops::default() };
    let mut vm = vm_of("fn imp() { return import(\"m.jfs\").get() }\nlet m = import(\"m.jfs\")\nlog(m.get())", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    t.eq("module: waits", vm.waiting(), 1);
    // another invocation cannot import it meanwhile
    vm.call("imp", &[]).expect("imp");
    match go(&mut vm, &mut h, u64::MAX) {
        Status::Error(e) => t.eq("module: still waiting", e.msg, String::from("Modul m.jfs: sein Code wartet noch in await – import geht erst, wenn er fertig ist")),
        s => t.check(&format!("module: {:?}", s), false),
    }
    vm.resolve(job(&h, "m"), Value::Int(42));
    vm.wake();
    go(&mut vm, &mut h, u64::MAX);
    vm.call("imp", &[]).expect("imp");
    t.eq("module: then", go(&mut vm, &mut h, u64::MAX), Status::Done(Value::Int(42)));
    t.eq("module: log", h.log.clone(), vec![String::from("42")]);
}

fn fuel_and_cancel(t: &mut T) {
    // the fuel of an invocation counts on after its wait
    let mut h = Ops::default();
    let mut vm = vm_of("let s = 0\nfor i in 0..20 { s += i }\nawait later(\"x\")\nfor i in 0..200 { s += i }", Limits { fuel: 500, ..Limits::default() });
    t.check("fuel: waits", matches!(go(&mut vm, &mut h, u64::MAX), Status::Done(_)));
    let before = vm.call_fuel();
    t.check("fuel: used some", before > 50 && before < 500);
    vm.resolve(job(&h, "x"), Value::Nil);
    vm.wake();
    t.eq("fuel: after the wake", vm.call_fuel(), before);
    match go(&mut vm, &mut h, u64::MAX) {
        Status::Error(e) => t.eq("fuel: the limit holds over the wait", e.kind, ErrorKind::Fuel),
        s => t.check(&format!("fuel: {:?}", s), false),
    }
    // cancel_waiting: the invocation is gone, its promise rejected
    let mut h = Ops::default();
    let mut vm = vm_of("fn w() { await later(\"w\"); log(\"never\") }", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    vm.call("w", &[]).expect("w");
    let Status::Done(p) = go(&mut vm, &mut h, u64::MAX) else { return t.check("cancel: waits", false) };
    t.check("cancel_waiting", vm.cancel_waiting(p));
    t.check("cancel_waiting twice", !vm.cancel_waiting(p));
    t.eq("cancel: nothing waits", vm.waiting(), 0);
    t.eq("cancel: its promise", vm.promise_state(p).map(|s| matches!(s, PromiseState::Rejected(_))), Some(true));
    t.check("cancel: the job settles", vm.resolve(job(&h, "w"), Value::Nil));
    t.eq("cancel: nobody to wake", (vm.ready(), vm.wake()), (0, None));
    t.eq("cancel: log", h.log.len(), 0);
    // cancel() of a woken invocation rejects its promise
    vm.call("w", &[]).expect("w");
    let Status::Done(p) = go(&mut vm, &mut h, u64::MAX) else { return t.check("cancel: waits 2", false) };
    let id = h.jobs.last().map_or(0, |j| j.0);
    vm.resolve(id, Value::Nil);
    vm.wake();
    vm.cancel();
    t.eq("cancel: woken one", vm.promise_state(p).map(|s| matches!(s, PromiseState::Rejected(_))), Some(true));
}

fn own_promise(t: &mut T) {
    // the promise run() gives: another invocation awaits it
    let mut h = Ops::default();
    let mut vm = vm_of("fn work(t) { let v = await later(t); if v == 0 { throw \"null\" } return v * 2 }\nfn waiter(p) {\n  try { log(\"waited\", await p) } catch e { log(\"failed\", e) }\n}", Limits::default());
    go(&mut vm, &mut h, u64::MAX);
    for tag in ["a", "b"] {
        let a = s(&mut vm, tag);
        vm.call("work", &[a]).expect("work");
        let Status::Done(p) = go(&mut vm, &mut h, u64::MAX) else { return t.check("own: waits", false) };
        let keep = vm.pin(p);
        vm.call("waiter", &[p]).expect("waiter");
        go(&mut vm, &mut h, u64::MAX);
        vm.unpin(keep);
    }
    t.eq("own: four wait", vm.waiting(), 4);
    vm.resolve(job(&h, "a"), Value::Int(21));
    vm.resolve(job(&h, "b"), Value::Int(0));
    while vm.wake().is_some() {
        go(&mut vm, &mut h, u64::MAX);
    }
    t.eq("own: log", h.log.clone(), vec![String::from("waited 42"), String::from("failed null")]);
    t.eq("own: nothing waits", vm.waiting(), 0);
}
