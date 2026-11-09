# The language JFScript

This page came from section 29 of JFLib's guide. The crate is `jfscript` in [`script/`](../script/lib.rs); programs name it `script` (`script::compile`).

## Compiler and VM

[`script/`](../script/lib.rs) runs JFNetworker's `.jfs` scripts: `compile` turns the
source into bytecode, a `Vm` runs it with fuel (virtual CPU time), a heap of its own with a
mark-and-sweep collector and hard limits. Everything is deterministic: no clock, no randomness,
float arithmetic in software (`script::num`), so a script gives the same bits on every target,
and a run split into slices of any size gives the same result as one uninterrupted run. The
syntax and the editor helpers (`check`, `tokens`, `outline`) are in the crate comment.

```rust
use script::{Cx, Fail, Host, Limits, Status, Value, Vm};

let prog = match script::compile_named("server.jfs", src) {
    Ok(p) => p,
    Err(diags) => return show(&diags),                 // "3:5: Fehler: …", warnings left out
};
let mut vm = Vm::new(&prog, Limits::default());       // the top-level code is pending
let mut host = MyHost::default();
loop {
    match vm.run(&mut host, 10_000) {                 // at most 10 000 fuel per slice
        Status::Done(_) => break,
        Status::Yielded => continue,                  // a simulator would let time pass here
        Status::Error(e) => { log(&e.to_string()); break }   // message, position, stack trace
    }
}
let req = vm.new_str("GET /")?;
vm.call("on_request", &[req])?;                       // a global function, then run() again
let v = vm.global("hits");                            // read and set globals
```

A host gives scripts their functions by name. The standard library (below) answers first, so
names such as `str.split` or `list.map` never reach the host:

```rust
#[derive(Default)]
struct MyHost { handlers: Vec<script::Pin> }

impl Host for MyHost {
    fn call(&mut self, cx: &mut Cx, name: &str, args: &[Value]) -> Result<Value, Fail> {
        match name {
            "log" => { emit(&cx.display(args[0])); Ok(Value::Nil) }      // log(x)
            "http.serve" => { self.handlers.push(cx.pin(args[1])); Ok(Value::Nil) }
            "map.json" => {                                             // res.json(200, v)
                let body = cx.json_encode(args[2], 0)?;                 // like json.encode
                send(&body);
                Ok(Value::Nil)
            }
            "list.each" => {                                            // [1, 2].each(f)
                let items = cx.list(args[0]).map(|l| l.to_vec()).unwrap_or_default();
                for x in &items {
                    cx.keep(*x);                                        // f may empty the list
                }
                for x in items {
                    cx.spend(1)?;                                       // fuel by the work
                    cx.call(self, args[1], &[x])?;                      // f runs to its end
                }
                Ok(Value::Nil)
            }
            "fail" => Err(cx.error("kaputt")),                          // a catchable exception
            _ => Err(Fail::Unknown),                                    // "Unbekannte Funktion …"
        }
    }
    fn knows(&self, name: &str) -> bool { name == "log" }              // `let f = log` works
}
```

* Names: a name the script does not declare comes from the host. `print`, `len`, `type`,
  `str`, `int`, `float` are built in (`print` writes to `vm.output()` / `take_output()`,
  capped by `Limits::output`), the standard library comes next. `f(..)` and `ns.f(..)` call
  `Host::call` with `"f"` / `"ns.f"`; `obj.m(..)` calls a function stored in a map under `m`,
  then the built-ins and the library, then `Host::call("list.m", [obj, ..])` (the type name
  first, the object as the first argument). `vm.set_global("config", map)` gives a host name a
  value, which then wins: `config.port`, or `math.sqrt(2.0)` when `math` is a map holding
  `vm.native("math.sqrt")`.
