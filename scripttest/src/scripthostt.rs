// scripthostt -- (S2) checks of the host interface in its second version (docs/HOST.md) with a
// host like modeltool's and JFDiagrammer's: a value type vec (x, y, z read-only; + - * / with
// scalars, unary -, == by value, < by length), meshes that methods make and the collector drops
// (counted), elements with a settable text and events (click bubbles up to the root, input stays
// at its element, as this host decides; e.stop() ends the way up), an event object of the host;
// errors of getters, setters and operators with their positions, fuel and limits in hooks and
// listeners, yields in a hook, slices and clones; and the same host's description (script::Api)
// in the language services (completion after a dot, hover, signature help, diagnostics of
// unknown members) and the object catalogue.  (S3) The gaps the first S2 host found: forms
// (overloads), optional parameters and unions in signatures, binding a call (also by name),
// counting arguments (check_calls), compile_api with the warnings of the same parse (also of
// modules and add_script), the host's type names in messages, closer suggestions, removed names

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use script::api::{self, Arg, EntryKind, Mismatch, Type};
use script::ls::{self, CompleteCx, Context, HostItem, ItemKind};
use script::{num, Api, Cx, ErrorKind, Fail, Host, Limits, Object, Operator, Status, Value, Vm, Warning};

use crate::T;

struct Mesh {
    faces: i64,
    parent: Option<u64>,
}

struct Elem {
    name: String,
    text: String,
    parent: Option<u64>,
}

struct Ev {
    ty: String,
    target: u64,
    current: u64,
    stopped: bool,
}

/// the host of the checks (see the module comment)
struct TestHost {
    meshes: Vec<Option<Mesh>>,
    /// the most meshes alive at once
    peak: usize,
    elems: Vec<Elem>,
    events: Vec<Ev>,
    /// what Host::drop reported: (type, id)
    dropped: Vec<(String, u64)>,
    /// Host::listen: "on click", "off input"
    listens: Vec<String>,
    log: Vec<String>,
    /// (S3) the modules Host::load gives: (path, source)
    files: Vec<(String, String)>,
}

impl TestHost {
    fn new() -> TestHost {
        let e = |name: &str, parent: Option<u64>| Elem { name: String::from(name), text: String::new(), parent };
        TestHost {
            meshes: Vec::new(),
            peak: 0,
            elems: vec![e("root", None), e("panel", Some(0)), e("btn", Some(1)), e("field", Some(1))],
            events: Vec::new(),
            dropped: Vec::new(),
            listens: Vec::new(),
            log: Vec::new(),
            files: Vec::new(),
        }
    }

    fn mesh(&mut self, cx: &mut Cx, faces: i64, parent: Option<u64>) -> Result<Value, Fail> {
        let id = match self.meshes.iter().position(|m| m.is_none()) {
            Some(i) => {
                self.meshes[i] = Some(Mesh { faces, parent });
                i
            }
            None => {
                self.meshes.push(Some(Mesh { faces, parent }));
                self.meshes.len() - 1
            }
        };
        self.peak = self.peak.max(self.meshes.iter().filter(|m| m.is_some()).count());
        cx.new_object("mesh", id as u64)
    }

    fn vec3(cx: &mut Cx, x: f64, y: f64, z: f64) -> Result<Value, Fail> {
        cx.new_value("vec", &[Value::Float(x), Value::Float(y), Value::Float(z)])
    }

    /// the three numbers of a vec
    fn xyz(cx: &Cx, v: Value) -> Option<[f64; 3]> {
        let d = cx.data(v)?;
        Some([d[0].as_float()?, d[1].as_float()?, d[2].as_float()?])
    }

    fn elem_named(&self, name: &str) -> Option<u64> {
        self.elems.iter().position(|e| e.name == name).map(|i| i as u64)
    }

    /// the way of an event up from target: itself, its parents (click), or itself alone
    fn path(&self, target: u64, ty: &str) -> Vec<u64> {
        let mut p = vec![target];
        if ty == "click" {
            let mut at = self.elems[target as usize].parent;
            while let Some(x) = at {
                p.push(x);
                at = self.elems[x as usize].parent;
            }
        }
        p
    }
}

fn len3(a: [f64; 3]) -> f64 {
    num::sqrt(num::add(num::add(num::mul(a[0], a[0]), num::mul(a[1], a[1])), num::mul(a[2], a[2])))
}

impl Host for TestHost {
    fn load(&mut self, _cx: &mut Cx, path: &str) -> Result<String, String> {
        self.files.iter().find(|f| f.0 == path).map(|f| f.1.clone()).ok_or_else(|| String::from("die Datei gibt es nicht"))
    }

    fn call(&mut self, cx: &mut Cx, name: &str, args: &[Value]) -> Result<Value, Fail> {
        let arg = |i: usize| args.get(i).copied().unwrap_or(Value::Nil);
        match name {
            "vec" => {
                let f = |v: Value| v.as_float().unwrap_or(0.0);
                Self::vec3(cx, f(arg(0)), f(arg(1)), f(arg(2)))
            }
            "cube" => self.mesh(cx, 6, None),
            "elem" => {
                let n = cx.str(arg(0)).map(String::from).unwrap_or_default();
                match self.elem_named(&n) {
                    Some(id) => cx.new_object("element", id),
                    None => Err(cx.error(&format!("elem: kein Element {}", n))),
                }
            }
            "log" => {
                let s = args.iter().map(|a| cx.display_with(*a, self)).collect::<Vec<_>>().join(" ");
                self.log.push(s);
                Ok(Value::Nil)
            }
            _ => Err(Fail::Unknown),
        }
    }

    fn get(&mut self, cx: &mut Cx, obj: Object, name: &str) -> Result<Value, Fail> {
        match (obj.ty, name) {
            ("vec", "x" | "y" | "z") => {
                let i = match name {
                    "x" => 0,
                    "y" => 1,
                    _ => 2,
                };
                Ok(cx.data(obj.value).map_or(Value::Nil, |d| d[i]))
            }
            ("mesh", "faces") => Ok(Value::Int(self.meshes[obj.id as usize].as_ref().map_or(-1, |m| m.faces))),
            ("mesh", "parent") => match self.meshes[obj.id as usize].as_ref().and_then(|m| m.parent) {
                Some(p) => cx.new_object("mesh", p),
                None => Ok(Value::Nil),
            },
            ("mesh", "boom") => Err(cx.error("kaputt")),
            ("mesh", "slow") => {
                cx.spend(5_000_000)?;
                Ok(Value::Int(1))
            }
            ("mesh", "pause") => {
                cx.yield_now();
                Ok(Value::Int(7))
            }
            ("mesh", "stale") => Ok(Value::Native(99_999)),
            ("element", "text") => cx.new_str(&self.elems[obj.id as usize].text.clone()),
            ("element", "name") => cx.new_str(&self.elems[obj.id as usize].name.clone()),
            ("element", "parent") => match self.elems[obj.id as usize].parent {
                Some(p) => cx.new_object("element", p),
                None => Ok(Value::Nil),
            },
            ("element", "children") => {
                let kids: Vec<u64> = (0..self.elems.len() as u64).filter(|&i| self.elems[i as usize].parent == Some(obj.id)).collect();
                let l = cx.new_list(Vec::new())?;
                for k in kids {
                    let e = cx.new_object("element", k)?;
                    cx.list_push(l, e)?;
                }
                Ok(l)
            }
            ("event", "type") => cx.new_str(&self.events[obj.id as usize].ty.clone()),
            ("event", "target") => cx.new_object("element", self.events[obj.id as usize].target),
            ("event", "current") => cx.new_object("element", self.events[obj.id as usize].current),
            _ => Err(Fail::Unknown),
        }
    }

    fn set(&mut self, cx: &mut Cx, obj: Object, name: &str, value: Value) -> Result<(), Fail> {
        match (obj.ty, name) {
            ("element", "text") => match cx.str(value) {
                Some(s) => {
                    self.elems[obj.id as usize].text = String::from(s);
                    Ok(())
                }
                None => Err(cx.error(&format!("element.text muss str sein, nicht {}", cx.type_of(value)))),
            },
            ("element", "name") | ("mesh", "faces") => Err(cx.error(&format!("{}.{} ist nur lesbar", obj.ty, name))),
            ("vec", _) => Err(cx.error("vec lässt sich nicht ändern")),
            _ => Err(Fail::Unknown),
        }
    }

    fn method(&mut self, cx: &mut Cx, obj: Object, name: &str, args: &[Value]) -> Result<Value, Fail> {
        let arg = |i: usize| args.get(i).copied().unwrap_or(Value::Nil);
        match (obj.ty, name) {
            ("mesh", "bevel") => {
                let segs = match arg(1) {
                    Value::Int(n) => n,
                    _ => 1,
                };
                let faces = self.meshes[obj.id as usize].as_ref().map_or(0, |m| m.faces);
                self.mesh(cx, faces + 12 * segs, Some(obj.id))
            }
            ("mesh", "same") => cx.new_object("mesh", obj.id),
            ("vec", "length") => match Self::xyz(cx, obj.value) {
                Some(a) => Ok(Value::Float(len3(a))),
                None => Ok(Value::Nil),
            },
            ("vec", "dot") => match (Self::xyz(cx, obj.value), Self::xyz(cx, arg(0))) {
                (Some(a), Some(b)) => Ok(Value::Float(num::add(num::add(num::mul(a[0], b[0]), num::mul(a[1], b[1])), num::mul(a[2], b[2])))),
                _ => Err(cx.error("vec.dot braucht einen vec")),
            },
            ("element", "click") => {
                // a click from the script: the listeners now, inside this call, up the way
                let path = self.path(obj.id, "click");
                let ev = self.events.len() as u64;
                self.events.push(Ev { ty: String::from("click"), target: obj.id, current: obj.id, stopped: false });
                let e = cx.new_object("event", ev)?;
                for at in path {
                    self.events[ev as usize].current = at;
                    let el = cx.new_object("element", at)?;
                    cx.keep(el);
                    cx.emit(self, el, "click", &[e])?;
                    if self.events[ev as usize].stopped {
                        break;
                    }
                }
                Ok(Value::Nil)
            }
            ("element", "remove") => {
                // the host deletes the element: its listeners go
                let n = cx.remove_listeners(obj.value);
                Ok(Value::Int(n as i64))
            }
            ("event", "stop") => {
                self.events[obj.id as usize].stopped = true;
                Ok(Value::Nil)
            }
            _ => Err(Fail::Unknown),
        }
    }

    fn op(&mut self, cx: &mut Cx, op: Operator, a: Value, b: Value) -> Result<Value, Fail> {
        let (va, vb) = (Self::xyz(cx, a), Self::xyz(cx, b));
        let (sa, sb) = (a.as_float(), b.as_float());
        let map = |p: [f64; 3], f: &dyn Fn(f64) -> f64| [f(p[0]), f(p[1]), f(p[2])];
        let r = match (op, va, vb) {
            (Operator::Add, Some(p), Some(q)) => [num::add(p[0], q[0]), num::add(p[1], q[1]), num::add(p[2], q[2])],
            (Operator::Sub, Some(p), Some(q)) => [num::sub(p[0], q[0]), num::sub(p[1], q[1]), num::sub(p[2], q[2])],
            (Operator::Mul, Some(p), None) if sb.is_some() => map(p, &|x| num::mul(x, sb.unwrap_or(0.0))),
            (Operator::Mul, None, Some(q)) if sa.is_some() => map(q, &|x| num::mul(sa.unwrap_or(0.0), x)),
            (Operator::Div, Some(p), None) if sb.is_some() => map(p, &|x| num::div(x, sb.unwrap_or(0.0))),
            (Operator::Neg, Some(p), None) => map(p, &num::neg),
            (Operator::Lt | Operator::Le | Operator::Gt | Operator::Ge, Some(p), Some(q)) => {
                let (x, y) = (len3(p), len3(q));
                let c = num::cmp(x, y);
                let r = match op {
                    Operator::Lt => c == Some(core::cmp::Ordering::Less),
                    Operator::Le => c.is_some_and(|o| o != core::cmp::Ordering::Greater),
                    Operator::Gt => c == Some(core::cmp::Ordering::Greater),
                    _ => c.is_some_and(|o| o != core::cmp::Ordering::Less),
                };
                return Ok(Value::Bool(r));
            }
            _ => return Err(Fail::Unknown),
        };
        Self::vec3(cx, r[0], r[1], r[2])
    }

