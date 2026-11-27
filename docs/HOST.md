# Hosts: the interface in its second version

How an app gives scripts its object model: objects of its own, properties, methods, operators,
events, and a description of all of it for the editor and the object catalogue. This is stage S2
of [RICHTUNG.md](RICHTUNG.md) (G4.1, G4.2, G4.3, G4.5, G3 and G8.3). The first version -- functions
by name through `Host::call`, maps as objects, `HostItem` lists for the editor -- stays as it is
([LANGUAGE.md](LANGUAGE.md)); everything here is added on top, and a host that does not use it
compiles and behaves as before. What stage S3 added for the gaps the first such host (JFLib's
modeltool) found is marked (S3): forms, optional parameters and unions in signatures, binding a
call, `compile_api`, the host's type names in messages, closer suggestions and removed names; the
named arguments of the syntax pass, which reach a host's functions through its description; and
the messages in the app's language ([below](#the-language-of-the-messages-s3)).

What the script sees is in [LANGUAGE.md](LANGUAGE.md#host-objects). The checks of all of it are
the group `scripthost` of `scripttest` ([`scripthostt.rs`](../scripttest/src/scripthostt.rs)), a
host like modeltool's and JFDiagrammer's in about 400 lines. (S3) Long operations with promises
and `await` are [below](#long-operations-promises-and-await), their checks the group
`scriptawait` ([`scriptawaitt.rs`](../scripttest/src/scriptawaitt.rs)).

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
  hat keine Eigenschaft q", (S3) also the library's and the VM's: "str.split: sep muss str sein,
  nicht mesh"). It must not be a name of the language (`str`, `list`, ..); `Api::validate` says
  so. `Value::type_name()` alone says `object` (a value does not know its VM); `vm.type_of(v)` /
  `cx.type_of(v)` give the host's name.
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

## Long operations: promises and await

(S3, G4.6 of [RICHTUNG.md](RICHTUNG.md)) A file dialog, a download, a query to a server: the
native starts the work and gives the script a promise; the script writes `await` and waits for the
value, while the app runs other invocations. Callbacks (`fetch(url, fn(err, resp) { .. })`) keep
working as before; a host offers either, or both. What the script sees is in
[LANGUAGE.md](LANGUAGE.md#waiting-await); the VM's side in [`task.rs`](../script/task.rs).

```rust
use script::{Status, Value};

// the native: start the work, give the script the promise, keep its id
fn call(&mut self, cx: &mut Cx, name: &str, args: &[Value]) -> Result<Value, Fail> {
    match name {
        "ui.open_file" => {
            let p = cx.promise()?;                           // Promise { id, value }
            self.dialogs.open_file(p.id, filter(cx, args)); // the app's own, asynchronous
            Ok(p.value)                                      // `await ui.open_file(..)` waits
        }
        _ => Err(Fail::Unknown),
    }
}

// the macro: an invocation like any other
vm.call("macro", &[])?;
match run_budget(&mut vm, &mut host) {                       // run() in slices, as before
    Status::Done(v) if vm.is_waiting(v) => self.macros.push(v),  // it waits: v is its promise
    Status::Done(v) => { /* finished with v */ }
    Status::Error(e) => self.console(&e.to_string()),
    _ => {}
}

// later, between runs: the dialog closed
match chosen {
    Some(path) => { let s = vm.new_str(&path)?; vm.resolve(id, s); }
    None => { vm.resolve(id, Value::Nil); }               // or vm.reject_msg(id, "abgebrochen")
}

// each frame, while the VM is idle: the invocations whose value came go on
while let Some(p) = vm.wake() {                              // p: the promise Done gave
    match run_budget(&mut vm, &mut host) {
        Status::Done(v) if vm.is_waiting(v) => {}            // it waits again (v == p)
        Status::Done(v) => self.macro_done(p, v),
        Status::Error(e) => self.console(&e.to_string()),
        _ => {}
    }
}
```

* `cx.promise()` (inside a native) or `vm.new_promise()` (between runs) make a promise: its `id`
  and its `value` for the script (`type(p)` is `"promise"`; `vm.object(p)` gives the id too).
  It stays alive until it is settled -- the host holds it by its id, as with a pin -- whether
  the script keeps it or not.
* `vm.resolve(id, v)`, `vm.reject(id, reason)` and `vm.reject_msg(id, "Text")` settle it, between
  runs or inside a native (`cx.resolve` / `cx.reject`); `false` when no promise of that id waits
  (settled already, unknown) or the value is not valid. A rejection throws the reason at the
  script's `await`, catchable, with the await's line and column. `vm.promise_state(v)` tells
  `Waiting`, `Resolved(v)` or `Rejected(v)`.
* An invocation that awaits a promise that is not settled waits: its stack and frames leave the
  VM, `run()` returns `Status::Done(p)` with `p` the invocation's own promise (the same one at
  each of its later waits; `vm.is_waiting(p)` tells it from a promise the script returned), and
  the VM is idle -- `call`, `call_value`, `emit` and `add_script` work. No new `Status`: a host
  that makes no promises never sees any of this, and its code compiles and runs as before.
* `vm.wake()` makes the oldest invocation whose promise settled the pending one again and gives
  its promise; `run()` continues it after its `await`. When it ends, its promise settles: resolved
  with its result, rejected with the message of its error -- so a host may also hand `p` to
  another invocation that awaits it. Order: in the order the promises were settled, the
  invocations of one promise in the order they began to wait; nothing runs unless the host calls
  `wake()`. `vm.waiting()` counts the waiting invocations, `vm.ready()` those that can go on;
  `vm.cancel_waiting(p)` drops one (its promise is rejected), `vm.cancel()` drops the pending one
  as before.
* Listeners: `vm.emit` runs them as before; one that waits counts as handled (`Handled { fuel,
  error: None }`) and goes on through `wake()`. Inside a native (`cx.call`, `cx.emit`) a callback
  runs to its end and cannot wait: `await` of a promise that is not settled throws there.
* Fuel and limits (G5.5; the user's decision of 2026-11-27): `Limits::fuel` holds for each
  stretch of an invocation between two waits, so an invocation that waits often (dialogs, the
  network, one host operation per shape) is not ended only because it is long in all, while an
  endless loop without a wait still ends with `ErrorKind::Fuel`. Its count goes on over the
  waits: `vm.call_fuel()` and `cx.fuel()` count from its start, so a host that turns fuel into
  time sees one clock. The frame budget is the host's, as before, per `run()`. Splitting into
  slices of any size gives the same result, waits included.
* The collector keeps what a waiting invocation holds (its stack, its closures, its promise) and
  every promise that is not settled. `Vm: Clone` copies waiting invocations and promises with
  their ids; the host settles them in each copy it lets go on. Drops of host objects come when an
  invocation begins to wait, as when one ends.
* The debugger: a step goes with its invocation over the wait, breakpoints stop in a woken one
  as in any other, `frames()` and `locals()` show it.
* The description: a function that returns a promise is written `promise<T>` (or `promise`):
  `.func("ui.open_file(filter: str): promise<str>", "..")`; `let path = await ui.open_file(..)`
  then has the type `str` in the language services. `promise` is a name of the language, not of
  a host's type.

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

* Types are text: `int float str bool bytes list map fn nil any`, (S3) `number` (int or float), a
  described type, `list<T>` (a list of T: `for x in` and `l[i]` give T), (S3) `promise` and `promise<T>`
  (`await` gives a T), (S3) a union of those,
  `number | vec`, also in properties, values and results (`find(name: str): shape | nil`; `T |
  nil` has T's members in the editor, another union none). Anything else is shown as written.
* Parameters: `name: T`, `name: T = default`, (S3) `name?: T` (optional without a default),
  `...rest: T`; a parameter without a type is any; a function without `: R` returns nothing
  known. A parameter with `?` or a default may be left out, a rest parameter takes any number.
* (S3) Forms (overloads, as TypeScript writes them): a function or method declared several times,
  each form with its own description; `api.sigs(name)` / `ty.sigs(name)` give them in order,
  `global(name)` and `find_method(name)` the first. Two identical forms are an error of
  `validate`, two forms are not.
* Operators: `A op B: R` for `+ - * / % < <= > >=`, `-A: R` for the minus; listed under the type
  they belong to, also with the type on the right (`float * vec: vec`).
* (S3) `.removed("geom.move", "m.translate")` (on `Api` for global and namespaced names, on `Type`
  for members): a script that still uses the name hears "geom.move wurde entfernt – nimm
  m.translate" instead of a guess (a warning for members, an error for a global name, as unknown
  names are).
* (S3) `.check_calls()`: the signatures name every form a call may take, so `ls::check` and
  `compile_api` warn about a call whose number of arguments fits none ("geom.box: 4 Argumente,
  nimmt aber höchstens 3 (w, h, d)", "vec: 2 Argumente passen zu keiner Form (vec(x, y, z),
  vec(v))"). Off by default: descriptions of S2 name the longest form and leave the shorter ones
  to the text (modeltool's `m.move(v)`), and parameters without a default were never required.
* `Api::from_items(&host_items)` turns a `HostItem` list (the form before S2) into a description
  without types, so a host can move step by step.

```rust
let api = Api::new("modeltool")
    .func("vec(x: float, y: float, z?: float): vec", "Ein Vektor aus Zahlen.")
    .func("vec(v: vec | list): vec", "Ein Vektor aus einem Vektor oder einer Liste [x, y, z].")
    .removed("geom.move", "m.translate")
    .typ(Type::new("mesh", "Ein Netz.")
        .method("translate(x: float, y: float, z: float): mesh", "Verschoben um x, y, z.")
        .method("translate(v: vec | list): mesh", "Verschoben um v.")
        .method("scale(s: number | vec): mesh", "Skaliert.")
        .removed("move", "translate"))
    .check_calls();
```

### Binding a call (S3)

`api::bind(&forms, &args)` says which form a call fits and which parameter each argument fills;
a call by position reaches a host function as the script wrote it, and a host binds when it wants
to: to pick an overload, to warn about extra arguments, to read an argument by its parameter's
name. A call with named arguments the VM binds itself ([below](#named-arguments-s3)).

```rust
use script::api::{self, Arg};

fn call(&mut self, cx: &mut Cx, name: &str, args: &[Value]) -> Result<Value, Fail> {
    let tys: Vec<&str> = args.iter().map(|v| cx.type_of(*v)).collect();
    let a: Vec<Arg> = tys.iter().map(|t| Arg::pos(t)).collect();     // Arg::named(n, t) later
    let b = match api::bind(&self.api.sigs(name), &a) {
        Ok(b) => b,
        Err(why) => return Err(cx.error(&self.wrong_call(name, &why))),  // each form's Mismatch
    };
    match (name, b.form) {
        ("vec", 0) => vec3(cx, num(args, b.arg("x")), num(args, b.arg("y")), num(args, b.arg("z"))),
        ("vec", 1) => vec_of(cx, args[b.arg("v").unwrap()]),
        ..
    }
}
```

The rules, in this order (each form's first broken rule is its `Mismatch`):

1. Arguments by position fill the parameters in order; a rest parameter takes every one from its
   place on (`Binding::args("cuts")`). More than the parameters: `TooMany { most }`.
2. A named argument fills the parameter of its name: none of that name, or a rest parameter:
   `NoParam`; a parameter that already has an argument (by position or named twice): `Twice`.
3. A parameter that may not be left out (no `?`, no default, not rest) without an argument:
   `Missing`.
4. Each argument's type fits its parameter's (`api::fits`): the same type, `any` or not known
   (`""`) on either side, int for float and number, float for number, a list for `list<T>`, a
   union when one of its types fits; `nil` fits an optional parameter (a left-out argument is nil).
   Else `Type`.

The first form that fits, in the order the host declared them, wins (as in TypeScript), so a
host lists the more specific forms first. `Sig::arity()` gives the fewest and the most arguments
of a form, `api::count_warning(shown, &forms, n)` the warning `ls::check` gives for a number
that fits none: a host's own run-time warning about extra arguments can use either.

### Named arguments (S3)

A script may name arguments: `box(1, d: 3)`, `m.bevel(width: 0.1)`, `geom.cyl(h: 2, r: 1)`
([LANGUAGE.md](LANGUAGE.md#the-syntax-of-s3)). The host's functions and methods get them through
its description, and **`Host::call` and `Host::method` keep getting values by position**, so an
existing host needs no change:

* A program of `script::compile_api(name, src, &api)` keeps its description (also for the
  modules it imports and the scripts of `add_script`). At a call with named arguments the VM
  takes the forms of the function (`api.sigs("box")`) or of the method of the object's type
  (`api.get_type("mesh").sigs("bevel")`), binds the arguments with `api::bind_lenient` -- the
  first form that fits (`bind`, the types as `type(x)` gives them), else the first that takes
  them by position and name alone (`Sig::bind_names`: a parameter left out and the types are the
  host's matter, as for a call by position) -- and calls the host with the values in that form's
  parameter order: `nil` for a parameter left out before the last one given, nothing after it, a
  rest parameter's values in their place. With `box(w: float, h: float = 1, d?: float)`,
  `box(1, d: 3)` arrives as `box(1, nil, 3)`, `box(w: 2)` as `box(2)`, `box(d: 1, w: 2)` as
  `box(2, nil, 1)`; with the forms `vec(x, y, z?)` and `vec(v: list)`, `vec(v: l)` arrives as
  `vec(l)`. A host that binds positionally (above) picks the same form for these.
* A name no form takes is an error with the call's position, the same text as `api::named_message`
  gives: at compile time where the call names a described function ("box hat keinen Parameter
  dd", "box: der Parameter w hat schon ein Argument"), a warning of `ls::check` / `compile_api`
  for a method of a type the services know ("mesh.bevel hat keinen Parameter widht – meinst du
  width?"), else a catchable error at run time.
* Without a description (`compile`, `compile_with`) a named argument to a host function is an
  error: at compile time where the call names the host's function (`box(w: 1)`: "box: benannte
  Argumente gehen an Funktionen des Hosts nur mit seiner Beschreibung (script::Api,
  compile_api)"), at run time where it comes as a value or is a host object's method. A map a
  host set with `set_global` (`config`) counts as the host's name there.
* `obj.on(..)` and `obj.off(..)` take `event:` and `f:`; the library takes the names of its
  descriptions; a script function its parameters' names.
* What a host must change: nothing. The parameter names of its description are part of its
  interface now (G9.3: English) and should not change lightly. A host does not see which
  arguments were named -- the VM hands it the values in parameter order only; handing it the
  names as well would need a second calling convention, so that is not in S3.

### Compiling with the description (S3)

`script::compile_api(name, src, &api)` is `compile_with(name, src, &api.names())` plus the
warnings of the language services on the same parse, so a host need not parse twice:

```rust
let c = script::compile_api("haus.jfs", src, &api)?;   // Err: the errors, as compile_with's
for w in &c.warnings { report(w) }                      // Diagnostic: file, line, col, msg
let mut vm = Vm::new(&c.program, limits);
// .. run; the modules it imported (Host::load) were compiled the same way:
for w in vm.take_warnings() { report(w) }               // a module's with its path, Cx::warn's
```

* The warnings are those of `ls::check(src, &api)` (with the file): members a known host type
  does not have, removed names, a division of two ints, a top-level `let` read before it ran,
  with `check_calls` the number of arguments. Unknown names are errors, as with `compile_with`.
* A module imported by such a program, and a script of `vm.add_script`, is compiled with the
  same description; its warnings join the VM's own (`Cx::warn`'s) in the order they came, as
  `Warning { file, line, col, msg }` with the module's path, at most `MAX_WARNINGS` between two
  `take_warnings`. A program of `compile_with` gives none, as before.
* modeltool parses each script and module twice today (`ls::check` and `compile_with`); with
  `compile_api` its script's warnings come with the program and its modules' through
  `take_warnings`, each module's at the time of its import among the host's own (today it lists
  all of the language services' first).

### In the editor

```rust
use script::ls::{self, Context};

let cx = Context::new(&api).with_loader(&load);             // or Context { api, host: &items, load }
let c = ls::complete(src, caret, &cx);                     // after "m." the members of a mesh
let h = ls::hover(src, at, &cx);                           // "mesh.bevel(width: float, ..): mesh"
let s = ls::signature(src, caret, &cx);                    // parameters with types and defaults
let d = ls::check(src, &cx);                               // syntax, names, unknown members
```

(S3) With forms, `signature` gives every form (`s.forms`, each with its label, parameter ranges,
active parameter and description) and in `label`, `params`, `active`, `doc` the one at `s.form`:
the first with room for the argument at the caret whose finished arguments fit where their types
are known (`put("a", |` picks `put(s: str, ..)`), else the first with room, else the first.
`hover` shows each form on a line of its own, completion one item per name with its first form.
A call's type is the result all forms its arguments may bind to share: `load("a")` is a mesh
when `load(path: str): mesh` and `load(id: int): vec`, `load(x)` of an unknown `x` is not known.

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
a host namespace that is not there, (S3) a removed name with what to take instead, with
`check_calls` a call whose number of arguments fits no form; unknown names are errors, as
`compile_with` makes them.

(S3) "meinst du x?" names only a candidate that is really close (`check.rs`): inserted, deleted,
changed characters and two swapped neighbours count one each, a difference in case nothing; at
most 1 for names of 3 to 5 characters, 2 for 6 to 8, 3 for longer ones, and from 2 on the first
letter must be the same. A member is compared without its namespace and only with the members of
that namespace or type; names of 1 or 2 characters get none, a member of 2 that lacks a letter
does (`vec.ad` -> `vec.add`). Kept: `prnt`, `pirnt`, `PRINT` -> `print`, `wran` -> `warn`,
`facs` -> `faces`, `cilynder` -> `cylinder`; gone: `geom.move` -> `geom.cone` (2 changes in 4
characters), `normal` -> `format`, `loger` -> `log`.

### The object catalogue

`api.markdown()` is the app's reference as a page (globals, then one section per type with its
properties, methods, events and operators); `api.catalogue()` is the same as a tree of `Entry {
kind, name, label, doc, children }` for a UI. (S3) The forms of a function or method are an entry
(a line) each, in their order; optional parameters show as `z?: float`, unions as written. The
catalogue window itself belongs to the later crate `jfscript-ide` (G8.1).

```text
### vec (Werttyp: == vergleicht die Werte)

Ein Vektor.

Eigenschaften:

- `readonly x: float` – Die x-Komponente.

Operatoren:

- `vec + vec: vec` – Die Summe.
```

(S3) The headings and the words JFScript adds ("Eigenschaften", `EntryKind::word`) follow the
language of the messages ([below](#the-language-of-the-messages-s3)); the names and the host's
texts stay as the host wrote them.

## The language of the messages (S3)

JFScript's messages and the texts of its language services come in the app's language (G9.1,
G9.2 of [RICHTUNG.md](RICHTUNG.md)): German, the language they were written in, and English; more
languages are a column each in a table. Without a word from the host everything stays German,
byte for byte as before, so existing hosts and their tests see no change.

```rust
// once at start and whenever the user switches (a JFLib app: its UI language)
let code = script::set_language(&ui::i18n::language_code());   // "de", "en"; "fr" -> "en"
assert_eq!(script::language(), code);
script::languages();                                          // ["de", "en"]
```

* `script::set_language(code)` takes "de", "en" or a BCP 47 tag ("en-GB" takes "en"). A code the
  table has no column for takes English, as JFLib's `ui::i18n::set_language_code` does for a UI
  language its catalogue lacks; "" is German again. It gives the code it took.
* The setting holds for the whole process, as JFLib's UI language does: messages are made deep in
  the lexer, the parser, the checks, the VM and the library, where no host value reaches. It is an
  atomic, so threads may compile and run while it changes; each message takes the language of the
  moment it is made (a `Diagnostic` keeps its text, a VM's `limit_text` is made in `Vm::new`). A
  process that wants two languages at once (two windows, two users) is not served; JFLib's apps
  have one UI language.
* What follows it: compile errors and warnings (`Diagnostic::msg`, and Display "3:5: error: .."
  for "3:5: Fehler: .."), run-time errors (`Error::msg`, Display with its trace "in the main
  program"), the message a script catches with `try`/`catch` for an error of the VM or the
  library, the warnings of `compile_api` and of modules through `take_warnings`, `Api::validate`,
  the texts of `ls::complete`, `hover`, `signature`, `check` and `rename`, `ls::library_doc`,
  `SymKind::word`, `EntryKind::word`, the headings of `Api::markdown`, `Vm::limit_text` and the
  default reason of `Host::load`.
* What does not (G9.3, and so a script gives the same result under every host): keywords, names
  of the library and of the host, parameter names, type names (`int`, `str`, `mesh`), the text of
  values (`str(x)`, `print`, `${..}`; a promise prints as `<promise 3 ausstehend>` in every
  language), and every text of the host: `cx.error(..)`, `cx.warn(..)`, `Fail::Fatal`, the reason
  of `Host::load`, a value a script throws itself, the descriptions of `script::Api` and
  `HostItem`. Fuel, heap and the other limits do not depend on the language either.
* A caught message changes with the language: a script that wants to tell errors apart throws
  values of its own (`throw {code: 3}`) instead of comparing texts.
* Placeholders in the table (`script/locales/messages.tsv`, the form of JFLib's `ui.tsv`): `{}` in
  order, `{0}` `{1}` by position when a language needs another word order, `{name}` by name;
  `{{` and `}}` are braces. A language without its own cell for a text shows the English one.

The host's own descriptions are the host's to translate: JFScript never translates them (G9.2).
A host that writes them in its UI language builds the `Api` with those texts and builds it again
when the language changes. One that keeps them in one language (German, as its source texts) and
its translations in its catalogue (JFLib's `locales/ui.tsv`, each description a row) puts them
through that catalogue in one step:

```rust
let api = Api::new("modeltool")
    .func("vec(x: float, y: float, z: float = 0): vec", "Ein neuer Vektor.")
    .typ(Type::new("vec", "Ein Vektor.").prop("readonly x: float", "Die x-Komponente."))
    .map_docs(|d| String::from(catalog.tr(d)));    // every description: app, globals, members
```

`HostItem` lists (the form before S2) hold `&'static str`; a host makes the list with the texts of
its language (`ui::i18n::t(..)`). Names, types and signatures stay as written, so a script and its
completion look the same in every language.

The checks are the group `scriptmsg` of `scripttest`
([`scriptmsgt.rs`](../scripttest/src/scriptmsgt.rs)): every key of the sources has a row with an
English cell and the same placeholders, every row a key, a literal of `script/` that reads like
German must be a message (see `script/msg.rs`), then messages, warnings and the services' texts in
English, and the German ones unchanged after a switch.