* Natives: `cx.spend(n)?` adds fuel for the work done and ends the invocation once
  `Limits::fuel` is used up (call it as a long job goes; `cx.charge(n)` only adds),
  `cx.yield_now()` ends the run after the call (`Status::Yielded`, the next `run` continues),
  `cx.call(self, f, &args)` runs a callback to its end (no yield in between, nested at most 20
  deep; an exception that leaves it keeps its position and stack trace). Objects a native
  allocates stay alive until it returns; values it only holds while calling back need
  `cx.keep(v)`. `cx.json_encode(v, indent)` / `cx.json_decode(text)` (and
  `vm.json_encode` / `vm.json_decode` between runs) work like `json.encode` / `json.decode`.
  `cx.fuel()` is the fuel of the invocation so far, over all its runs and this call included:
  a host that turns fuel into time knows the moment a native is called at (JFNetworker's
  scripts send a packet then: `apps/JFNetworker/crates/jfn_sim/scriptsvc.rs`).
* Values (`Value`, 16 bytes) refer to the VM's heap. A value the host keeps beyond the next
  `run` must be pinned (`pin` / `pinned` / `unpin`), e.g. a request handler; `valid(v)` tells
  a stale one, and the API returns `None` or an error instead of crashing on it. The host's own
  allocations between runs can collect too (a callback that allocates nothing itself must not
  fill the heap with its arguments): what the host made since the last `run` stays until the next
  one, a value it took out of a pin or a map only while the pin or the map still holds it.
* Fuel: one per instruction, more for copying (string and list concatenation, literals, map
  iteration). `run(host, fuel)` stops before the next instruction once `fuel` is used (it may
  use a little more: `last_fuel()`). `Limits::fuel` caps one invocation over all its runs, so
  an endless loop ends with `ErrorKind::Fuel` ("Rechenzeit aufgebraucht …"). JFNetworker turns
  fuel into simulated CPU time.
* Limits (`Limits`): heap bytes (accounted the same way on every target), stack values, call
  depth, fuel per invocation, print output. Crossing one ends the invocation at once; scripts
  cannot catch it. Runtime errors (`1 / 0`, a bad index, wrong types, unknown functions) throw
  a `str` with a German message that `try`/`catch` can catch; uncaught, they end the
  invocation with an `Error` (`kind`, `msg`, `line`, `col`, `trace`). The VM stays usable:
  globals keep their values, `call` works again; `cancel()` drops a pending invocation.
* `Vm: Clone + Send`: a clone is a full copy (heap included) that runs on independently, also
  from the middle of a yielded invocation; a `Program` is shared (`Arc`) between VMs.
* Semantics for script writers: `nil` and `false` are falsy, everything else is truthy; `int`
  is i64 with wrapping `+ - *`, `/` and `%` truncate and throw on 0; an `int` meeting a `float`
  becomes a `float`; `==` compares numbers exactly across types, `str` and `bytes` by content,
  lists, maps and functions by identity; maps keep insertion order, keys are `str`, `int` or
  `bool`, a missing key reads `nil`; `for k, v in map` visits the keys present at its start;
  missing arguments are `nil`, extra ones are dropped; closures capture variables (a loop
  variable afresh each round).
* Testing: `coretest --only script` (parser), `coretest --only scriptvm` (compiler, VM,
  collector, fuel, yield at every instruction against an uninterrupted run, soft float against
  the hardware) and `coretest --only scriptlib` (the standard library); `program.disasm()`
  prints the bytecode.

**The standard library** ([`stdlib.rs`](../script/stdlib.rs) and its neighbours) is part of
every VM, whatever the host. Methods take the value first, so `s.split(",")` and
`str.split(s, ",")` are the same call; a map's own field of the same name wins over a map
method (`map.keys(m)` always reaches the library), and a script may shadow a module with a
variable (`let list = []`). `script::library()` lists every name for editors.