    fn listen(&mut self, cx: &mut Cx, obj: Object, event: &str, on: bool) -> Result<(), Fail> {
        match (obj.ty, event) {
            ("element", "click" | "input") => {
                self.listens.push(format!("{} {}", if on { "on" } else { "off" }, event));
                Ok(())
            }
            ("element", "full") => Err(cx.error("zu viele Ereignis-Funktionen")),
            _ => Err(Fail::Unknown),
        }
    }

    fn to_str(&self, vm: &Vm, obj: Object) -> Option<String> {
        match obj.ty {
            "mesh" => Some(format!("mesh#{}({} Flächen)", obj.id, self.meshes[obj.id as usize].as_ref().map_or(0, |m| m.faces))),
            "element" => Some(format!("<{}>", self.elems[obj.id as usize].name)),
            // a value type: the default text, from its data
            _ => {
                let _ = vm;
                None
            }
        }
    }

    fn drop(&mut self, obj: Object) {
        self.dropped.push((String::from(obj.ty), obj.id));
        if obj.ty == "mesh" {
            self.meshes[obj.id as usize] = None;
        }
    }
}

/// the description of TestHost (as an app writes it, in its language; English names)
fn api() -> Api {
    Api::new("Testhost")
        .doc("Ein Host für die Prüfungen von S2.")
        .func("vec(x: float, y: float, z: float = 0): vec", "Ein neuer Vektor.")
        .func("cube(size: float = 1): mesh", "Ein Würfel.")
        .func("elem(name: str): element", "Das Element mit diesem Namen.")
        .value("doc: element", "Das Wurzelelement der Seite.")
        .namespace("log", "Das Protokoll.")
        .func("log.warn(...values): nil", "Eine Warnung ins Protokoll.")
        .typ(
            Type::new("vec", "Ein Vektor mit drei Komponenten.")
                .value()
                .prop("readonly x: float", "Die x-Komponente.")
                .prop("readonly y: float", "Die y-Komponente.")
                .prop("readonly z: float", "Die z-Komponente.")
                .method("length(): float", "Die Länge.")
                .method("dot(other: vec): float", "Das Skalarprodukt.")
                .op("vec + vec: vec", "Die Summe.")
                .op("vec - vec: vec", "Die Differenz.")
                .op("vec * float: vec", "Gestreckt.")
                .op("float * vec: vec", "Gestreckt.")
                .op("vec / float: vec", "Gestaucht.")
                .op("-vec: vec", "Umgekehrt.")
                .op("vec < vec: bool", "Vergleicht die Längen."),
        )
        .typ(
            Type::new("mesh", "Ein Netz aus Flächen.")
                .prop("readonly faces: int", "Die Zahl der Flächen.")
                .prop("readonly parent: mesh", "Das Netz, aus dem es entstand.")
                .method("bevel(width: float, segments: int = 1): mesh", "Ein neues Netz mit abgeschrägten Kanten.")
                .method("move(to: vec = vec(0, 0, 0), fast: bool = false): mesh", "Verschoben.")
                .method("same(): mesh", "Dasselbe Netz."),
        )
        .typ(
            Type::new("element", "Ein Element der Seite.")
                .prop("text: str", "Der Text.")
                .prop("readonly name: str", "Der Name.")
                .prop("readonly parent: element", "Das Element darüber.")
                .prop("readonly children: list<element>", "Die Elemente darin.")
                .method("click(): nil", "Löst click aus.")
                .method("remove(): int", "Entfernt das Element.")
                .event("click: event", "Ein Klick; steigt zu den Eltern auf.")
                .event("input: event", "Eine Eingabe; bleibt beim Element."),
        )
        .typ(
            Type::new("event", "Ein Ereignis.")
                .prop("readonly type: str", "click oder input.")
                .prop("readonly target: element", "Wo es geschah.")
                .prop("readonly current: element", "Wessen Funktion gerade läuft.")
                .method("stop(): nil", "Hält das Aufsteigen an."),
        )
}

/// a VM for src with the host's names; doc is the root element
fn vm_for(src: &str, lim: Limits) -> Result<Vm, String> {
    let a = api();
    let prog = script::compile_with("host.jfs", src, &a.names()).map_err(|d| d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("; "))?;
    let mut vm = Vm::new(&prog, lim);
    let root = vm.new_object("element", 0).map_err(|e| e.msg)?;
    vm.set_global("doc", root);
    Ok(vm)
}

/// run the pending invocation to its end: the result as text or "ERR line:col message"
fn finish(vm: &mut Vm, h: &mut TestHost) -> String {
    loop {
        match vm.run(h, u64::MAX) {
            Status::Done(v) => return vm.display(v),
            Status::Yielded | Status::Paused(_) => {}
            Status::Error(e) => return format!("ERR {}:{} {}", e.line, e.col, e.msg),
        }
    }
}

/// the result of src (its top-level `return`-less value: the last call's) with a fresh host
fn eval(src: &str) -> String {
    let mut h = TestHost::new();
    eval_in(src, &mut h)
}

fn eval_in(src: &str, h: &mut TestHost) -> String {
    let wrapped = format!("fn main() {{\n{}\n}}\n", src);
    let mut vm = match vm_for(&wrapped, Limits::default()) {
        Ok(v) => v,
        Err(e) => return format!("COMPILE {}", e),
    };
    let r = finish(&mut vm, h);
    if r.starts_with("ERR") {
        return r;
    }
    if let Err(e) = vm.call("main", &[]) {
        return format!("ERR {}", e.msg);
    }
    finish(&mut vm, h)
}

/// (scripttest --bench vm) what host objects cost: microseconds per 100 000 rounds of a loop
/// with a property set, a property read, a vec operator, a method that makes a mesh, against
/// the same loop with a map
pub fn bench() {
    let progs: [(&str, &str); 5] = [
        ("map field", "let m = {text: \"\"}\nfor i in 0..100000 { m.text = \"x\" }"),
        ("set prop", "let b = elem(\"btn\")\nfor i in 0..100000 { b.text = \"x\" }"),
        ("get prop", "let m = cube(1)\nlet s = 0\nfor i in 0..100000 { s += m.faces }"),
        ("vec +", "let v = vec(0, 0, 0)\nlet d = vec(1, 2, 3)\nfor i in 0..100000 { v = v + d }"),
        ("method", "let m = cube(1)\nfor i in 0..100000 { let n = m.same() }"),
    ];
    for (name, src) in progs {
        let mut best = u64::MAX;
        for _ in 0..5 {
            let mut h = TestHost::new();
            let mut vm = vm_for(src, Limits { fuel: u64::MAX, ..Limits::default() }).unwrap();
            let t0 = plat::time_us();
            let _ = finish(&mut vm, &mut h);
            best = best.min(plat::time_us().saturating_sub(t0));
        }
        plat::write_stdout(format!("host {:<10} {:>8} us per 100 000 rounds\n", name, best).as_bytes());
    }
}

pub fn run(t: &mut T) {
    values(t);
    errors(t);
    meshes(t);
    drops(t);
    elements(t);
    events(t);
    limits(t);
    slices(t);
    default_method(t);
    services(t);
    diagnostics(t);
    catalogue(t);
    forms(t);
    binding(t);
    calls(t);
    form_services(t);
    form_catalogue(t);
    compiled(t);
    type_names(t);
    suggestions(t);
}

// ---------------------------------------------------------------- the VM

