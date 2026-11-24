# Hosts: the interface in its second version

How an app gives scripts its object model: objects of its own, properties, methods, operators,
events, and a description of all of it for the editor and the object catalogue. This is stage S2
of [RICHTUNG.md](RICHTUNG.md) (G4.1, G4.2, G4.3, G4.5, G3 and G8.3). The first version -- functions
by name through `Host::call`, maps as objects, `HostItem` lists for the editor -- stays as it is
([LANGUAGE.md](LANGUAGE.md)); everything here is added on top, and a host that does not use it
compiles and behaves as before.

What the script sees is in [LANGUAGE.md](LANGUAGE.md#host-objects). The checks of all of it are
the group `scripthost` of `scripttest` ([`scripthostt.rs`](../scripttest/src/scripthostt.rs)), a
host like modeltool's and JFDiagrammer's in about 400 lines.

## Objects

A host object is a value the script cannot open: `Value::Host`, with the host's type name and an
id. There are two kinds.

```rust
// an object with identity: an element, a mesh, a shape, a page
let el = cx.new_object("element", 42)?;        // the same value while the script holds it
// a value type: a vector, a point, a color -- the data lives in the VM
let v = cx.new_value("vec", &[Value::Float(1.0), Value::Float(2.0), Value::Float(0.0)])?;

let o = cx.object(v).unwrap();                  // Object { ty: "vec", id: 0, value: v }
let d = cx.data(v).unwrap();                    // &[Value]: the three floats
```

* **Objects with identity** (`new_object(ty, id)`): the VM keeps one object per `(ty, id)` while
  it lives, so `doc.find("btn") == doc.find("btn")` and `==` compares identity. When the collector
  frees it -- no script holds it any more -- the host hears of it in `Host::drop(Object)` and may
  free what belongs to it (a mesh's vertices). A later `new_object` with the same id makes a new
  object; a drop the host has not been told of yet is then forgotten, so `drop` always means
  "there is no object of this id in the VM now".
* **Value types** (`new_value(ty, &data)`): a new object each time, its data copied into the VM
  (any values, traced by the collector) and never changed; `==` compares the type and the data
  (numbers as `==` does), `print` shows `vec(1.0, 2.0, 0.0)`. The host needs no table of them and
  hears of no drops.
* The type is a `&'static str` the host chooses: what `type(x)` gives and what errors name ("vec
  hat keine Eigenschaft q"). It must not be a name of the language (`str`, `list`, ..);
  `Api::validate` says so.
* Objects the host makes between runs are valid until the next run, as `new_str`'s are; values it
  keeps longer it pins (`vm.pin`). `vm.find_object(ty, id)` gives the live object of an id
  without making one, `vm.take_dropped()` the drops at once (after `vm.collect()`, say).
* When drops arrive: before the next call of the host (a native or any of the hooks below) or
  when the invocation ends -- at the same point however a run is split into slices, so a host
  that reuses ids stays deterministic. A clone of the VM holds the same objects and reports its
  own drops; a host that clones VMs counts references.

## Properties, methods, operators, text

All of these are methods of `Host` with defaults (`Err(Fail::Unknown)`), asked only for host
objects; maps keep their fields, the built-in types their methods.

```rust
impl Host for Model {
    fn call(&mut self, cx: &mut Cx, name: &str, args: &[Value]) -> Result<Value, Fail> {
        match name {
            "vec" => vec3(cx, f(args, 0), f(args, 1), f(args, 2)),
            "cube" => { let id = self.add_mesh(Mesh::cube()); cx.new_object("mesh", id) }
            _ => Err(Fail::Unknown),
        }
    }
    // shape.text, v.x
    fn get(&mut self, cx: &mut Cx, obj: Object, name: &str) -> Result<Value, Fail> {
        match (obj.ty, name) {
            ("vec", "x") => Ok(cx.data(obj.value).unwrap()[0]),
            ("mesh", "faces") => Ok(Value::Int(self.mesh(obj.id).faces as i64)),
            ("shape", "text") => cx.new_str(&self.shape(obj.id).text),
            _ => Err(Fail::Unknown),                         // "mesh hat keine Eigenschaft q"
        }
    }
    // shape.text = "x", also shape.text += "!" (get, the operator, set)
    fn set(&mut self, cx: &mut Cx, obj: Object, name: &str, v: Value) -> Result<(), Fail> {
        match (obj.ty, name) {
            ("shape", "text") => { self.shape_mut(obj.id).text = text_of(cx, v)?; Ok(()) }
            ("shape", "id") => Err(cx.error("shape.id ist nur lesbar")),
            _ => Err(Fail::Unknown),
        }
    }
    // m.bevel(0.1); the default asks Host::call("mesh.bevel", [m, 0.1]) as before S2
    fn method(&mut self, cx: &mut Cx, obj: Object, name: &str, args: &[Value]) -> Result<Value, Fail> {
        match (obj.ty, name) {
            ("mesh", "bevel") => { let id = self.bevel(obj.id, f(args, 0)); cx.new_object("mesh", id) }
            _ => Err(Fail::Unknown),                         // "mesh hat keine Methode x"
        }
    }
    // a + b, a * 2, 2 * a, -a (b nil), a < b (a bool); == never comes here
    fn op(&mut self, cx: &mut Cx, op: Operator, a: Value, b: Value) -> Result<Value, Fail> {
        match (op, xyz(cx, a), xyz(cx, b)) {
            (Operator::Add, Some(p), Some(q)) => vec3(cx, add(p.0, q.0), add(p.1, q.1), add(p.2, q.2)),
            (Operator::Neg, Some(p), None) => vec3(cx, neg(p.0), neg(p.1), neg(p.2)),
            _ => Err(Fail::Unknown),                         // "Der Operator + passt nicht zu vec und str"
        }
    }
    // print(x), str(x), "${x}"; None: "<mesh 3>" / "vec(1.0, 2.0, 0.0)"
    fn to_str(&self, vm: &Vm, obj: Object) -> Option<String> {
        (obj.ty == "mesh").then(|| format!("mesh({} Flächen)", self.mesh(obj.id).faces))
    }
    fn drop(&mut self, obj: Object) {
        if obj.ty == "mesh" { self.free_mesh(obj.id) }
    }
}
```

* A hook runs like a native: what it allocates stays alive until it returns, its value is checked,
  `cx.spend(n)` charges fuel and stops at `Limits::fuel`, `cx.yield_now()` ends the run after it,
  `cx.call(self, f, &args)` calls back, `cx.position()` / `cx.warn(..)` know the script's place.
  The instruction itself costs one fuel; a property that does real work charges for it.
* Errors: `Err(cx.error("…"))` throws a str the script can catch, at the position of the property,
  the operator or the method, with the stack trace; `Fail::Fatal` ends the invocation;
  `Fail::Unknown` gives the VM's message with the type's name.
* Operators: `+ - * / %`, `< <= > >=` and the unary minus reach `Host::op` when either side is a
  host object, also with the host object on the right (`2 * v`). `==` and `!=` stay in the VM:
  identity for objects, type and data for value types -- so `[v].contains(w)` and
  `assert_eq` agree with `==`. There are no operators for maps or script values (G4.3).
* `cx.display_with(v, self)` (and `vm.display_with`) give a host's own functions (a `log`) the
  text `print` shows; `vm.display(v)` and the debugger's views use the default text.
* Determinism (G2) is the host's: operators of a vector should compute with `script::num`, as
  the language does, or with hardware floats only where G2 allows it.
* The speed of code without host objects does not change: the instructions find a host object
  in their error path and leave the loop for the hook (`scripttest --bench vm` measures it).

## Events

One form for every app (G4.5): `obj.on("type", fn(e) { .. })`, `obj.off("type", f)` and
`obj.off("type")`. The VM keeps the functions; the host says which events an object has and
fires them.

```rust
// which events an object has (and a place to start producing them, or to refuse: a limit)
fn listen(&mut self, cx: &mut Cx, obj: Object, event: &str, on: bool) -> Result<(), Fail> {
    match (obj.ty, event) {
        ("shape", "click" | "changed") => Ok(()),
        _ => Err(Fail::Unknown),                             // "shape hat kein Ereignis x"
    }
}
```

Firing an event between runs, bubbling as the host decides (shape -> page -> document), each
listener an invocation of its own with fresh fuel and the VM's limits:

```rust
fn click(&mut self, vm: &mut Vm, shape: u64) {
    let ev = self.new_event("click", shape);                 // the host's own record
    let e = vm.new_object("event", ev).unwrap();             // the event object: a host object
    let keep = vm.pin(e);                                    // alive while the way goes up
    for (ty, id) in self.path_up(shape) {                    // [("shape", 7), ("page", 1), ("document", 0)]
        let target = vm.new_object(ty, id).unwrap();
        for h in vm.emit(self, target, "click", &[vm.pinned(keep)]).unwrap() {
            if let Some(err) = h.error { self.console(&err.to_string()) }   // fuel: h.fuel
        }
        if self.event(ev).stopped { break }                  // e.stop() set it
    }
    vm.unpin(keep);
}
```

* `vm.emit(host, obj, event, &args)` calls the listeners of `event` on `obj` in the order they
  came; one that an earlier listener removed does not run; an error ends only its listener
  (`Handled { fuel, error }`). It needs an idle VM and steps over a debugger's pauses -- a host
  with a debugger takes `vm.listeners(obj, event)` and runs each through `call_value` itself.
* Inside a native (`el.click()` of a script): `cx.emit(self, obj, "click", &[e])` runs them to
  their end through `cx.call`, in the fuel of the current invocation.
* The listeners keep their object alive: a shape with a listener is never dropped. When the host
  deletes the shape, it calls `vm.remove_listeners(obj)` (or `cx.remove_listeners`); then the
  object can go, and `Host::drop` follows.
* The same function twice for one event is one listener (as `addEventListener`). Listeners count
  toward `Limits::heap`; `vm.listener_count()` and `vm.has_listeners("click")` let a host look
  before it makes an event (JFNetworker's pages do so).
* JFNetworker's page scripts keep their handlers themselves (`pagescript.rs`); moving them to
  these listeners is possible and optional (G8.5).

## Descriptions

A host describes its object model as data (`script::Api`, [`api.rs`](../script/api.rs)). The
language services take it for completion, hover, signature help and diagnostics, and the object
catalogue is made from it (G3, G8.3). Names are English (G9.3), the texts in the app's language
(G9.2); signatures are written as in TypeScript.

```rust
use script::api::{Api, Type};

let api = Api::new("modeltool")
    .doc("Modelle aus Regeln und Bausteinen.")
    .func("vec(x: float, y: float, z: float = 0): vec", "Ein neuer Vektor.")
    .func("cube(size: float = 1): mesh", "Ein Würfel.")
    .value("doc: document", "Das offene Dokument.")
    .namespace("log", "Das Protokoll.")
    .func("log.warn(...values): nil", "Eine Warnung.")
    .typ(Type::new("vec", "Ein Vektor.").value()
        .prop("readonly x: float", "Die x-Komponente.")
        .method("length(): float", "Die Länge.")
        .op("vec + vec: vec", "Die Summe.")
        .op("float * vec: vec", "Gestreckt.")
        .op("-vec: vec", "Umgekehrt."))
    .typ(Type::new("mesh", "Ein Netz.")
        .prop("readonly faces: int", "Die Zahl der Flächen.")
        .method("bevel(width: float, segments: int = 1): mesh", "Mit abgeschrägten Kanten."))
    .typ(Type::new("shape", "Eine Form auf der Seite.")
        .prop("text: str", "Der Text.")
        .prop("readonly page: page", "Die Seite.")
        .event("click: event", "Ein Klick; steigt zur Seite und zum Dokument auf."));
assert!(api.validate().is_empty());                         // unknown types, names twice ..
let prog = script::compile_with("model.jfs", src, &api.names())?;   // its names, strict
```

* Types are text: `int float str bool bytes list map fn nil any`, a described type, `list<T>`
  (a list of T: `for x in` and `l[i]` give T). Anything else is shown as written.
* Parameters: `name: T`, `name: T = default`, `...rest: T`; a parameter without a type is any;
  a function without `: R` returns nothing known.
* Operators: `A op B: R` for `+ - * / % < <= > >=`, `-A: R` for the minus; listed under the type
  they belong to, also with the type on the right (`float * vec: vec`).
* `Api::from_items(&host_items)` turns a `HostItem` list (the form before S2) into a description
  without types, so a host can move step by step.

### In the editor

```rust
use script::ls::{self, Context};

let cx = Context::new(&api).with_loader(&load);             // or Context { api, host: &items, load }
let c = ls::complete(src, caret, &cx);                     // after "m." the members of a mesh
let h = ls::hover(src, at, &cx);                           // "mesh.bevel(width: float, ..): mesh"
let s = ls::signature(src, caret, &cx);                    // parameters with types and defaults
let d = ls::check(src, &cx);                               // syntax, names, unknown members
```

`complete`, `hover`, `signature` and `definition` take a `&CompleteCx` as before or a `&Context`
(or `&api`). The type of the code before a dot comes from the description and the script, simply
and honestly (no type annotations in scripts yet, G3 b is later):

* a host value (`doc`), a host function's result (`cube(1)`), a property (`doc.page`), a method's
  result (`m.bevel(1)`), a chain of them (`doc.page.shapes[0].`, also while the line does not
  parse), `l[i]` of a `list<T>`, the operators of the description (`v * 2 + w` is a vec)
* a `let` whose every assignment in the file has the same type, a `for` variable over a
  `list<T>`, the parameter of a function given to `obj.on("click", fn(e) { .. })` (the event's
  type)
* anything else -- parameters, a variable assigned different types, a value of another file --
  is not known, and the services say what they said before.

`ls::check` warns about a property or method the known type does not have (with the closest
name: "mesh hat keine Eigenschaft facs – meinst du faces?"), a read-only property assigned, a
method read as a property and the other way round, an event the type does not have, a member of
a host namespace that is not there; unknown names are errors, as `compile_with` makes them.

### The object catalogue

`api.markdown()` is the app's reference as a page (globals, then one section per type with its
properties, methods, events and operators); `api.catalogue()` is the same as a tree of `Entry {
kind, name, label, doc, children }` for a UI. The catalogue window itself belongs to the later
crate `jfscript-ide` (G8.1).

```text
### vec (Werttyp: == vergleicht die Werte)

Ein Vektor.

Eigenschaften:

- `readonly x: float` – Die x-Komponente.

Operatoren:

- `vec + vec: vec` – Die Summe.
```