| Module | Functions and constants |
|---|---|
| built in | `print(..)`, `len(x)`, `type(x)` (`"nil" "bool" "int" "float" "str" "bytes" "list" "map" "fn"`), `str(x)`, `int(x)` (truncates floats, parses `"42"`, `"0x1F"`, `"-0b11"`, throws otherwise), `float(x)` |
| `str` | `s.len()` `s.slice(start, end)` `s.split(sep, n)` `sep.join(list)` `s.trim(chars)` `s.trim_start(chars)` `s.trim_end(chars)` `s.find(sub, from)` `s.find_last(sub)` `s.contains(sub)` `s.starts_with(p)` `s.ends_with(p)` `s.count(sub)` `s.replace(from, to, n)` `s.upper()` `s.lower()` `s.repeat(n)` `s.chars()` `s.bytes()` `s.code(i)` `str.from_code(n, ..)` `s.format(args..)` `s.hash()` |
| `list` | `l.len()` `l.push(x, ..)` `l.pop()` `l.insert(i, x)` `l.remove(i)` `l.slice(start, end)` `l.reverse()` `l.sort(cmp)` `l.map(f)` `l.filter(f)` `l.reduce(f, init)` `l.contains(x)` `l.index(x)` `l.join(sep)` |
| `map` | `m.len()` `m.keys()` `m.values()` `m.entries()` `m.has(k)` `m.get(k, default)` `m.delete(k)` `m.merge(other, ..)` |
| `bytes` | `bytes()` `bytes("text")` (UTF-8) `bytes([1, 2])` `bytes(1, 2)` `b.len()` `b.slice(start, end)` `b.hex()` `bytes.from_hex(hex)` `b.text(lossy)` `b.hash()`; also `b[i]`, `b + b`, `for x in b` |
| `math` | `abs` `min` `max` `clamp` `floor` `ceil` `round` `trunc` `sqrt` `pow` `exp` `log` `log2` `log10` `sin` `cos` `is_nan` `is_finite`; `math.pi` `math.e` `math.tau` `math.inf` `math.nan` |
| `json` | `json.encode(v, indent)` `json.decode(text)` |
| `int`, `float` | `int.parse(s, base)` `float.parse(s)` (`nil` when `s` is no number), `int.max` `int.min` |

```js
// a request path and a header
let parts = "/users/42/posts".split("/").filter(fn(p) { return len(p) > 0 })
let id = int.parse(parts[1]) ?? 0                    // 42; nil for "abc", so ?? gives 0
let kv = "Content-Type:  text/html".split(":", 2)    // ["Content-Type", "  text/html"]
print(kv[0].lower(), kv[1].trim())                   // content-type text/html

// records sorted stably by a comparison (a number, or a bool: true puts a first)
let jobs = [{n: "b", prio: 2}, {n: "a", prio: 1}, {n: "c", prio: 2}]
jobs.sort(fn(x, y) { return x.prio - y.prio })       // a, b, c
print(jobs.map(fn(j) { return j.n }).join(", "))     // a, b, c

// JSON keeps the order of maps; ints stay ints, floats floats
let t = json.encode({id: 7, tags: ["x"], load: 0.5}) // {"id":7,"tags":["x"],"load":0.5}
let v = json.decode(t)
print("{:>4}|{:.2}|{:#x}".format(v.id, v.load, 255)) //    7|0.50|0xff

// math in software: the same bits on every machine
let backoff = math.min(30.0, 0.1 * math.pow(2, 5))   // 3.2
let load = 100 + 50 * math.sin(2 * math.pi * 3 / 24) // a daily curve
let shard = "user:42".hash() % 8                     // FNV-1a, stable everywhere
```

* Strings: lengths and indexes count characters (`"Größe".len()` is 5, `.find("ö")` 2);
  `slice` takes negative indexes from the end and clamps the others; `find` answers `nil`
  when there is no match; `split()` without a separator splits at runs of white space,
  `split("")` into characters, `n` caps the parts (the last keeps the rest); `upper` / `lower`
  map Unicode fully (`"straße".upper()` is `"STRASSE"`); `hash()` is FNV-1a (32 bits) of the
  UTF-8 bytes, for hash rings and shards.
* `format`: `{}` the next argument, `{0}` one by position, `{name}` a field of the first
  argument (a map), `{{` and `}}` for braces; after a colon
  `[[fill]align][+][#][0][width][.precision][type]` with align `<` `>` `^`, type `x X b o d`
  (ints), `f e` (numbers), `s`. A precision rounds the exact value (ties to even:
  `"{:.1}".format(0.25)` is `0.2`); width at most 10 000, precision at most 100.