fn values(t: &mut T) {
    // (main's lines are 2.., so line 2 is the first line of a case)
    let r = eval(
        "let a = vec(1, 2, 3)\nlet b = vec(4, 5, 6)\nlet c = a\nc += b\n\
         return [type(a), a.x, a.y, a.z, a + b == vec(5, 7, 9), a - b == vec(-3, -3, -3), a * 2 == vec(2, 4, 6), 2 * a == vec(2, 4, 6), \
         -a == vec(-1, -2, -3), a / 2 == vec(0.5, 1, 1.5), a == vec(1, 2, 3), a != b, a == a, [a].contains(vec(1, 2, 3)), a < b, b <= a, \
         c == vec(5, 7, 9), a == vec(1, 2, 3), a.length() < 4, a.dot(b), str(a), \"${b}\", type(type(a))]",
    );
    t.eq(
        "vec: properties, operators, == by value, value semantics, text",
        r.as_str(),
        "[\"vec\", 1.0, 2.0, 3.0, true, true, true, true, true, true, true, true, true, true, true, false, true, true, true, 32.0, \"vec(1.0, 2.0, 3.0)\", \"vec(4.0, 5.0, 6.0)\", \"str\"]",
    );
    // two values of different types are never equal, an object is truthy
    t.eq("vec against others", eval("let a = vec(0, 0, 0)\nreturn [a == 0, a == nil, a == [0, 0, 0], !a, a ?? 1, a == cube(1)]").as_str(), "[false, false, false, false, vec(0.0, 0.0, 0.0), false]");
    // Value::as_float
    t.eq("as_float", (Value::Int(3).as_float(), Value::Float(0.5).as_float(), Value::Nil.as_float()), (Some(3.0), Some(0.5), None));
    // print, str and interpolation ask the host; Vm::display does not
    let mut h = TestHost::new();
    let mut vm = vm_for("let m = cube(1)\nprint(m, [m, vec(1, 2, 3)], doc)\nlog(m)\nlet s = \"${m}|\" + str(m)\n", Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    t.eq("print asks the host", vm.take_output().as_str(), "mesh#0(6 Flächen) [mesh#0(6 Flächen), vec(1.0, 2.0, 3.0)] <root>\n");
    let m = vm.global("m").unwrap();
    t.eq("display without the host", vm.display(m).as_str(), "<mesh 0>");
    t.eq("display_with", vm.display_with(m, &h).as_str(), "mesh#0(6 Flächen)");
    t.eq("str and ${} ask the host", vm.str(vm.global("s").unwrap()).unwrap_or(""), "mesh#0(6 Flächen)|mesh#0(6 Flächen)");
    t.eq("cx.display_with", h.log.clone(), vec![String::from("mesh#0(6 Flächen)")]);
    t.eq("object", vm.object(m).map(|o| (o.ty, o.id)), Some(("mesh", 0)));
    t.eq("object of a value", vm.object(Value::Int(1)), None);
    t.eq("type_of", (vm.type_of(m), vm.type_of(Value::Int(1))), ("mesh", "int"));
    t.check("data of a mesh is none", vm.data(m).is_none());
    let v = vm.new_value("vec", &[Value::Float(1.0), Value::Int(2), Value::Nil]).unwrap();
    t.eq("data of a value", vm.data(v).map(|d| d.to_vec()), Some(vec![Value::Float(1.0), Value::Int(2), Value::Nil]));
    // the debugger's views
    t.eq("children of a value", vm.children(v).into_iter().map(|(k, x)| (k, vm.repr(x))).collect::<Vec<_>>(), vec![(String::from("0"), String::from("1.0")), (String::from("1"), String::from("2")), (String::from("2"), String::from("nil"))]);
    t.eq("children of an object", vm.children(m).len(), 0);
    t.eq("repr and preview", (vm.repr(v), vm.preview(m, 4)), (String::from("vec(1.0, 2, nil)"), String::from("<me…")));
    t.eq("a value's object has id 0", vm.object(v).map(|o| (o.ty, o.id)), Some(("vec", 0)));
    let w = vm.new_value("vec", &[Value::Int(1), Value::Float(2.0), Value::Nil]).unwrap();
    t.check("Vm::equal of values", vm.equal(v, w));
    let p = vm.new_value("pt", &[Value::Float(1.0), Value::Int(2), Value::Nil]).unwrap();
    t.check("a new value of another type is not equal", !vm.equal(v, p));
    t.eq("json refuses host objects", eval("return json.encode([vec(1, 2, 3)])").as_str(), "ERR 2:13 json.encode: vec (ein Objekt des Hosts) lässt sich nicht als JSON darstellen");
}

fn errors(t: &mut T) {
    let cases: &[(&str, &str)] = &[
        // a host error in a setter: thrown at the property, catchable
        ("let a = vec(1, 2, 3)\na.x = 5", "ERR 3:3 vec lässt sich nicht ändern"),
        ("let a = vec(1, 2, 3)\ntry { a.x = 5 } catch e { return e }", "vec lässt sich nicht ändern"),
        ("let a = vec(1, 2, 3)\nreturn a + \"x\"", "ERR 3:10 Der Operator + passt nicht zu vec und str"),
        ("try { return 1 + vec(1, 2, 3) } catch e { return e }", "Der Operator + passt nicht zu int und vec"),
        ("return vec(1, 2, 3).q", "ERR 2:21 vec hat keine Eigenschaft q"),
        ("return vec(1, 2, 3).foo(1)", "ERR 2:21 vec hat keine Methode foo"),
        ("return -cube(1)", "ERR 2:8 Das Minus passt nicht zu mesh"),
        ("return cube(1) < cube(2)", "ERR 2:16 Der Vergleich < passt nicht zu mesh und mesh"),
        ("return cube(1) * 2", "ERR 2:16 Der Operator * passt nicht zu mesh und int"),
        ("let m = cube(1)\nm.faces = 3", "ERR 3:3 mesh.faces ist nur lesbar"),
        ("let m = cube(1)\nm.color = 3", "ERR 3:3 mesh hat keine Eigenschaft color"),
        ("let m = cube(1)\ntry { return m.boom } catch e { return \"gefangen: \" + e }", "gefangen: kaputt"),
        ("let m = cube(1)\nlet x = 1\nreturn m.boom", "ERR 4:10 kaputt"),
        ("return len(vec(1, 2, 3))", "ERR 2:8 len braucht str, bytes, list oder map, nicht vec"),
        ("for x in cube(1) { }", "ERR 2:10 for braucht list, map, str oder bytes, nicht mesh"),
        ("return vec(1, 2, 3)[0]", "ERR 2:20 vec lässt sich nicht indizieren"),
        ("return cube(1)()", "ERR 2:8 mesh ist keine Funktion"),
        ("return {a: 1}.a + doc.name", "ERR 2:17 Der Operator + passt nicht zu int und str"),
        ("doc.text = 5", "ERR 2:5 element.text muss str sein, nicht int"),
        ("return cube(1).stale", "ERR 2:16 stale gab einen ungültigen Wert zurück"),
    ];
    for (src, want) in cases {
        t.eq(src, eval(src).as_str(), *want);
    }
    // the trace of an error in a getter names the function
    let mut h = TestHost::new();
    let mut vm = vm_for("fn get(m) { return m.boom }\nfn main() { return get(cube(1)) }\n", Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    vm.call("main", &[]).unwrap();
    match vm.run(&mut h, u64::MAX) {
        Status::Error(e) => {
            t.eq("getter error: kind and place", (e.kind, e.line, e.col, e.msg.as_str()), (ErrorKind::Uncaught, 1, 22, "kaputt"));
            t.eq("getter error: trace", e.trace.iter().map(|x| (x.func.clone(), x.line)).collect::<Vec<_>>(), vec![(String::from("get"), 1), (String::from("main"), 2)]);
        }
        s => t.check(&format!("getter error: {:?}", s), false),
    }
}

fn meshes(t: &mut T) {
    let r = eval(
        "let m = cube(1)\nlet n = m.bevel(0.1)\nlet k = n.bevel(0.2, 3)\n\
         return [type(m), m.faces, n.faces, k.faces, m == m, m == n, m.same() == m, cube(1) == cube(1), m != n, n.parent == m, k.parent.parent == m, m.parent, str(k)]",
    );
    t.eq("mesh: methods make objects, identity", r.as_str(), "[\"mesh\", 6, 18, 54, true, false, true, false, true, true, true, nil, \"mesh#2(54 Flächen)\"]");
    // the same (type, id) is the same object as long as the script holds it
    let mut h = TestHost::new();
    let mut vm = vm_for("let m = cube(1)\nlet same = m.same()\n", Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    let (m, s) = (vm.global("m").unwrap(), vm.global("same").unwrap());
    t.check("interned: one value", m == s);
    t.eq("find_object", vm.find_object("mesh", 0), Some(m));
    t.eq("find_object of none", vm.find_object("mesh", 5), None);
    t.eq("new_object between runs gives the live one", vm.new_object("mesh", 0).ok(), Some(m));
}

fn drops(t: &mut T) {
    // 2000 temporary meshes: the collector frees all but the two the script keeps
    let mut h = TestHost::new();
    let mut vm = vm_for("let keep = cube(1)\nlet kid = keep.bevel(1)\nfn churn(n) { for i in 0..n { let x = cube(1).bevel(0.5) } }\n", Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    vm.call("churn", &[Value::Int(1000)]).unwrap();
    let _ = finish(&mut vm, &mut h);
    vm.collect();
    // (the collection between runs reaches the host with the next run, or take_dropped)
    let d = vm.take_dropped();
    t.eq("dropped: every temporary mesh", d.len() + h.dropped.len(), 2000);
    for o in &d {
        h.drop(*o);
    }
    t.check("dropped: only meshes, value nil", d.iter().all(|o| o.ty == "mesh" && o.value == Value::Nil));
    t.check("dropped: the kept ones live", h.meshes[0].is_some() && h.meshes[1].is_some() && !h.dropped.iter().any(|x| x.1 < 2));
    t.eq("dropped: the host freed the rest", h.meshes.iter().filter(|m| m.is_some()).count(), 2);
    t.eq("dropped: nothing twice", {
        let mut ids: Vec<u64> = h.dropped.iter().map(|x| x.1).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }, h.dropped.len());
    // during a long run: Host::drop comes before the next host call, so the host can reuse ids
    let mut h = TestHost::new();
    let mut vm = vm_for("fn churn(n) { for i in 0..n { let x = cube(1) } }\n", Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    vm.call("churn", &[Value::Int(40_000)]).unwrap();
    let _ = finish(&mut vm, &mut h);
    t.check(&format!("drops during the run keep the host small (peak {})", h.peak), h.peak > 1000 && h.peak < 20_000);
    vm.collect();
    t.eq("drops during the run: all of them after a last collection", h.dropped.len() + vm.take_dropped().len(), 40_000);
    // the host hears of drops at the same points however the run is split: reused ids, the
    // order of the drops and what the script sees are the same
    let churn = |slice: u64| {
        let mut h = TestHost::new();
        let mut vm = vm_for("let ids = []\nfor i in 0..12000 { let m = cube(1)\n if i % 1000 == 0 { ids.push(str(m)) } }\n", Limits::default()).unwrap();
        loop {
            match vm.run(&mut h, slice) {
                Status::Yielded => {}
                _ => break,
            }
        }
        (vm.display(vm.global("ids").unwrap()), h.dropped.len(), h.dropped.iter().map(|d| d.1).sum::<u64>(), h.peak)
    };
    let one = churn(u64::MAX);
    t.check(&format!("churn: the collector ran ({} drops)", one.1), one.1 > 5000);
    t.eq("drops: slices of 997 fuel give the same", churn(997), one.clone());
    t.eq("drops: slices of 1 fuel give the same", churn(1), one);
    // an object made again before the host heard of its drop: the drop is forgotten
    let mut vm = vm_for("", Limits::default()).unwrap();
    let mut h = TestHost::new();
    let _ = finish(&mut vm, &mut h);
    let _ = vm.new_object("mesh", 77).unwrap();
    vm.collect();
    let again = vm.new_object("mesh", 77).unwrap();
    t.eq("a drop of an object made again is forgotten", vm.take_dropped().len(), 0);
    t.eq("the new object is the live one", vm.find_object("mesh", 77), Some(again));
    // value types are never reported
    let mut vm = vm_for("fn many() { for i in 0..5000 { let v = vec(i, 0, 0) + vec(1, 1, 1) } }\n", Limits::default()).unwrap();
    let mut h = TestHost::new();
    let _ = finish(&mut vm, &mut h);
    vm.call("many", &[]).unwrap();
    let _ = finish(&mut vm, &mut h);
    vm.collect();
    t.eq("values are not dropped", (h.dropped.len(), vm.take_dropped().len()), (0, 0));
    // a clone holds the same objects and reports its own drops
    let mut h = TestHost::new();
    let mut vm = vm_for("let m = cube(1)\nfn free() { m = nil }\n", Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    let mut c = vm.clone();
    c.call("free", &[]).unwrap();
    let _ = finish(&mut c, &mut h);
    c.collect();
    t.eq("clone: its drop", c.take_dropped().iter().map(|o| (o.ty, o.id)).collect::<Vec<_>>(), vec![("mesh", 0)]);
    vm.collect();
    t.eq("clone: the original keeps its object", (vm.take_dropped().len(), vm.object(vm.global("m").unwrap()).map(|o| o.id)), (0, Some(0)));
}

fn elements(t: &mut T) {
    let mut h = TestHost::new();
    let r = eval_in(
        "let btn = elem(\"btn\")\nbtn.text = \"OK\"\nbtn.text += \"!\"\n\
         return [btn.text, btn.name, btn.parent.name, doc.children.map(fn(e) { return e.name }), elem(\"panel\") == btn.parent, str(btn), doc == btn.parent.parent]",
        &mut h,
    );
    t.eq("element: settable text, compound, chains", r.as_str(), "[\"OK!\", \"btn\", \"panel\", [\"panel\"], true, \"<btn>\", true]");
    t.eq("element: the host's text", h.elems[2].text.as_str(), "OK!");
    t.eq("element: read-only", eval("elem(\"btn\").name = \"x\"").as_str(), "ERR 2:13 element.name ist nur lesbar");
}

/// the click of the view at element `at`: the event object, then the host's way up with
/// Vm::emit per element while e.stop() was not called -> what each listener did
fn fire(vm: &mut Vm, h: &mut TestHost, at: u64, ty: &str) -> Vec<script::Handled> {
    let id = h.events.len() as u64;
    h.events.push(Ev { ty: String::from(ty), target: at, current: at, stopped: false });
    let ev = vm.new_object("event", id).unwrap();
    // (alive while the way goes up: new_object below may collect)
    let keep = vm.pin(ev);
    let mut all = Vec::new();
    for x in h.path(at, ty) {
        h.events[id as usize].current = x;
        let el = vm.new_object("element", x).unwrap();
        all.extend(vm.emit(h, el, ty, &[vm.pinned(keep)]).unwrap());
        if h.events[id as usize].stopped {
            break;
        }
    }
    vm.unpin(keep);
    all
}

fn events(t: &mut T) {
    let src = "let seen = []\nlet btn = elem(\"btn\")\nlet panel = elem(\"panel\")\n\
        fn on_btn(e) { seen.push(\"btn:\" + e.type + \":\" + e.target.name + \":\" + e.current.name) }\n\
        fn on_panel(e) { seen.push(\"panel:\" + e.current.name) }\n\
        fn stopper(e) { seen.push(\"stop\"); e.stop() }\n\
        btn.on(\"click\", on_btn)\nbtn.on(\"click\", on_btn)\npanel.on(\"click\", on_panel)\n\
        doc.on(\"click\", fn(e) { seen.push(\"root\") })\n\
        btn.on(\"input\", fn(e) { seen.push(\"input:\" + e.target.name) })\ndoc.on(\"input\", fn(e) { seen.push(\"never\") })\n\
        fn take() { let s = seen.join(\" \"); seen = []; return s }\n";
    let mut h = TestHost::new();
    let mut vm = vm_for(src, Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    let take = |vm: &mut Vm, h: &mut TestHost| -> String {
        vm.call("take", &[]).unwrap();
        let s = finish(vm, h);
        s.trim_matches('"').to_string()
    };
    t.eq("listen hook", h.listens.clone(), vec![String::from("on click"), String::from("on click"), String::from("on click"), String::from("on click"), String::from("on input"), String::from("on input")]);
    let btn = vm.global("btn").unwrap();
    t.eq("the same function once", vm.listeners(btn, "click").len(), 1);
    t.eq("listener_count, has_listeners", (vm.listener_count(), vm.has_listeners("click"), vm.has_listeners("hover")), (5, true, false));
    let r = fire(&mut vm, &mut h, 2, "click");
    t.eq("click bubbles up to the root", take(&mut vm, &mut h).as_str(), "btn:click:btn:btn panel:panel root");
    t.check("each listener an invocation with fuel", r.len() == 3 && r.iter().all(|x| x.error.is_none() && x.fuel > 0));
    let _ = fire(&mut vm, &mut h, 2, "input");
    t.eq("input stays at its element", take(&mut vm, &mut h).as_str(), "input:btn");
    // e.stop() in the panel's second listener ends the way up after the panel
    let mut n = 0;
    let mut add = |vm: &mut Vm, h: &mut TestHost, code: &str| {
        n += 1;
        match vm.add_script(&format!("more{}.jfs", n), code) {
            Ok(()) => {
                let r = finish(vm, h);
                if r.starts_with("ERR") {
                    h.log.push(r);
                }
            }
            Err(d) => h.log.push(format!("{:?}", d)),
        }
    };
    add(&mut vm, &mut h, "panel.on(\"click\", stopper)");
    let _ = fire(&mut vm, &mut h, 2, "click");
    t.eq("e.stop() ends the way up", take(&mut vm, &mut h).as_str(), "btn:click:btn:btn panel:panel stop");
    add(&mut vm, &mut h, "panel.off(\"click\", stopper)");
    let _ = fire(&mut vm, &mut h, 2, "click");
    t.eq("off takes it away", take(&mut vm, &mut h).as_str(), "btn:click:btn:btn panel:panel root");
    // a listener that throws, one that runs too long: each ends alone, the next one runs
    add(&mut vm, &mut h, "btn.on(\"click\", fn(e) { throw \"weg\" })\nbtn.on(\"click\", fn(e) { while true {} })\nbtn.on(\"click\", fn(e) { seen.push(\"after\") })");
    let r = fire(&mut vm, &mut h, 2, "click");
    let kinds: Vec<Option<ErrorKind>> = r.iter().map(|x| x.error.as_ref().map(|e| e.kind)).collect();
    t.eq("errors end only their listener", kinds, vec![None, Some(ErrorKind::Uncaught), Some(ErrorKind::Fuel), None, None, None]);
    t.eq("errors: the others ran", take(&mut vm, &mut h).as_str(), "btn:click:btn:btn after panel:panel root");
    // a listener that removes a later one of the same element: that one does not run
    add(&mut vm, &mut h, "fn second(e) { seen.push(\"second\") }\nbtn.off(\"click\")\nbtn.on(\"click\", fn(e) { seen.push(\"first\"); btn.off(\"click\", second) })\nbtn.on(\"click\", second)");
    t.eq("off: two", vm.listeners(btn, "click").len(), 2);
    let _ = fire(&mut vm, &mut h, 2, "click");
    t.eq("a listener removed by an earlier one does not run", take(&mut vm, &mut h).as_str(), "first panel:panel root");
    // el.click() of the script: the listeners inside the call, up the way
    add(&mut vm, &mut h, "btn.click()\nseen.push(\"done\")");
    t.eq("click() inside the script", take(&mut vm, &mut h).as_str(), "first panel:panel root done");
    t.eq("the added scripts ran", h.log.clone(), Vec::<String>::new());
    // wrong uses: catchable errors with the type's name
    let cases: &[(&str, &str)] = &[
        ("return elem(\"btn\").on(\"hover\", fn(e) {})", "ERR 2:20 element hat kein Ereignis hover"),
        ("try { elem(\"btn\").on(\"hover\", fn(e) {}) } catch e { return e }", "element hat kein Ereignis hover"),
        ("return elem(\"btn\").on(\"click\", 3)", "ERR 2:20 element.on: hier muss eine Funktion stehen, nicht int"),
        ("return elem(\"btn\").on(1, fn() {})", "ERR 2:20 element.on: das Ereignis muss str sein, nicht int"),
        ("return elem(\"btn\").on(\"click\", print)", "ERR 2:20 element.on: hier muss eine Funktion des Skripts stehen, etwa fn(e) { .. }"),
        ("return cube(1).on(\"x\", fn() {})", "ERR 2:16 mesh hat kein Ereignis x"),
        ("return elem(\"btn\").on(\"full\", fn() {})", "ERR 2:20 zu viele Ereignis-Funktionen"),
        ("return vec(1, 2, 3).off(\"x\")", "ERR 2:21 vec hat kein Ereignis x"),
    ];
    for (src, want) in cases {
        t.eq(src, eval(src).as_str(), *want);
    }
    // listeners keep their object alive; remove_listeners (the host deleted it) lets it go
    let mut h = TestHost::new();
    let mut vm = vm_for("let f = elem(\"field\")\nf.on(\"input\", fn(e) {})\nf = nil\nfn gone() { return elem(\"field\").remove() }\n", Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    vm.collect();
    t.eq("an object with listeners stays", vm.take_dropped().len(), 0);
    vm.call("gone", &[]).unwrap();
    t.eq("remove_listeners: how many", finish(&mut vm, &mut h).as_str(), "1");
    vm.collect();
    t.eq("then it is collected", vm.take_dropped().iter().map(|o| (o.ty, o.id)).collect::<Vec<_>>(), vec![("element", 3)]);
    // listeners count toward the heap
    let mut h = TestHost::new();
    let mut vm = vm_for("fn many(n) { let b = elem(\"btn\")\n for i in 0..n { b.on(\"click\", fn(e) {}) } }\n", Limits { heap: 64 << 10, ..Limits::default() }).unwrap();
    let _ = finish(&mut vm, &mut h);
    vm.call("many", &[Value::Int(100_000)]).unwrap();
    t.check("listeners: the heap limit holds", finish(&mut vm, &mut h).starts_with("ERR") && vm.listener_count() < 2000);
    // emit: wrong uses
    t.check("emit needs a host object", vm.emit(&mut h, Value::Int(1), "click", &[]).is_err());
    let panel = vm.new_object("element", 1).unwrap();
    t.eq("emit without listeners", vm.emit(&mut h, panel, "input", &[]).map(|v| v.len()), Ok(0));
}

fn limits(t: &mut T) {
    // fuel: a getter that spends it ends the invocation for good
    let lim = Limits { fuel: 1_000_000, ..Limits::default() };
    let mut h = TestHost::new();
    let mut vm = vm_for("fn main() { try { return cube(1).slow } catch e { return \"gefangen\" } }\n", lim).unwrap();
    let _ = finish(&mut vm, &mut h);
    vm.call("main", &[]).unwrap();
    match vm.run(&mut h, u64::MAX) {
        Status::Error(e) => t.eq("fuel in a getter", (e.kind, e.line), (ErrorKind::Fuel, 1)),
        s => t.check(&format!("fuel in a getter: {:?}", s), false),
    }
    // a property access costs fuel like any instruction
    let fuel = |src: &str| {
        let mut h = TestHost::new();
        let mut vm = vm_for(src, Limits::default()).unwrap();
        let _ = finish(&mut vm, &mut h);
        vm.call_fuel()
    };
    t.check("property reads cost fuel", fuel("let m = cube(1)\nlet s = 0\nfor i in 0..100 { s += m.faces }\n") > fuel("let m = cube(1)\nlet s = 0\nfor i in 0..100 { s += 6 }\n"));
    // a hook may yield: the run ends after it and goes on
    let mut h = TestHost::new();
    let mut vm = vm_for("let r = cube(1).pause + 1\n", Limits::default()).unwrap();
    t.eq("yield in a getter", vm.run(&mut h, u64::MAX), Status::Yielded);
    t.check("then on", matches!(vm.run(&mut h, u64::MAX), Status::Done(_)) && vm.global("r") == Some(Value::Int(8)));
}

/// a script with host objects split into runs of one fuel each gives what one run gives
fn slices(t: &mut T) {
    let src = "let a = vec(1, 2, 3)\nlet m = cube(1)\nlet out = []\nfor i in 0..20 {\n  a = a * 2 - vec(1, 1, 1)\n  m = m.bevel(0.1)\n  out.push(a.x + m.faces)\n}\nlet b = elem(\"btn\")\nb.text = \"${a}\"\nb.text += \"!\"\nout.push(b.text, -a == a * -1)\n";
    let whole = {
        let mut h = TestHost::new();
        let mut vm = vm_for(src, Limits::default()).unwrap();
        let _ = finish(&mut vm, &mut h);
        (vm.display(vm.global("out").unwrap()), vm.call_fuel(), h.meshes.len())
    };
    let sliced = {
        let mut h = TestHost::new();
        let mut vm = vm_for(src, Limits::default()).unwrap();
        let mut runs = 0;
        loop {
            runs += 1;
            match vm.run(&mut h, 1) {
                Status::Yielded => {}
                Status::Done(_) => break,
                s => {
                    t.check(&format!("slices: {:?}", s), false);
                    break;
                }
            }
        }
        t.check("slices: many runs", runs > 100);
        (vm.display(vm.global("out").unwrap()), vm.call_fuel(), h.meshes.len())
    };
    t.eq("slices of one fuel give the same", sliced, whole.clone());
    // with the debugger: pauses on the lines with host objects, then single steps, the same
    let debugged = {
        let mut h = TestHost::new();
        let mut vm = vm_for(src, Limits::default()).unwrap();
        vm.set_breakpoint("host.jfs", 5, None).unwrap();
        let (mut pauses, mut steps) = (0, 0);
        loop {
            match vm.run(&mut h, u64::MAX) {
                Status::Paused(_) => {
                    pauses += 1;
                    if pauses > 3 {
                        vm.clear_breakpoints();
                    }
                    if steps < 40 {
                        steps += 1;
                        vm.step(script::Step::Into);
                    }
                }
                Status::Yielded => {}
                Status::Done(_) => break,
                s => {
                    t.check(&format!("debugger: {:?}", s), false);
                    break;
                }
            }
        }
        t.check(&format!("debugger: it paused ({} pauses)", pauses), pauses > 3);
        (vm.display(vm.global("out").unwrap()), vm.call_fuel(), h.meshes.len())
    };
    t.eq("paused and stepped gives the same", debugged, whole);
    // a clone in the middle runs on alike
    let mut h = TestHost::new();
    let mut vm = vm_for(src, Limits::default()).unwrap();
    let _ = vm.run(&mut h, 200);
    let mut c = vm.clone();
    let mut h2 = TestHost::new();
    h2.meshes = (0..h.meshes.len()).map(|i| h.meshes[i].as_ref().map(|m| Mesh { faces: m.faces, parent: m.parent })).collect();
    h2.elems = h.elems.iter().map(|e| Elem { name: e.name.clone(), text: e.text.clone(), parent: e.parent }).collect();
    let _ = finish(&mut vm, &mut h);
    let _ = finish(&mut c, &mut h2);
    t.eq("clone in the middle", vm.display(c.global("out").unwrap()), vm.display(vm.global("out").unwrap()));
}

/// a host with call alone: methods of its objects reach Host::call as "type.name"
fn default_method(t: &mut T) {
    struct Old;
    impl Host for Old {
        fn call(&mut self, cx: &mut Cx, name: &str, args: &[Value]) -> Result<Value, Fail> {
            match name {
                "thing" => cx.new_object("thing", 4),
                "thing.twice" => Ok(Value::Int(2 * args.get(1).and_then(|v| v.as_float()).unwrap_or(0.0) as i64 + cx.object(args[0]).map_or(0, |o| o.id as i64))),
                _ => Err(Fail::Unknown),
            }
        }
    }
    let prog = script::compile_with("", "let r = thing().twice(5)\nlet t = type(thing())\nlet e = \"\"\ntry { thing().nope() } catch x { e = x }\ntry { thing().size } catch x { e += \"|\" + x }\n", &["thing"]).unwrap();
    let mut vm = Vm::new(&prog, Limits::default());
    let _ = vm.run(&mut Old, u64::MAX);
    t.eq("Host::method's default: Host::call(\"thing.twice\", [obj, ..])", vm.global("r"), Some(Value::Int(14)));
    t.eq("default hooks", vm.str(vm.global("e").unwrap()).unwrap_or("").to_string(), String::from("thing hat keine Methode nope|thing hat keine Eigenschaft size"));
    t.eq("type of a host object without a description", vm.str(vm.global("t").unwrap()), Some("thing"));
}

// ---------------------------------------------------------------- the language services

fn labels(src: &str, cx: &Context) -> Vec<String> {
    let at = src.find('|').expect("caret");
    let code = src.replacen('|', "", 1);
    ls::complete(&code, at as u32, cx).items.into_iter().map(|i| i.label).collect()
}

fn services(t: &mut T) {
    let a = api();
    let cx = Context::new(&a);
    t.eq("the description is whole", a.validate(), Vec::<String>::new());
    let has = |got: &[String], want: &[&str]| want.iter().all(|w| got.iter().any(|g| g == w));
    let none = |got: &[String], not: &[&str]| not.iter().all(|w| !got.iter().any(|g| g == w));
    let vecm = labels("vec(1, 2).|", &cx);
    t.check(&format!("after a host function's result: {:?}", vecm), has(&vecm, &["x", "y", "z", "length", "dot"]) && none(&vecm, &["split", "push", "on"]));
    let m = labels("let m = cube(1)\nm.|", &cx);
    t.check(&format!("a let of a host function: {:?}", m), has(&m, &["faces", "parent", "bevel", "same"]) && none(&m, &["x", "split"]));
    let ch = labels("let m = cube(1)\nm.bevel(0.1).parent.|", &cx);
    t.check(&format!("a chain of calls and properties: {:?}", ch), has(&ch, &["faces", "bevel"]));
    let ix = labels("doc.children[0].|", &cx);
    t.check(&format!("an item of a list<element>: {:?}", ix), has(&ix, &["text", "name", "on", "off", "click"]) && none(&ix, &["faces"]));
    let lp = labels("for e in doc.children {\n  e.|\n}", &cx);
    t.check(&format!("a for over a list<element>: {:?}", lp), has(&lp, &["text", "on"]));
    let ev = labels("doc.on(\"click\", fn(ev) {\n  ev.|\n})", &cx);
    t.check(&format!("the parameter of a listener: {:?}", ev), has(&ev, &["type", "target", "current", "stop"]) && none(&ev, &["text"]));
    let ev2 = labels("let b = elem(\"btn\")\nb.on(\"input\", fn(e) { e.target.| })", &cx);
    t.check(&format!("the event's target: {:?}", ev2), has(&ev2, &["text", "parent"]));
    let pre = labels("let m = cube(1)\nm.fa|", &cx);
    t.eq("a prefix", pre, vec![String::from("faces")]);
    let ns = labels("log.|", &cx);
    t.eq("a namespace of the description", ns, vec![String::from("warn")]);
    let op = labels("let v = vec(1, 2) * 2 + vec(0, 1)\nv.|", &cx);
    t.check(&format!("operators of the description: {:?}", op), has(&op, &["length"]));
    let neg = labels("let v = -vec(1, 2)\nv.|", &cx);
    t.check(&format!("unary minus of the description: {:?}", neg), has(&neg, &["dot"]));
    // not known: as before (all four types' methods), never a host type's
    let mixed = labels("let m = cube(1)\nm = 5\nm.|", &cx);
    t.check(&format!("assigned another type: not known: {:?}", mixed), has(&mixed, &["split", "push"]) && none(&mixed, &["faces"]));
    let param = labels("fn f(m) { m.| }", &cx);
    t.check(&format!("a parameter: not known: {:?}", param), has(&param, &["split"]) && none(&param, &["faces"]));
    let s = labels("let s = \"a\"\ns.|", &cx);
    t.check("a str stays a str", has(&s, &["split", "upper"]) && none(&s, &["push", "faces"]));
    let lib = labels("let l = doc.children\nl.|", &cx);
    t.check(&format!("a list<T> has the list's methods: {:?}", lib), has(&lib, &["push", "map"]) && none(&lib, &["split", "text"]));
    // names: the description's globals with their declarations
    let c = ls::complete("ve", 2, &cx);
    let v = c.items.iter().find(|i| i.label == "vec");
    t.eq("a global function", v.map(|i| (i.kind, i.detail.as_str(), i.doc.as_str())), Some((ItemKind::Function, "vec(x: float, y: float, z: float = 0): vec", "Ein neuer Vektor.")));
    let c = ls::complete("do", 2, &cx);
    t.eq("a global value", c.items.iter().find(|i| i.label == "doc").map(|i| (i.kind, i.detail.as_str())), Some((ItemKind::Value, "doc: element")));
    let c = ls::complete("lo", 2, &cx);
    t.eq("a namespace", c.items.iter().find(|i| i.label == "log").map(|i| i.kind), Some(ItemKind::Module));
    let c = ls::complete("let m = cube(1)\nm.", 18, &cx);
    t.eq("a property item", c.items.iter().find(|i| i.label == "faces").map(|i| (i.kind, i.detail.as_str())), Some((ItemKind::Value, "mesh.faces: int (nur lesbar)")));
    t.eq("a method item", c.items.iter().find(|i| i.label == "bevel").map(|i| (i.kind, i.detail.as_str())), Some((ItemKind::Method, "mesh.bevel(width: float, segments: int = 1): mesh")));
    // hover
    let hv = |src: &str| {
        let at = src.find('|').expect("caret");
        let code = src.replacen('|', "", 1);
        ls::hover(&code, at as u32, &cx).map(|h| (h.sig, h.kind, h.doc))
    };
    t.eq("hover: a method", hv("let m = cube(1)\nm.bev|el(1)"), Some((String::from("mesh.bevel(width: float, segments: int = 1): mesh"), String::from("Methode von mesh"), String::from("Ein neues Netz mit abgeschrägten Kanten."))));
    t.eq("hover: a property", hv("doc.te|xt"), Some((String::from("element.text: str"), String::from("Eigenschaft von element"), String::from("Der Text."))));
    t.eq("hover: a read-only property", hv("vec(1, 2).|x").map(|h| h.1), Some(String::from("Eigenschaft von vec, nur lesbar")));
    t.eq("hover: a variable's type", hv("let m = cube(1)\n|m").map(|h| h.0), Some(String::from("let m: mesh")));
    t.eq("hover: a listener's parameter", hv("doc.on(\"click\", fn(ev) { |ev })").map(|h| h.0), Some(String::from("ev: event")));
    t.eq("hover: a host function", hv("|vec(1, 2)").map(|h| (h.0, h.1)), Some((String::from("vec(x: float, y: float, z: float = 0): vec"), String::from("Name des Hosts"))));
    t.eq("hover: on", hv("doc.|on(\"click\", f)").map(|h| h.2), Some(String::from("click: event\ninput: event")));
    t.eq("hover: a member it does not have", hv("let m = cube(1)\nm.|nope"), None);
    // signature help
    let sg = |src: &str| {
        let at = src.find('|').expect("caret");
        let code = src.replacen('|', "", 1);
        ls::signature(&code, at as u32, &cx).map(|s| {
            let ps: Vec<String> = s.params.iter().map(|&(a, b)| String::from(&s.label[a as usize..b as usize])).collect();
            (s.label, ps, s.active)
        })
    };
    t.eq(
        "signature: a method",
        sg("let m = cube(1)\nm.bevel(0.1, |"),
        Some((String::from("mesh.bevel(width: float, segments: int = 1): mesh"), vec![String::from("width: float"), String::from("segments: int = 1")], 1)),
    );
    t.eq("signature: a default with commas", sg("cube(1).move(|").map(|s| s.1), Some(vec![String::from("to: vec = vec(0, 0, 0)"), String::from("fast: bool = false")]));
    t.eq("signature: a host function", sg("vec(1, |").map(|s| (s.0, s.2)), Some((String::from("vec(x: float, y: float, z: float = 0): vec"), 1)));
    t.eq("signature: a rest parameter", sg("log.warn(1, 2, |").map(|s| (s.1, s.2)), Some((vec![String::from("...values")], 0)));
    // the form before S2 still works, alone and beside an Api
    let items = [HostItem { name: "net.listen", params: Some("port, handlers"), doc: "Wartet." }, HostItem { name: "config", params: None, doc: "Die Einstellungen." }];
    let old = CompleteCx { host: &items, load: None };
    t.eq("CompleteCx: names", ls::complete("net.", 4, &old).items.iter().map(|i| i.label.clone()).collect::<Vec<_>>(), vec![String::from("listen")]);
    t.eq("CompleteCx: signature", ls::signature("net.listen(1, ", 14, &old).map(|s| s.active), Some(1));
    // (the call shapes of JFNetworker's and editortest's code: a temporary default, a field)
    t.check("CompleteCx::default()", ls::complete("pri", 3, &ls::CompleteCx::default()).items.iter().any(|i| i.label == "print"));
    t.check("CompleteCx: definition, hover", ls::definition("let a = 1\na", 10, &old).is_some() && ls::hover("print", 1, &ls::CompleteCx::default()).is_some());
    let both = Context { api: Some(&a), host: &items, load: None };
    let c = ls::complete("", 0, &both);
    t.check("Context: both kinds of names", c.items.iter().any(|i| i.label == "net") && c.items.iter().any(|i| i.label == "vec"));
    // Api::from_items: the same names
    let from = Api::from_items(&items);
    t.eq("from_items: globals", from.globals.iter().map(|g| g.name.clone()).collect::<Vec<_>>(), vec![String::from("net.listen"), String::from("config"), String::from("net")]);
    t.eq("from_items: names", from.names(), vec!["net", "config"]);
    let c = ls::complete("net.", 4, &from);
    t.eq("from_items: members", c.items.iter().map(|i| (i.label.clone(), i.detail.clone())).collect::<Vec<_>>(), vec![(String::from("listen"), String::from("net.listen(port, handlers)"))]);
    // compile_with(api.names()): a host's names, unknown ones refused
    t.check("compile_with(names)", script::compile_with("", "let v = vec(1, 2)\nlog.warn(v)\ndoc.text = \"x\"", &a.names()).is_ok());
    t.check("compile_with(names) refuses others", script::compile_with("", "vecc(1)", &a.names()).is_err());
}

fn diagnostics(t: &mut T) {
    let a = api();
    let cx = Context::new(&a);
    let d = |src: &str| ls::check(src, &cx).iter().map(|x| x.to_string()).collect::<Vec<_>>();
    t.eq("check: a clean script", d("let m = cube(1)\nlet n = m.bevel(0.1, 2)\nprint(n.faces + m.faces)\ndoc.text = \"x\"\ndoc.on(\"click\", fn(e) { e.stop() })\nlog.warn(1)"), Vec::<String>::new());
    let cases: &[(&str, &str)] = &[
        ("let m = cube(1)\nprint(m.facs)", "2:9: Warnung: mesh hat keine Eigenschaft facs – meinst du faces?"),
        ("let m = cube(1)\nm.faces = 3", "2:3: Warnung: mesh.faces ist nur lesbar"),
        ("let m = cube(1)\nlet f = m.bevel", "2:11: Warnung: bevel ist eine Methode von mesh, keine Eigenschaft – mit Klammern: bevel(..)"),
        ("cube(1).faces()", "1:9: Warnung: faces ist eine Eigenschaft von mesh, keine Methode – ohne Klammern: .faces"),
        ("cube(1).bevl(1)", "1:9: Warnung: mesh hat keine Methode bevl – meinst du bevel?"),
        ("doc.on(\"clik\", fn(e) {})", "1:8: Warnung: element hat kein Ereignis clik – meinst du click?"),
        ("cube(1).on(\"x\", fn(e) {})", "1:9: Warnung: mesh hat keine Ereignisse"),
        ("log.wran(1)", "1:5: Warnung: Unbekannter Name log.wran – meinst du log.warn?"),
        ("for e in doc.children { e.txt = 1 }", "1:27: Warnung: element hat keine Eigenschaft txt – meinst du text?"),
        ("doc.on(\"click\", fn(ev) { ev.targt })", "1:29: Warnung: event hat keine Eigenschaft targt – meinst du target?"),
        ("prnt(1)", "1:1: Fehler: Unbekannter Name prnt – meinst du print?"),
    ];
    for (src, want) in cases {
        t.eq(src, d(src), vec![String::from(*want)]);
    }
    // not known: no member diagnostics
    t.eq("check: unknown types say nothing", d("fn f(m) { return m.whatever }\nlet x = cube(1)\nx = 2\nprint(x.nope)"), Vec::<String>::new());
    // without a description: as script::check
    let old = CompleteCx::default();
    let src = "let x = 7 / 2\nprnt(x)";
    t.eq("check without a description is script::check", ls::check(src, &old), script::check(src));
}

fn catalogue(t: &mut T) {
    let a = api();
    let cat = a.catalogue();
    let names: Vec<(EntryKind, String)> = cat.iter().map(|e| (e.kind, e.name.clone())).collect();
    t.eq(
        "catalogue: globals, then types, by name",
        names,
        vec![
            (EntryKind::Function, String::from("cube")),
            (EntryKind::Value, String::from("doc")),
            (EntryKind::Function, String::from("elem")),
            (EntryKind::Namespace, String::from("log")),
            (EntryKind::Function, String::from("vec")),
            (EntryKind::Type, String::from("element")),
            (EntryKind::Type, String::from("event")),
            (EntryKind::Type, String::from("mesh")),
            (EntryKind::ValueType, String::from("vec")),
        ],
    );
    t.eq("catalogue: a namespace's members", cat[3].children.iter().map(|e| e.label.clone()).collect::<Vec<_>>(), vec![String::from("log.warn(...values): nil")]);
    let v = &cat[8];
    t.eq(
        "catalogue: a type's members",
        v.children.iter().map(|e| (e.kind, e.label.clone())).take(5).collect::<Vec<_>>(),
        vec![
            (EntryKind::Property, String::from("readonly x: float")),
            (EntryKind::Property, String::from("readonly y: float")),
            (EntryKind::Property, String::from("readonly z: float")),
            (EntryKind::Method, String::from("dot(other: vec): float")),
            (EntryKind::Method, String::from("length(): float")),
        ],
    );
    t.eq("catalogue: operators as described", v.children.iter().filter(|e| e.kind == EntryKind::Operator).map(|e| e.label.clone()).collect::<Vec<_>>(), vec![
        String::from("vec + vec: vec"),
        String::from("vec - vec: vec"),
        String::from("vec * float: vec"),
        String::from("float * vec: vec"),
        String::from("vec / float: vec"),
        String::from("-vec: vec"),
        String::from("vec < vec: bool"),
    ]);
    let md = a.markdown();
    let want_lines = [
        "# Testhost",
        "Ein Host für die Prüfungen von S2.",
        "## Globale Namen",
        "- `cube(size: float = 1): mesh` – Ein Würfel.",
        "- `doc: element` – Das Wurzelelement der Seite.",
        "- `log` – Das Protokoll.",
        "  - `log.warn(...values): nil` – Eine Warnung ins Protokoll.",
        "## Typen",
        "### element",
        "Ereignisse (obj.on(name, fn(e) { .. })):",
        "- `click: event` – Ein Klick; steigt zu den Eltern auf.",
        "- `readonly children: list<element>` – Die Elemente darin.",
        "### vec (Werttyp: == vergleicht die Werte)",
        "- `float * vec: vec` – Gestreckt.",
        "Operatoren:",
    ];
    for w in want_lines {
        t.check(&format!("markdown has {:?}", w), md.lines().any(|l| l == w));
    }
    let order = ["## Globale Namen", "## Typen", "### element", "### event", "### mesh", "### vec"];
    let pos: Vec<usize> = order.iter().map(|h| md.find(h).unwrap_or(usize::MAX)).collect();
    t.check("markdown: the sections in order", pos.windows(2).all(|w| w[0] < w[1]) && pos.iter().all(|p| *p != usize::MAX));
    t.check("markdown: ends with a line", md.ends_with('\n') && !md.ends_with("\n\n"));
    // validate tells what does not fit
    let bad = Api::new("x")
        .func("f(a: shape): nope", "")
        .value("f: int", "")
        .typ(Type::new("p", "").prop("q: int", "").method("q(): int", "").method("on(): nil", "").op("int + int: int", "").event("e: unknown", ""))
        .typ(Type::new("str", ""));
    let v = bad.validate();
    let want = [
        "f (a): unbekannter Typ shape",
        "f: unbekannter Typ nope",
        "f: der Name kommt zweimal vor",
        "p.q: der Name kommt zweimal vor",
        "p.on: on und off sind die Ereignisse der VM",
        "p Ereignis e: unbekannter Typ unknown",
        "p int + int: int: keiner der Operanden ist p",
        "str: ein Name der Sprache, kein Typ des Hosts",
    ];
    for w in want {
        t.check(&format!("validate: {}", w), v.iter().any(|x| x == w));
    }
    t.eq("validate: nothing else", v.len(), want.len());
}

// ---------------------------------------------------------------- (S3) the gaps the first S2 host found

/// (S3) a description with forms (overloads), optional parameters, unions and removed names,
/// whose calls are counted (check_calls)
fn api3() -> Api {
    Api::new("Testhost S3")
        .doc("Formen, optionale Parameter, Vereinigungen.")
        .func("vec(x: float, y: float, z?: float): vec", "Ein Vektor aus Zahlen.")
        .func("vec(v: vec | list): vec", "Ein Vektor aus einem Vektor oder einer Liste.")
        .func("vec.dot(a: vec, b: vec): float", "Das Skalarprodukt.")
        .func("cube(size: number = 1): mesh", "Ein Würfel.")
        .func("load(path: str): mesh", "Ein Netz aus einer Datei.")
        .func("load(id: int): vec", "Ein gespeicherter Punkt.")
        .func("put(v: vec, at?: int): nil", "Legt einen Punkt ab.")
        .func("put(s: str, at?: int): nil", "Legt einen Text ab.")
        .func("pick(a: number | vec): number | vec", "Das Argument.")
        .namespace("geom", "Körper.")
        .func("geom.box(w: float, h: float, d: float): mesh", "Ein Quader.")
        .func("geom.cone(r: float, h: float): mesh", "Ein Kegel.")
        .func("geom.merge(...meshes: mesh): mesh", "Zusammengefügt.")
        .removed("geom.move", "m.translate")
        .removed("box", "geom.box")
        .value("sel: mesh | nil", "Die Auswahl, wenn es eine gibt.")
        .typ(Type::new("vec", "Ein Vektor.").value().prop("readonly x: float", "Die x-Komponente.").method("length(): float", "Die Länge."))
        .typ(
            Type::new("mesh", "Ein Netz.")
                .prop("readonly faces: int", "Die Zahl der Flächen.")
                .method("translate(x: float, y: float, z: float): mesh", "Verschoben um x, y, z.")
                .method("translate(v: vec): mesh", "Verschoben um v.")
                .method("scale(s: number | vec): mesh", "Skaliert.")
                .method("bevel(width: float, segments?: int): mesh", "Abgeschrägt.")
                .removed("move", "translate"),
        )
        .check_calls()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| String::from(*s)).collect()
}

/// optional parameters, unions, forms as the description has them; fits
fn forms(t: &mut T) {
    let a = api3();
    t.eq("forms: the description is whole", a.validate(), Vec::<String>::new());
    let v = a.sigs("vec");
    t.eq("forms: vec has two", v.iter().map(|s| s.label("vec")).collect::<Vec<_>>(), strings(&["vec(x: float, y: float, z?: float): vec", "vec(v: vec | list): vec"]));
    let p = &v[0].params[2];
    t.eq("optional: z?", (p.name.as_str(), p.ty.as_str(), p.optional, p.default.clone(), p.rest), ("z", "float", true, None, false));
    t.eq("a union", v[1].params[0].ty.as_str(), "vec | list");
    t.eq("arity", (v[0].arity(), v[1].arity(), a.sigs("geom.merge")[0].arity(), a.sigs("cube")[0].arity()), ((2, Some(3)), (1, Some(1)), (0, None), (0, Some(1))));
    // the signatures of S2 mean what they meant (a default makes a parameter optional)
    let old = Api::new("s2").func("f(a: float, b: int = 1, ...rest): mesh", "");
    let s = old.sigs("f")[0];
    t.eq(
        "S2 parameters",
        s.params.iter().map(|p| (p.name.as_str(), p.ty.as_str(), p.default.as_deref(), p.rest, p.optional)).collect::<Vec<_>>(),
        vec![("a", "float", None, false, false), ("b", "int", Some("1"), false, true), ("rest", "", None, true, false)],
    );
    t.eq("S2 label", s.label("f"), String::from("f(a: float, b: int = 1, ...rest): mesh"));
    // unions written without blanks, in properties and results
    let u = Api::new("u").func("f(a: int|str): vec|nil", "").typ(Type::new("vec", "").prop("p:int | list<vec>", ""));
    let f = u.sigs("f")[0];
    t.eq("union text", (f.params[0].ty.as_str(), f.ret.as_str(), u.types[0].props[0].ty.as_str()), ("int | str", "vec | nil", "int | list<vec>"));
    t.eq("union valid", u.validate(), Vec::<String>::new());
    // the forms of a method; global() and find_method() give the first
    let m = a.get_type("mesh").unwrap();
    t.eq("method forms", m.sigs("translate").len(), 2);
    t.eq("find_methods", m.find_methods("translate").iter().map(|x| x.doc.as_str()).collect::<Vec<_>>(), vec!["Verschoben um x, y, z.", "Verschoben um v."]);
    t.eq("find_method: the first", m.find_method("translate").map(|x| x.sig.params.len()), Some(3));
    t.eq("global: the first", a.global("vec").map(|g| g.doc.as_str()), Some("Ein Vektor aus Zahlen."));
    t.eq("funcs", a.funcs("load").len(), 2);
    let cases = [
        ("int", "float", true),
        ("float", "int", false),
        ("int", "number", true),
        ("float", "number", true),
        ("str", "number", false),
        ("list", "list<vec>", true),
        ("list<vec>", "list", true),
        ("list<int>", "list<float>", true),
        ("list<str>", "list<vec>", false),
        ("vec", "vec | list", true),
        ("str", "vec | list", false),
        ("int | float", "number", true),
        ("int | str", "int", false),
        ("", "vec", true),
        ("any", "vec", true),
        ("nil", "vec", false),
        ("nil", "vec | nil", true),
    ];
    for (got, want, yes) in cases {
        t.eq(&format!("fits({}, {})", got, want), api::fits(got, want), yes);
    }
}

/// which form a call binds to, and which parameter each argument fills
fn binding(t: &mut T) {
    let a = api3();
    let vf = a.sigs("vec");
    let pos = |tys: &[&'static str]| -> Vec<Arg<'static>> { tys.iter().map(|x| Arg::pos(x)).collect() };
    let form = |args: &[Arg]| api::bind(&vf, args).map(|b| (b.form, b.params.clone()));
    t.eq("bind: three numbers", form(&pos(&["int", "float", "int"])), Ok((0, vec![vec![0], vec![1], vec![2]])));
    t.eq("bind: z left out", form(&pos(&["int", "int"])), Ok((0, vec![vec![0], vec![1], vec![]])));
    t.eq("bind: a list", form(&pos(&["list"])), Ok((1, vec![vec![0]])));
    t.eq("bind: a vec", form(&pos(&["vec"])), Ok((1, vec![vec![0]])));
    t.eq("bind: a type not known: the form of that number", form(&pos(&[""])), Ok((1, vec![vec![0]])));
    t.eq("bind: a str fits none", form(&pos(&["str"])), Err(vec![Mismatch::Missing { param: 1 }, Mismatch::Type { arg: 0, param: 0 }]));
    t.eq("bind: too many", form(&pos(&["int"; 4])), Err(vec![Mismatch::TooMany { most: 3 }, Mismatch::TooMany { most: 1 }]));
    t.eq("bind: no forms", api::bind(&[], &pos(&["int"])).map(|b| b.form), Err(vec![]));
    // a rest parameter takes any number
    let merge = a.sigs("geom.merge");
    t.eq("bind: rest, none", api::bind(&merge, &[]).map(|b| b.params), Ok(vec![vec![]]));
    let b = api::bind(&merge, &pos(&["mesh", "mesh", "mesh"])).unwrap();
    t.eq("bind: rest, three", (b.params.clone(), b.args("meshes").to_vec(), b.arg("meshes")), (vec![vec![0, 1, 2]], vec![0, 1, 2], Some(0)));
    t.eq("bind: rest of a wrong type", api::bind(&merge, &pos(&["mesh", "vec"])).map(|b| b.form), Err(vec![Mismatch::Type { arg: 1, param: 0 }]));
    // nil is what a left-out argument is: it fits an optional parameter only
    let bevel = a.get_type("mesh").unwrap().sigs("bevel");
    t.check("bind: nil for an optional parameter", api::bind(&bevel, &pos(&["float", "nil"])).is_ok());
    t.eq("bind: nil for one that is not", api::bind(&bevel, &pos(&["nil"])).map(|b| b.form), Err(vec![Mismatch::Type { arg: 0, param: 0 }]));
    // named arguments (their syntax comes later in S3): by the parameter's name, after the
    // positional ones
    let s = vf[0];
    t.eq("bind: named", s.bind(&[Arg::pos("int"), Arg::named("z", "int"), Arg::named("y", "float")]), Ok(vec![vec![0], vec![2], vec![1]]));
    let b = api::bind(&vf, &[Arg::named("v", "list")]).unwrap();
    t.eq("bind: a name picks the form", (b.form, b.arg("v"), b.arg("x")), (1, Some(0), None));
    t.eq("bind: a name no parameter has", s.bind(&[Arg::pos("int"), Arg::named("w", "int")]), Err(Mismatch::NoParam { arg: 1 }));
    t.eq("bind: by position and by name", s.bind(&[Arg::pos("int"), Arg::pos("int"), Arg::named("x", "int")]), Err(Mismatch::Twice { arg: 2, param: 0 }));
    t.eq("bind: a rest parameter by name", merge[0].bind(&[Arg::named("meshes", "mesh")]), Err(Mismatch::NoParam { arg: 0 }));
    t.eq("bind: named, one missing", s.bind(&[Arg::named("y", "int")]), Err(Mismatch::Missing { param: 0 }));
}

/// ls::check with check_calls: the number of arguments against every form
fn calls(t: &mut T) {
    let a = api3();
    let cx = Context::new(&a);
    let d = |src: &str| ls::check(src, &cx).iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let clean = "let v = vec(1, 2)\nlet w = vec(1, 2, 3)\nlet u = vec([1, 2, 3])\nlet m = cube()\n\
                 let n = cube(2).translate(v).translate(1, 2, 3).bevel(0.1).bevel(0.1, 2).scale(2).scale(v)\n\
                 let g = geom.merge()\nlet h = geom.merge(m, n, m)\nprint(vec.dot(v, w), load(\"a\"), load(1), put(v), put(\"a\", 2))";
    t.eq("calls: all fit a form", d(clean), Vec::<String>::new());
    let cases: &[(&str, &str)] = &[
        ("geom.box(1, 2)", "1:6: Warnung: geom.box: 2 Argumente, braucht aber mindestens 3 (w, h, d)"),
        ("geom.box(1, 2, 3, 4)", "1:6: Warnung: geom.box: 4 Argumente, nimmt aber höchstens 3 (w, h, d)"),
        ("cube(1, 2)", "1:1: Warnung: cube: 2 Argumente, nimmt aber höchstens 1 (size)"),
        ("vec(1, 2, 3, 4)", "1:1: Warnung: vec: 4 Argumente passen zu keiner Form (vec(x, y, z), vec(v))"),
        ("vec()", "1:1: Warnung: vec: ohne Argumente passt keine Form (vec(x, y, z), vec(v))"),
        ("cube(1).translate(1, 2)", "1:9: Warnung: mesh.translate: 2 Argumente passen zu keiner Form (mesh.translate(x, y, z), mesh.translate(v))"),
        ("cube(1).bevel()", "1:9: Warnung: mesh.bevel: kein Argument, braucht aber mindestens 1 (width, segments)"),
        ("cube(1).bevel(1, 2, 3)", "1:9: Warnung: mesh.bevel: 3 Argumente, nimmt aber höchstens 2 (width, segments)"),
        ("vec(1, 2).length(1)", "1:11: Warnung: vec.length: 1 Argument, nimmt aber keine"),
        ("put(1, 2, 3)", "1:1: Warnung: put: 3 Argumente passen zu keiner Form (put(v, at), put(s, at))"),
    ];
    for (src, want) in cases {
        t.eq(src, d(src), vec![String::from(*want)]);
    }
    // a function of the script with a host's name is the script's
    t.eq("calls: the script's own vec", d("fn vec(a) { return a }\nprint(vec(1, 2, 3, 4))"), Vec::<String>::new());
    // without check_calls (descriptions of S2 name the longest form): nothing counted
    let s2 = api();
    t.eq("calls: not counted without check_calls", ls::check("cube(1, 2, 3)\nvec()\ncube(1).bevel()", &s2), Vec::new());
    // a host's own warning about extra arguments: the most any form reads
    let most = |forms: &[&script::api::Sig]| forms.iter().map(|s| s.arity().1).try_fold(0, |m, x| x.map(|x| m.max(x)));
    t.eq("the most arguments of the forms", (most(&a.sigs("vec")), most(&a.sigs("geom.merge")), most(&a.sigs("cube"))), (Some(3), None, Some(1)));
}

/// signature help, hover, completion and the types with forms and unions
fn form_services(t: &mut T) {
    let a = api3();
    let cx = Context::new(&a);
    let sg = |src: &str| {
        let at = src.find('|').expect("caret");
        let code = src.replacen('|', "", 1);
        ls::signature(&code, at as u32, &cx).map(|s| (s.forms.iter().map(|f| f.label.clone()).collect::<Vec<_>>(), s.form, s.label.clone(), s.active))
    };
    let vf = strings(&["vec(x: float, y: float, z?: float): vec", "vec(v: vec | list): vec"]);
    t.eq("signature: the forms, the first with room", sg("vec(|"), Some((vf.clone(), 0, vf[0].clone(), 0)));
    t.eq("signature: the active parameter", sg("vec(1, 2, |"), Some((vf.clone(), 0, vf[0].clone(), 2)));
    t.eq("signature: no form with room: the first", sg("vec(1, 2, 3, |").map(|x| (x.1, x.3)), Some((0, 3)));
    // put(v: vec, at?) and put(s: str, at?): the type of the first argument picks the form
    let put = |s: &str| sg(s).map(|x| (x.1, x.3));
    t.eq("signature: a str picks its form", put("put(\"a\", |"), Some((1, 1)));
    t.eq("signature: a vec picks its form", put("put(vec(1, 2), |"), Some((0, 1)));
    t.eq("signature: a let of a known type", put("let v = vec(1, 2)\nput(v, |"), Some((0, 1)));
    t.eq("signature: a type not known: the first", put("fn f(x) { put(x, | }"), Some((0, 1)));
    t.eq("signature: the form's description", ls::signature("put(\"a\", ", 9, &cx).map(|s| s.doc), Some(String::from("Legt einen Text ab.")));
    t.eq("signature: a method's forms", sg("cube(1).translate(|").map(|x| x.0), Some(strings(&["mesh.translate(x: float, y: float, z: float): mesh", "mesh.translate(v: vec): mesh"])));
    t.eq("signature: a union", sg("cube(1).scale(|").map(|x| x.2), Some(String::from("mesh.scale(s: number | vec): mesh")));
    t.eq("signature: one form", ls::signature("geom.box(1, ", 12, &cx).map(|s| (s.forms.len(), s.form, s.active)), Some((1, 0, 1)));
    // hover: each form on a line of its own
    let hv = |src: &str| {
        let at = src.find('|').expect("caret");
        let code = src.replacen('|', "", 1);
        ls::hover(&code, at as u32, &cx).map(|h| (h.sig, h.doc))
    };
    t.eq(
        "hover: a function's forms",
        hv("|vec(1, 2)"),
        Some((String::from("vec(x: float, y: float, z?: float): vec\nvec(v: vec | list): vec"), String::from("Ein Vektor aus Zahlen.\nEin Vektor aus einem Vektor oder einer Liste."))),
    );
    t.eq("hover: a method's forms", hv("cube(1).trans|late(1, 2, 3)").map(|h| h.0), Some(String::from("mesh.translate(x: float, y: float, z: float): mesh\nmesh.translate(v: vec): mesh")));
    t.eq("hover: one form", hv("geom.b|ox(1, 2, 3)").map(|h| h.0), Some(String::from("geom.box(w: float, h: float, d: float): mesh")));
    // completion: one item per name, with its first form
    let c = ls::complete("ve", 2, &cx);
    t.eq("complete: one vec", c.items.iter().filter(|i| i.label == "vec").map(|i| i.detail.clone()).collect::<Vec<_>>(), strings(&["vec(x: float, y: float, z?: float): vec"]));
    t.eq("complete: one translate", labels("cube(1).|", &cx).iter().filter(|l| *l == "translate").count(), 1);
    // a call's type: the result of the forms its arguments may bind to
    let has = |src: &str, w: &str| labels(src, &cx).iter().any(|g| g == w);
    t.check("load(str) is a mesh", has("load(\"a\").|", "faces"));
    t.check("load(int) is a vec", has("load(1).|", "length"));
    t.check("load(an int of a let) is a vec", has("let n = 1\nlet p = load(n)\np.|", "length"));
    t.check("load(x) is not known", !has("fn f(x) { load(x).| }", "faces") && !has("fn f(x) { load(x).| }", "length"));
    // T | nil has T's members
    t.check("a union with nil", has("sel.|", "faces"));
    let d = |src: &str| ls::check(src, &cx).iter().map(|x| x.to_string()).collect::<Vec<_>>();
    t.eq("check: a union with nil", d("print(sel.facs)"), strings(&["1:11: Warnung: mesh hat keine Eigenschaft facs – meinst du faces?"]));
    t.eq("check: another union has no members", d("print(pick(1).nope)"), Vec::<String>::new());
    t.check("complete: another union has no members", !has("pick(1).|", "length"));
    t.eq("check: forms of different results: not known", d("fn f(x) { return load(x).nope }"), Vec::<String>::new());
    t.eq("check: a known form's result", d("print(load(1).nope)"), strings(&["1:15: Warnung: vec hat keine Eigenschaft nope"]));
}

/// the catalogue, the Markdown page and validate with forms and unions
fn form_catalogue(t: &mut T) {
    let a = api3();
    let cat = a.catalogue();
    let vecs: Vec<&script::api::Entry> = cat.iter().filter(|e| e.name == "vec" && e.kind == EntryKind::Function).collect();
    t.eq("catalogue: a function's forms in their order", vecs.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(), vec!["vec(x: float, y: float, z?: float): vec", "vec(v: vec | list): vec"]);
    t.eq("catalogue: the namespace's members once", vecs.iter().map(|e| e.children.len()).collect::<Vec<_>>(), vec![1, 0]);
    let mesh = cat.iter().find(|e| e.name == "mesh").unwrap();
    t.eq(
        "catalogue: a method's forms",
        mesh.children.iter().filter(|e| e.name == "translate").map(|e| (e.kind, e.label.as_str(), e.doc.as_str())).collect::<Vec<_>>(),
        vec![(EntryKind::Method, "translate(x: float, y: float, z: float): mesh", "Verschoben um x, y, z."), (EntryKind::Method, "translate(v: vec): mesh", "Verschoben um v.")],
    );
    let md = a.markdown();
    for w in [
        "- `vec(x: float, y: float, z?: float): vec` – Ein Vektor aus Zahlen.",
        "- `vec(v: vec | list): vec` – Ein Vektor aus einem Vektor oder einer Liste.",
        "  - `vec.dot(a: vec, b: vec): float` – Das Skalarprodukt.",
        "- `sel: mesh | nil` – Die Auswahl, wenn es eine gibt.",
        "- `bevel(width: float, segments?: int): mesh` – Abgeschrägt.",
        "- `scale(s: number | vec): mesh` – Skaliert.",
        "- `translate(v: vec): mesh` – Verschoben um v.",
        "  - `geom.merge(...meshes: mesh): mesh` – Zusammengefügt.",
    ] {
        t.check(&format!("markdown has {:?}", w), md.lines().any(|l| l == w));
    }
    t.check("markdown: the forms in their order", md.find("vec(x: float").unwrap_or(usize::MAX) < md.find("vec(v: vec").unwrap_or(0));
    // validate: forms are no name twice; the same form twice, a rest parameter in the middle,
    // a removed name still described, a type of a union nobody describes are
    let bad = Api::new("x")
        .func("f(a: int): int", "")
        .func("f(a: int, b?: int): int", "")
        .func("f(a: int): int", "")
        .func("g(...a: int, b: int): nil", "")
        .func("h(a: int | shape): nil", "")
        .func("k(): nil", "")
        .removed("k", "f")
        .typ(Type::new("p", "").method("m(): nil", "").method("m(a: int): nil", "").method("m(): nil", "").prop("q: int", "").method("q(): int", "").removed("m", "q"));
    let v = bad.validate();
    let want = [
        "f: die Form f(a: int): int kommt zweimal vor",
        "g: ...a muss der letzte Parameter sein",
        "h (a): unbekannter Typ int | shape",
        "k: ist als entfernt angegeben und zugleich beschrieben",
        "p.m: die Form m(): nil kommt zweimal vor",
        "p.q: der Name kommt zweimal vor",
        "p.m: ist als entfernt angegeben und zugleich beschrieben",
    ];
    for w in want {
        t.check(&format!("validate: {}", w), v.iter().any(|x| x == w));
    }
    t.eq("validate: nothing else", v.len(), want.len());
}

/// compile_api: the program and the warnings of the same parse, also of modules and scripts
fn compiled(t: &mut T) {
    let a = api3();
    let src = "let m = cube(1)\nprint(m.facs)\nlet k = 7 / 2\ngeom.move(m)\n";
    let c = script::compile_api("main.jfs", src, &a);
    let got = c.as_ref().map(|c| c.warnings.iter().map(|d| format!("{}:{}", d.file, d)).collect::<Vec<_>>()).map_err(|e| e.len());
    let want: Vec<String> = ls::check(src, &a).iter().filter(|d| !d.error).map(|d| format!("main.jfs:{}", d)).collect();
    t.eq("compile_api: the warnings ls::check gives, with the file", got, Ok(want.clone()));
    t.eq("compile_api: a member, a division, a removed name", want.len(), 3);
    t.eq("compile_api: a member", want.first().map(|s| s.as_str()), Some("main.jfs:2:9: Warnung: mesh hat keine Eigenschaft facs – meinst du faces?"));
    t.check("compile_api: a division", want.get(1).is_some_and(|s| s.starts_with("main.jfs:3:9: Warnung: Ganzzahlige Division: 7 / 2")));
    t.eq("compile_api: a removed name", want.get(2).map(|s| s.as_str()), Some("main.jfs:4:6: Warnung: geom.move wurde entfernt – nimm m.translate"));
    // errors as compile_with gives them, a removed global name says what to take instead
    let e = |src: &str| match script::compile_api("x.jfs", src, &a) {
        Ok(_) => Vec::new(),
        Err(d) => d.iter().map(|x| format!("{} {}", x.file, x)).collect::<Vec<_>>(),
    };
    t.eq("compile_api: errors", e("prnt(1)\nbox(1, 2, 3)\nlet k = 7 / 2"), strings(&["x.jfs 1:1: Fehler: Unbekannter Name prnt – meinst du print?", "x.jfs 2:1: Fehler: box wurde entfernt – nimm geom.box"]));
    let with = script::compile_with("x.jfs", "prnt(1)", &a.names()).err().map(|d| d.iter().map(|x| format!("{} {}", x.file, x)).collect::<Vec<_>>());
    t.eq("compile_api: the errors of compile_with", Some(e("prnt(1)")), with);
    // the program runs as compile_with's
    let mut h = TestHost::new();
    let c = script::compile_api("ok.jfs", "let f = cube(2).faces", &a).unwrap();
    let mut vm = Vm::new(&c.program, Limits::default());
    let _ = finish(&mut vm, &mut h);
    t.eq("compile_api: the program runs", vm.global("f"), Some(Value::Int(6)));
    t.eq("compile_api: no warnings", (c.warnings.len(), c.program.name()), (0, "ok.jfs"));
    // a module imported through Host::load: its warnings, with its file, come with the VM's own
    let module = "fn f() {\n  return cube(1).facs\n}\nlet n = 7 / 2\n";
    let shown = |w: &[Warning]| w.iter().map(|x| format!("{}:{}:{}: {}", x.file, x.line, x.col, x.msg)).collect::<Vec<_>>();
    let mut h = TestHost::new();
    h.files.push((String::from("lib/m.jfs"), String::from(module)));
    let c = script::compile_api("app.jfs", "let m = import(\"lib/m.jfs\")\nlet r = m.n\n", &a).unwrap();
    let mut vm = Vm::new(&c.program, Limits::default());
    let _ = finish(&mut vm, &mut h);
    t.eq("compile_api: the module runs", vm.global("r"), Some(Value::Int(3)));
    let w = shown(&vm.take_warnings());
    t.eq("compile_api: a module's warnings", w.len(), 2);
    t.eq("compile_api: a module's member", w.first().map(|s| s.as_str()), Some("lib/m.jfs:2:18: mesh hat keine Eigenschaft facs – meinst du faces?"));
    t.check("compile_api: a module's division", w.get(1).is_some_and(|s| s.starts_with("lib/m.jfs:4:9: Ganzzahlige Division")));
    // compile_with as before: none
    let mut h = TestHost::new();
    h.files.push((String::from("lib/m.jfs"), String::from(module)));
    let p = script::compile_with("app.jfs", "let m = import(\"lib/m.jfs\")\nlet r = m.n\n", &a.names()).unwrap();
    let mut vm = Vm::new(&p, Limits::default());
    let _ = finish(&mut vm, &mut h);
    t.eq("compile_with: a module's warnings are not looked for", (vm.global("r"), vm.take_warnings().len()), (Some(Value::Int(3)), 0));
    // a further script of the VM (Vm::add_script)
    let mut h = TestHost::new();
    let c = script::compile_api("page.jfs", "let a = 1\n", &a).unwrap();
    let mut vm = Vm::new(&c.program, Limits::default());
    let _ = finish(&mut vm, &mut h);
    t.check("add_script", vm.add_script("two.jfs", "fn g() { return cube(1).facs }\nlet b = a + 1\n").is_ok());
    let _ = finish(&mut vm, &mut h);
    t.eq("add_script: its warnings", shown(&vm.take_warnings()), strings(&["two.jfs:1:25: mesh hat keine Eigenschaft facs – meinst du faces?"]));
}

/// messages name a host object's type by the host's name, as type(x) does
fn type_names(t: &mut T) {
    let cases: &[(&str, &str)] = &[
        ("return \"a\".split(cube(1))", "str.split: sep muss str sein, nicht mesh"),
        ("return [1].map(vec(1, 2, 3))", "list.map: f muss eine Funktion sein, nicht vec"),
        ("return int(cube(1))", "int(…) braucht eine Zahl oder einen Text, nicht mesh"),
        ("return float(vec(1, 2))", "float(…) braucht eine Zahl oder einen Text, nicht vec"),
        ("return math.abs(vec(1, 2, 3))", "math.abs: x muss eine Zahl sein, nicht vec"),
        ("let m = {}\nm[cube(1)] = 1", "Ein Map-Schlüssel muss str, int oder bool sein, nicht mesh"),
        ("return {a: 1}.get(cube(1))", "map.get: Ein Map-Schlüssel muss str, int oder bool sein, nicht mesh"),
        ("for i in 0..cube(1) { }", "Ein Bereich a..b braucht ganze Zahlen, nicht mesh"),
        ("return [cube(1), 1].sort()", "list.sort: int und mesh lassen sich nicht vergleichen – gib eine Vergleichsfunktion an"),
        ("return [3, 1].sort(fn(a, b) { return cube(1) })", "list.sort: cmp muss eine Zahl oder bool zurückgeben, nicht mesh"),
        ("return [1, 2].slice(vec(1, 2))", "list.slice: start muss int sein, nicht vec"),
        ("return list.range(0, vec(1, 2))", "list.range: end muss eine Zahl sein, nicht vec"),
        ("return bytes([vec(1, 2, 3)])", "bytes: Ein Byte muss int sein, nicht vec"),
        ("return \"{:d}\".format(cube(1))", "str.format: {:d} braucht int, nicht mesh"),
        ("return \"{a}\".format(cube(1))", "str.format: Für {a} muss das erste Argument eine Map sein, nicht mesh"),
        ("return import(cube(1))", "import braucht den Pfad einer Datei (str), nicht mesh"),
    ];
    for (src, want) in cases {
        let got = eval(src);
        t.check(&format!("{}: {}", src, got), got.starts_with("ERR ") && got.ends_with(want));
    }
    // a host calling a host object; Value::type_name stays "object" (it knows no VM)
    let mut h = TestHost::new();
    let mut vm = vm_for("let m = cube(1)\n", Limits::default()).unwrap();
    let _ = finish(&mut vm, &mut h);
    let m = vm.global("m").unwrap();
    t.eq("call_value of a host object", vm.call_value(m, &[]).err().map(|e| e.msg), Some(String::from("mesh ist keine Funktion")));
    t.eq("Value::type_name and Vm::type_of", (m.type_name(), vm.type_of(m)), ("object", "mesh"));
}

/// suggestions for unknown names: only really close ones
fn suggestions(t: &mut T) {
    let first = |d: Vec<script::Diagnostic>| d.into_iter().next().map(|x| x.msg).unwrap_or_default();
    let names: &[(&str, &str)] = &[
        // kept: a letter missing, two swapped, only the case, two changes in a long name
        ("print(prnt)", "Unbekannter Name prnt – meinst du print?"),
        ("pirnt(1)", "Unbekannter Name pirnt – meinst du print?"),
        ("PRINT(1)", "Unbekannter Name PRINT – meinst du print?"),
        ("let count = 0\ncoutn += 1", "Unbekannter Name coutn – meinst du count?"),
        ("let cylinder = 1\nprint(cilynder)", "Unbekannter Name cilynder – meinst du cylinder?"),
        // gone: two changes in a short name, another first letter
        ("print(loger)", "Unbekannter Name loger"),
        ("let format = 1\nprint(normal)", "Unbekannter Name normal"),
        ("x = 5", "Unbekannter Name x – eine neue Variable wird mit let x = … angelegt"),
    ];
    for (src, want) in names {
        t.eq(src, first(script::check(src)), String::from(*want));
    }
    // members of a host's namespace and types (ls::check with an Api)
    let g = Api::new("g").func("geom.box(w: float): mesh", "").func("geom.cone(r: float): mesh", "").typ(Type::new("mesh", "").prop("readonly faces: int", "").method("bevel(width: float): mesh", ""));
    let members: &[(&str, &str)] = &[
        ("geom.move(1)", "Unbekannter Name geom.move"),
        ("geom.bx(1)", "Unbekannter Name geom.bx – meinst du geom.box?"),
        ("geom.boxx(1)", "Unbekannter Name geom.boxx – meinst du geom.box?"),
        ("geom.Cone(1)", "Unbekannter Name geom.Cone – meinst du geom.cone?"),
        ("geom.box(1).bevl(1)", "mesh hat keine Methode bevl – meinst du bevel?"),
        ("geom.box(1).faxxs", "mesh hat keine Eigenschaft faxxs"),
        ("geom.box(1).fc", "mesh hat keine Eigenschaft fc"),
    ];
    for (src, want) in members {
        t.eq(src, first(ls::check(src, &g)), String::from(*want));
    }
    // removed names say what to take instead
    let a = api3();
    t.eq("removed: a member", first(ls::check("cube(1).move(1, 2, 3)", &a)), String::from("mesh.move wurde entfernt – nimm translate"));
    t.eq("removed: a member read", first(ls::check("print(cube(1).move)", &a)), String::from("mesh.move wurde entfernt – nimm translate"));
    t.eq("removed: a namespace's", first(ls::check("geom.move(1)", &a)), String::from("geom.move wurde entfernt – nimm m.translate"));
    t.eq("removed: a global name", first(ls::check("box(1)", &a)), String::from("box wurde entfernt – nimm geom.box"));
}