* Lists: `insert`, `remove`, `reverse` and `sort` change the list (`reverse` / `sort` return
  it), the others make a new one; `insert` takes `0..=len`, `remove` `0..len`. `map` /
  `filter` call `f(item)` with the item alone (so `l.map(str.trim)` works) on the items the
  list had at the call; `reduce` without `init` starts with the first item. `sort` is a stable
  merge sort of its own (the same comparisons on every target): without `cmp` it orders
  numbers by value (ints against floats exactly, `nan` last), strings and bytes by their
  bytes, and throws for other or mixed types; an error or an exception in `cmp` leaves the
  list as it was.
* Maps: `keys` / `values` / `entries` in insertion order; `delete` answers whether the key was
  there; `merge` makes a new map (later maps win, a key keeps its first position);
  `get(k, d)` answers `d` for a missing key.
* math: `abs`, `min`, `max`, `clamp`, `floor`, `ceil`, `round` and `trunc` keep ints as ints
  (`math.floor(7)` is `7`, `math.floor(2.5)` is `2.0`); `round` rounds halves away from zero;
  `min` / `max` take arguments or one list and answer `nan` when one is `nan`. The float
  functions ([`math.rs`](../script/math.rs)) are fdlibm in its FreeBSD / musl form on
  `script::num`: errors below 1 ulp, exact where the result is (`math.log2(8)` is `3.0`,
  `math.pow(2, 10)` is `1024.0`), `exp` and `pow` bit for bit like Java's `StrictMath`
  except that `pow` follows C99 where Java answers NaN (`pow(1, nan)`, `pow(-1, inf)` and
  `pow(1, inf)` are 1; `pow(x, 0)` is 1 even for `nan`); `sin` / `cos` reduce their argument
  exactly, so `sin(1e300)` is right too. `tools/script_math_check.py [--target both]` checks
  them against Python's `decimal` (and Windows against Linux).
* json: `encode` writes compact JSON (or `indent` 1 to 8 spaces per level); int and bool keys
  become their text; `nan` / `inf`, bytes, functions, colliding keys (`1` and `"1"`), data
  that contains itself and more than 64 levels throw. `decode` takes a str or bytes and makes
  maps in member order, numbers without a point or exponent ints (floats when they do not
  fit), the others floats; errors throw with the position:
  `json.decode: Hier fehlt ein Doppelpunkt (Zeile 1, Spalte 6)`.
* Errors in arguments throw a catchable `str` in German, named after the function and the
  parameter (`str.split: sep muss str sein, nicht int`,
  `list.insert: Index 5 liegt außerhalb der Liste (Länge 3)`).
* Fuel follows the work, on top of the 1 of the call: 1 per new object, 1 per 64 bytes of text
  read or written, 1 per 8 list values and 1 per 4 map entries copied or visited, 1 per
  comparison of a sort, 2 for `sqrt`, 4 for `exp log log2 log10 sin cos`, 8 for `pow`, plus
  what callbacks cost. Long operations pay as they go, so `Limits::fuel` stops a huge sort,
  split or JSON job in the middle (`ErrorKind::Fuel`, not catchable), and a list being
  sorted stays as it was.
* Heap: results are allocated in the VM, so `Limits::heap` holds for them (`ErrorKind::Heap`,
  not catchable), and a text whose size is known before it is built (`repeat`, `replace`,
  `join`, `format`) is checked first: `"x".repeat(1000000000000)` fails at once instead of
  allocating. JSON needs up to about three times the heap limit as temporary memory for its
  syntax tree.

**Hosts with names of their own**: `compile` checks names against `script::GLOBALS` (the host
API of JFNetworker) and only warns about others. A host with another set passes its names to
`script::compile_with(name, src, &["document", "fetch"])`: the built-ins and the standard
library are known anyway, and a name that is neither declared nor known is an error with its
line and column ("Unbekannter Name prnt – meinst du print?"), so a typing error stops the script
before it runs. `check_with(src, globals)` gives the editor's diagnostics for such a host.

**The runner `jfs`** ([`apps/jfs`](../jfs/README.md)) runs scripts on the command line:

```sh
jfs hello.jfs Anna Ben                  # config.args = ["Anna", "Ben"]
jfs -e "print([3, 1, 2].sort())"        # a one-liner; "-" reads the script from stdin
jfs --check hello.jfs                   # only the diagnostics: hello.jfs:4:1: Fehler: …
jfs --virtual-time --seed 7 sim.jfs     # timers without waiting, rand() reproducible
jfs --disk store.json -c port=7000 --fuel 0 --heap 64M server.jfs
```

Its console host answers the names of `GLOBALS`: `print` to stdout, `log` to stderr, `now`,
`rand`, `config` (from `JFS_CONFIG`, `--config K=V` and `--env NAME`), `timer` (an event loop on
the real clock or with `--virtual-time` a virtual one; the program ends when no timer and no
callback is left, or at `--until MS`) and `disk` (in memory, or in a JSON file with
`--disk FILE`); `net`, `dns`, `http` and `metrics` throw a catchable error. So a script that
uses only the standard library, `print`, `log` and `timer` runs the same in JFNetworker and in
`jfs`. The limits come from `--fuel`, `--heap`, `--depth` and `--stack` (else
`Limits::default()`). Errors of the compiler and of the run go to stderr with file, line,
column, the source line and the calls; exit code 1 (2 for a wrong command line). Its tests
are example scripts with their expected stdout, stderr, exit code and files
(`apps/jfs/tests/cases/*.test`), run by `python build_rs.py test jfs` on Windows and Linux.

**Language services and the formatter** ([`script::ls`](../script/ls.rs),
[`script::fmt`](../script/fmt.rs)) are what an editor or IDE needs, without editor types:
positions are bytes into the source, results carry `Span`s (lines and columns from 1). They
work on code that does not parse (what `parse_partial` made of it), so completion and hover
keep working while a line is half typed.

```rust
use script::ls::{self, CompleteCx, HostItem};

// the host's names with a short description (JFNetworker: workspace/devwin/scriptls.rs)
let host = [
    HostItem { name: "net.listen", params: Some("port, handlers"), doc: "Wartet auf TCP-Verbindungen" },
    HostItem { name: "config", params: None, doc: "Die Einstellungen des Dienstes" },
];
let load = |path: &str| fs_read(path);                  // the text of import("path"), if any
let cx = CompleteCx { host: &host, load: Some(&load) };

let c = ls::complete(src, caret, &cx);                   // items for src[c.start..c.end]
for i in &c.items { show(&i.label, i.kind, &i.detail, &i.doc); }
let h = ls::hover(src, at, &cx);                         // "fn greet(name)\nFunktion\n<doc>"
let s = ls::signature(src, caret, &cx);                  // "greet(name, greeting)", active 1
let d = ls::definition(src, at, &cx);                    // Location { path: None, span }
let r = ls::references(src, at);                         // the declaration and every use
let e = ls::rename(src, at, "count")?;                   // Vec<Edit> or a German reason
let new_src = ls::apply(src, &e);
let folds = ls::folding(src);                            // Fold { line, end, kind }
let outline = ls::symbols(src);                          // fns (nested ones as children), lets
let tidy = script::fmt::format(src)?;                    // or the syntax errors
```

* The index (`ls::index`) follows the scope rules of `ast.rs`: top-level `let` and `fn` are the
  script's globals (visible in every function, in top-level code after the `let` ran), a block's
  `fn`s in the whole block, a `let` from the next statement on, parameters around the body, `for`
  and `catch` variables around their block; an inner declaration shadows. `Index::at(pos)` is
  the symbol of a name, `Index::resolve(name, pos)` the one a name would mean there.
* Doc comments are the `///` lines right above a `fn` or `let`. Hover shows the signature
  (`fn greet(name)`, `let f = fn(a, b)`, `let m = import("x.jfs")`), the kind ("Parameter von
  greet", "lokale Variable in handle") and the doc; for the library and the host their
  description (`ls::library_doc(name)` has the library's, German, our own words).
* Completion ranks the exact-case prefix first, then by kind: names in scope (locals before
  the script's globals), the host's names, the built-ins and library modules, keywords (the
  statement keywords only where a statement may start, `else` / `catch` after a `}`, `in` after
  `for x`). After a dot: the members of a host namespace or a library module, the top-level
  names of an imported module (through `CompleteCx::load`), the methods of `str` / `list` /
  `map` / `bytes` when a literal or a known call tells the type (`let s = "a"`, `.split(..)`),
  else of all four (a method on several types lists their signatures). Nothing in comments and
  strings; in `${..}` as in code.
* `rename` refuses keywords, invalid names, a name declared in the same scope, a rename that
  would change what any name means (it resolves the renamed source again and compares), and
  code with syntax errors (statements that did not parse would be missed).
* `fmt::format` (two blanks per level; `format_with(src, "    ")`): one statement per line, the
  `{` of a block on its line, `else` / `catch` after the `}`, indentation by the open brackets
  plus one level for a continued statement, blanks around operators and after `,` and `:`,
  none inside brackets; the line breaks inside expressions, comments and up to one blank line
  stay. It refuses code with syntax errors, and it checks that the tree of its result equals
  the original's (`Program::sexpr`) -- so the compiled program is the same. Idempotent.
* Tests: `coretest --only scriptls` (definitions and references with shadowing, closures, loops;
  rename and its refusals; completion in scope, after a dot, of an import and on broken code;
  hover; signature help; folding; the outline; the formatter on every syntax form, idempotent,
  the same code).
**Scripts of one VM** ([`module.rs`](../script/module.rs)), as a page has several `<script>`
elements: `vm.add_script(name, src)` links a further script while the VM is idle, then `run()` runs
its top-level code with fresh fuel. Their top-level names are one scope: a script sees the `fn`s and
`let`s of the program and of the scripts before it, the same variables, and a name it declares again
is that variable; it does not see the names of a later one. It is compiled the way the program was
(`compile_with`: an unknown name is an error). `Err` gives its diagnostics and the VM goes on as it
was; an error while it runs ends only that invocation, and errors and traces name each script's file.

```rust
let stub = script::compile_with("", "", HOST_NAMES)?;
let mut vm = Vm::new(&stub, limits);
vm.run(&mut host, u64::MAX);                     // the empty program
for (name, src) in scripts {
    match vm.add_script(name, src) {
        Ok(()) => { vm.run(&mut host, u64::MAX); }   // Done or Error; the next one runs anyway
        Err(diags) => report(&diags),
    }
}
```

**Modules** ([`module.rs`](../script/module.rs)): `import(path)` is a function, not
syntax. It gives the top-level names of another file as a map -- its `fn`s and `let`s, without
the names that start with `_` -- with the values they have when the module's code has run:

```js
// lib/util.jfs
let _calls = 0                          // private: not in the map
fn shout(s) { _calls += 1; return s.upper() + "!" }
fn calls() { return _calls }            // the module's functions see its globals as they change

// main.jfs
let util = import("lib/util.jfs")       // relative to this file
print(util.shout("hallo"), util.calls())   // HALLO! 1
```

The host reads the files: `Host::load(cx, path) -> Result<String, String>` (the source, or the
German reason; the default has no modules). The VM resolves a relative path against the
importing file's directory, normalises it (`/` between the parts, a `\` counts as one, no `.`
or `..`) and runs each path once per VM: the first import compiles the file the way the
program was compiled (`compile_named`, or `compile_with` with the same host names), links its
code behind the VM's own and runs its top-level code as a call inside the importing
invocation, with the same fuel, limits, heap and output, so it yields, pauses and steps like
any call; later imports give the same map. Errors are catchable strings: a cycle
(`Zyklischer Import: a.jfs → b.jfs → a.jfs`), a missing file (`Modul lib/x.jfs: die Datei gibt
es nicht`), a module that does not compile (`lib/x.jfs:3:9: Hier fehlt …`; uncaught, the
`Error` has the module's file, line and column). A module whose top-level code throws is
forgotten, the next import runs it again. From now on `Diagnostic`, `Error` and `TraceLine`
carry a `file`; `Error`'s text names it only where a call is in another file than the error,
so messages of single files stay as they were. The program's own host values (`config`) hold
for its modules too. `script::check` stays per file: a module's members are dynamic.

**The debug interface** ([`debug.rs`](../script/debug.rs)) has no UI; the runner's
`--debug` and JFNetworker's script services build on it. The compiler keeps a line table
(`Program::line_table()`: every instruction's function, line, column and whether a statement
starts there; `break_lines()` lists the lines a breakpoint can stop at), the locals of every
function with the instructions where they are visible, and the names of upvalues.

```rust
use script::{PauseReason, Status, Step};

vm.set_breakpoint("lib/util.jfs", 7, None)?;          // also before the module is loaded
vm.set_breakpoint("main.jfs", 12, Some("n > 100"))?;  // a condition in jfs
loop {
    match vm.run(&mut host, 10_000) {
        Status::Paused(p) => {                          // before the statement of p.line
            for f in vm.frames() { show(&f.func, &f.file, f.line) }   // innermost first
            for (name, v) in vm.locals(0) { show_var(&name, &vm.preview(v, 80)) }
            let x = vm.eval(0, "n * 2 + len(items)");  // read-only, no calls, fuel unchanged
            vm.step(Step::Over);                        // Into, Over, Out; or nothing: continue
        }
        Status::Yielded => {}
        Status::Done(_) | Status::Error(_) => break,
    }
}
vm.request_pause();                                     // stops at the next statement
```

* Stops: before a statement (the first one of a line; a `for` loop's head and a `while` loop's
  condition each round) and, while stepping, at the first instruction after a return from the
  function the step began in, also when an exception leaves it. `Into` stops at the next
  statement anywhere, `Over` at the next one of this function or a caller, `Out` back in the
  caller; breakpoints stop meanwhile. A breakpoint on a line without a statement stops at the
  next line with one; one whose condition fails stops with `Pause::error`. A pause ends the
  step, and so does the end of the invocation.
* `run` returns `Status::Paused(Pause { reason, file, line, col, breakpoint, error })` also in
  the middle of its fuel; the next `run` goes on from there, and a run that pauses equals one
  that does not (result, output, fuel). Code a native runs through `Cx::call` (a callback of
  `list.map` or `list.sort`) never stops: the native is in the middle of its work.
* Inspecting: `frames()`, `locals(i)` (an inner variable hides an outer one), `upvalues(i)`,
  `frame_globals(i)` and `globals_of(file)` (the private `_` ones too), `modules()`,
  `repr(v)` / `preview(v, max)` / `children(v)` for a variables view that expands lists and
  maps; `eval(i, expr)` knows literals, the names of the frame, operators, fields, indexes
  and `len`, `str`, `type`, `int`, `float`. `script::debug::check_expr` checks a condition.
* Cost: without a breakpoint, step or pause request the VM runs as without a debugger (`run`
  uses the instruction loop as it is; `coretest --bench vm` measures it); with one, `run`
  drives it one instruction at a time and checks the statement starts and breakpoints in
  between, about four times slower.

**Tests in jfs**: `assert(cond, msg)` throws `Annahme verletzt: msg` when `cond` is falsy;
`assert_eq(a, b, msg)` compares numbers and texts as `==` does and lists and maps by their
content (maps in any order) and throws `msg: erwartet B, ist A` with both values as in a list.
`jfs --test FILE…` runs every top-level function `test_*` of the files in a fresh VM (the
top-level code first, so each test has fresh globals and modules; then its timers on the
virtual clock, at most 60 s or `--until`) and prints `ok` or `FEHLER` with file, line, column,
message and what the test printed, then the count; exit code 1 when a test failed or there was
none. `jfs --debug FILE` is a small console debugger on standard input (`break FILE:LINE [if
COND]`, `run`, `step`, `next`, `finish`, `continue`, `print EXPR`, `bt`, `locals`, `quit`).
Tests: `coretest --only scriptmod` (modules, the line table, breakpoints and every step kind
through recursion, closures, `try` / `catch` and module boundaries, a pause in the middle of
the fuel, paused runs equal to uninterrupted ones, the stack, `eval`, `assert`, the benchmark's
programs with and without a debugger) and the runner's cases.
