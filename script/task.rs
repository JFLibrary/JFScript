// task -- (S3) await: promises, the invocations that wait for them, and how they go on
//
//   let p = cx.promise()?;                   in a native: a new promise (Promise { id, value });
//   self.jobs.push(p.id); Ok(p.value)        the native starts its work and returns p.value
//   vm.resolve(id, v) / vm.reject(id, e)     later, between runs (or inside a native): the
//   vm.reject_msg(id, "Datei fehlt")         promise settles, the invocations that wait for it
//                                            are ready
//   while let Some(p) = vm.wake() { .. }     the next ready invocation is the pending one again;
//                                            run() continues it (p: its promise, see below)
//   vm.waiting() / vm.ready()                invocations that wait / whose value came
//   vm.is_waiting(p)                         p is the promise of an invocation that waits (what
//                                            run() gave), not a promise the script returned
//   vm.promise_state(v)                      Waiting, Resolved(v) or Rejected(v) of a promise
//   vm.cancel_waiting(p)                     drop a waiting invocation (its promise rejects)
//
// The script writes `await e`.  e not a promise: the value itself (as in JavaScript).  A settled
// promise: its value, or its reason thrown at the await (try / catch catches it; uncaught, the
// error has the await's line and column).  A promise that is not settled yet: the invocation
// waits.  Its stack and frames (all of them: await may stand in any function, the top-level code
// of the program, of a script of add_script or of a module) move out of the VM into a Task, run()
// returns Status::Done(p) with p the invocation's own promise (a new one at its first await, the
// same one after that), and the VM is idle: the host can call(), emit(), add_script() and run
// other invocations.  When the awaited promise settles, the task is ready; wake() puts the oldest
// ready one back (stack, frames, fuel, the debugger's step), the await runs again and finds the
// value.  When the woken invocation ends, its promise settles too: resolved with its result, or
// rejected with the message of its error (other invocations may wait for it when the host gave
// it to them).  So a host that does not use promises sees nothing new.
//
// Rules that keep it deterministic and safe:
//   * Order: ready tasks wake in the order their promises settled; the waiters of one promise in
//     the order they began to wait.  Nothing runs by itself: the host calls wake().
//   * Fuel and limits: Limits::fuel holds for each stretch of an invocation between two waits
//     (the user's decision of 2026-11-27): one that waits often -- a dialog, the network, a host
//     operation per shape -- is not ended only because it is long, while an endless loop without
//     a wait still ends with ErrorKind::Fuel.  The invocation's own count goes on over the waits:
//     call_fuel() and cx.fuel() count from its start (a host that turns fuel into time, as
//     JFNetworker does, sees one clock), and the stretch's limit is that count at the wake plus
//     Limits::fuel (Vm::fuel_end).  The other limits hold as for any invocation.
//   * Slices: the wait begins at the await instruction however a run is split; the await costs
//     1 fuel, and 1 again when it runs after the wake.
//   * Callbacks: code a native runs through Cx::call (list.map, list.sort, a host's own) cannot
//     wait, the native's Rust frames are in between: await of a promise that is not settled
//     throws there ("await kann in einem Rückruf … nicht warten", catchable, with the position);
//     settled promises and other values work.  check warns where it sees it (awaitchk.rs).
//   * The collector: the promises not settled yet are roots (the host settles them by id, so it
//     holds them until then, like a pin), the waiting invocations' stacks and closures too, and
//     the promise of the running invocation.  A settled promise is an ordinary object.
//   * Clone: a clone of the VM holds the same waiting invocations and promises with the same ids;
//     the host settles them in each VM it lets go on.
//   * The debugger: a step goes with its invocation (it does not stop another one that runs
//     meanwhile) and goes on after the wake; breakpoints, frames, locals and eval work in the
//     woken invocation as in any other.
//   * Drops of host objects (Host::drop) are delivered when an invocation begins to wait, as when
//     one ends.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::debug::Step;
use crate::heap::{Heap, Obj, PState, PromiseObj, Ref, Value};
use crate::module::UnitState;
use crate::vm::{msg, Cx, Error, Exc, Fail, Frame, Status, Vm};

/// (S3) a promise a native made (Cx::promise, Vm::new_promise): the id the host settles it by
/// (Vm::resolve, Vm::reject) and the value the script gets (what the native returns)
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Promise {
    pub id: u64,
    pub value: Value,
}

/// (S3) where a promise stands (Vm::promise_state)
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PromiseState {
    Waiting,
    Resolved(Value),
    Rejected(Value),
}

/// an invocation that waits in await, or whose value came (Tasks::ready)
#[derive(Clone)]
struct Task {
    /// the invocation's own promise (run() gave it when the invocation began to wait)
    promise: Ref,
    /// the id of the promise it waits for
    awaits: u64,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    /// its fuel so far
    fuel: u64,
    /// the debugger's step it was in
    step: Option<(Step, usize)>,
}

/// a promise that is not settled yet: its object, the invocations that wait for it (task ids)
#[derive(Clone)]
struct Open {
    r: Ref,
    waiters: Vec<u64>,
}

/// the VM's promises and waiting invocations (see the module comment)
#[derive(Clone, Default)]
pub(crate) struct Tasks {
    /// the promises not settled yet, by id (roots)
    open: BTreeMap<u64, Open>,
    /// the last id given
    next: u64,
    /// the waiting invocations by the id of their own promise
    waiting: BTreeMap<u64, Task>,
    /// the tasks whose promise settled, in that order
    ready: VecDeque<u64>,
    /// the promise of the running invocation, once it waited
    current: Option<(u64, Ref)>,
}

impl Tasks {
    /// the running invocation waited before (its promise settles when it ends)
    pub(crate) fn current_is_some(&self) -> bool {
        self.current.is_some()
    }

    /// mark what the promises and the waiting invocations hold (the collector's roots)
    pub(crate) fn roots(&self, heap: &mut Heap, gray: &mut Vec<u32>) {
        for o in self.open.values() {
            heap.mark_ref(o.r, gray);
        }
        for t in self.waiting.values() {
            heap.mark_ref(t.promise, gray);
            for v in &t.stack {
                heap.mark(*v, gray);
            }
            for f in &t.frames {
                if let Some(c) = f.closure {
                    heap.mark_ref(c, gray);
                }
            }
        }
        if let Some((_, r)) = self.current {
            heap.mark_ref(r, gray);
        }
    }
}

/// the message of await in a callback of a native
fn in_callback() -> String {
    String::from(
        "await kann in einem Rückruf, den eine Funktion aufruft (list.map, list.sort, eine Funktion des Hosts), nicht warten: der Wert steht noch aus – warte außerhalb, etwa in einer for-Schleife",
    )
}

impl Vm {
    // ---- promises

    /// a new promise that is not settled yet (between runs; a native takes Cx::promise): its id
    /// and its value, valid until the host settles it (it is kept alive until then)
    pub fn new_promise(&mut self) -> Result<Promise, Error> {
        self.promise_new().map_err(crate::vm::plain)
    }

    pub(crate) fn promise_new(&mut self) -> Result<Promise, Exc> {
        let id = self.tasks.next + 1;
        let r = self.alloc(Obj::Promise(Box::new(PromiseObj { id, state: PState::Waiting })))?;
        self.tasks.next = id;
        self.tasks.open.insert(id, Open { r, waiters: Vec::new() });
        Ok(Promise { id, value: Value::Host(r) })
    }

    /// settle promise `id` with the value v: the invocations that wait for it are ready (wake);
    /// false when no promise of this id waits (settled already, an unknown id) or v is not valid
    pub fn resolve(&mut self, id: u64, v: Value) -> bool {
        self.valid(v) && self.settle(id, PState::Resolved(v))
    }

    /// settle promise `id` with a reason: await throws it in the invocations that wait for it
    /// (as `throw reason` there); false as for resolve
    pub fn reject(&mut self, id: u64, reason: Value) -> bool {
        self.valid(reason) && self.settle(id, PState::Rejected(reason))
    }

    /// reject() with a str of the message (as Cx::error makes one); with the heap full the text
    /// "Speichergrenze erreicht"
    pub fn reject_msg(&mut self, id: u64, msg: &str) -> bool {
        if !self.tasks.open.contains_key(&id) {
            return false;
        }
        let v = self.alloc_str(String::from(msg)).unwrap_or(self.limit_text());
        self.settle(id, PState::Rejected(v))
    }

    fn settle(&mut self, id: u64, st: PState) -> bool {
        let Some(o) = self.tasks.open.remove(&id) else { return false };
        if let Obj::Promise(p) = self.heap.get_mut(o.r) {
            p.state = st;
        }
        self.tasks.ready.extend(o.waiters);
        true
    }

    /// the state of a promise (None for other values); its id is in vm.object(v)
    pub fn promise_state(&self, v: Value) -> Option<PromiseState> {
        let Value::Host(r) = v else { return None };
        if !self.valid(v) {
            return None;
        }
        match &self.heap.objs[r.0 as usize] {
            Obj::Promise(p) => Some(match p.state {
                PState::Waiting => PromiseState::Waiting,
                PState::Resolved(x) => PromiseState::Resolved(x),
                PState::Rejected(x) => PromiseState::Rejected(x),
            }),
            _ => None,
        }
    }

    // ---- waiting invocations

    /// the invocations that wait in await, ready ones included
    pub fn waiting(&self) -> usize {
        self.tasks.waiting.len()
    }

    /// the waiting invocations whose promise settled: wake() takes them in that order
    pub fn ready(&self) -> usize {
        self.tasks.ready.len()
    }

    /// whether v is the promise of an invocation that waits (what run() gave when it began to
    /// wait), ready or not; false for other values, a promise of the host among them (a script
    /// may return one: Done(p) with p waiting, but nothing to wake)
    pub fn is_waiting(&self, v: Value) -> bool {
        let Value::Host(r) = v else { return false };
        if !self.valid(v) {
            return false;
        }
        match &self.heap.objs[r.0 as usize] {
            Obj::Promise(p) => self.tasks.waiting.get(&p.id).is_some_and(|t| t.promise == r),
            _ => false,
        }
    }

    /// make the oldest ready invocation the pending one (run() continues it after its await) ->
    /// its promise, the one run() gave when it began to wait; None when the VM is busy or none
    /// is ready
    pub fn wake(&mut self) -> Option<Value> {
        if self.busy() {
            return None;
        }
        let id = self.tasks.ready.pop_front()?;
        let t = self.tasks.waiting.remove(&id)?;
        self.stack = t.stack;
        self.frames = t.frames;
        self.call_fuel = t.fuel;
        // a new stretch: Limits::fuel from here on (call_fuel counts on)
        self.fuel_end = t.fuel.saturating_add(self.lim.fuel);
        self.tasks.current = Some((id, t.promise));
        if let Some(s) = t.step {
            self.dbg_mut().set_step(s);
        }
        Some(Value::Host(t.promise))
    }

    /// drop the waiting invocation of promise p (the one run() gave): it does not go on, its
    /// promise is rejected ("Der Aufruf wurde abgebrochen"); false when p is no such promise.  A
    /// module whose top-level code it was in runs again at its next import.
    pub fn cancel_waiting(&mut self, p: Value) -> bool {
        let Value::Host(r) = p else { return false };
        if !self.valid(p) {
            return false;
        }
        let Obj::Promise(o) = &self.heap.objs[r.0 as usize] else { return false };
        let id = o.id;
        let Some(t) = self.tasks.waiting.remove(&id) else { return false };
        if let Some(o) = self.tasks.open.get_mut(&t.awaits) {
            o.waiters.retain(|w| *w != id);
        }
        self.tasks.ready.retain(|w| *w != id);
        for f in &t.frames {
            if f.closure.is_none() && f.proto != 0 {
                let u = self.code.protos[f.proto as usize].unit as usize;
                self.units[u].state = UnitState::Failed;
            }
        }
        self.reject_msg(id, "Der Aufruf wurde abgebrochen")
    }

    // ---- the VM's side

    /// Op::Await met a host object (r): a promise gives its value or throws its reason; one that
    /// is not settled makes the invocation wait (Exc::Wait: run() parks it) unless a native runs
    /// this code through Cx::call; other objects stay
    #[cold]
    #[inline(never)]
    pub(crate) fn await_value(&mut self, r: Ref) -> Result<(), Exc> {
        let Obj::Promise(p) = &self.heap.objs[r.0 as usize] else { return Ok(()) };
        match p.state {
            PState::Resolved(v) => {
                let n = self.stack.len();
                self.stack[n - 1] = v;
                Ok(())
            }
            PState::Rejected(v) => Err(Exc::Throw(v)),
            PState::Waiting if self.nest > 0 => msg(in_callback()),
            PState::Waiting => Err(Exc::Wait),
        }
    }

    /// run(): the invocation waits at its await (the promise on the top of its stack) -> its
    /// own promise; the VM is idle then
    #[cold]
    #[inline(never)]
    pub(crate) fn park(&mut self) -> Result<Value, Exc> {
        let awaits = match self.stack.last() {
            Some(Value::Host(a)) => match &self.heap.objs[a.0 as usize] {
                Obj::Promise(p) if self.tasks.open.contains_key(&p.id) => p.id,
                _ => return crate::vm::internal(),
            },
            _ => return crate::vm::internal(),
        };
        let (id, r) = match self.tasks.current.take() {
            Some(c) => c,
            None => {
                let p = self.promise_new()?;
                let Value::Host(r) = p.value else { return crate::vm::internal() };
                (p.id, r)
            }
        };
        // the await runs again after the wake and finds the value
        if let Some(f) = self.frames.last_mut() {
            f.pc = f.pc.saturating_sub(1);
        }
        let step = self.dbg.as_deref_mut().and_then(|d| d.take_step());
        let t = Task {
            promise: r,
            awaits,
            stack: core::mem::take(&mut self.stack),
            frames: core::mem::take(&mut self.frames),
            fuel: self.call_fuel + self.used,
            step,
        };
        if let Some(o) = self.tasks.open.get_mut(&awaits) {
            o.waiters.push(id);
        }
        self.tasks.waiting.insert(id, t);
        self.nroots.clear();
        self.escaped = None;
        Ok(Value::Host(r))
    }

    /// run() ended an invocation that had waited: its promise settles with the result or the
    /// error's message
    #[cold]
    #[inline(never)]
    pub(crate) fn task_ends(&mut self, st: &Status) {
        let Some((id, _)) = self.tasks.current.take() else { return };
        match st {
            Status::Done(v) => {
                self.settle(id, PState::Resolved(*v));
            }
            Status::Error(e) => {
                self.reject_msg(id, &e.msg);
            }
            _ => {}
        }
    }

    /// cancel(): the pending invocation's promise, if it waited before
    pub(crate) fn task_cancelled(&mut self) {
        if let Some((id, _)) = self.tasks.current.take() {
            self.reject_msg(id, "Der Aufruf wurde abgebrochen");
        }
    }

    /// the text of a promise for print and the debugger: "<promise 3 ausstehend>", "<promise 3
    /// erfüllt: 42>", "<promise 3 abgelehnt: \"kaputt\">"
    pub(crate) fn write_promise(&self, o: &mut String, p: &PromiseObj, depth: u32, seen: &mut Vec<Ref>, host: Option<&dyn crate::Host>) {
        let (what, v) = match p.state {
            PState::Waiting => ("ausstehend", None),
            PState::Resolved(v) => ("erfüllt", Some(v)),
            PState::Rejected(v) => ("abgelehnt", Some(v)),
        };
        o.push_str(&format!("<promise {} {}", p.id, what));
        if let Some(v) = v {
            o.push_str(": ");
            if depth >= 32 {
                o.push('…');
            } else {
                self.write_value(o, v, true, depth + 1, seen, host);
            }
        }
        o.push('>');
    }
}

impl Cx<'_> {
    /// (S3) a new promise for a long operation (a file dialog, a download): return its value to
    /// the script, keep its id, and settle it later with Vm::resolve / reject (between runs) or
    /// Cx::resolve / reject; the script's await waits until then (task.rs)
    pub fn promise(&mut self) -> Result<Promise, Fail> {
        let r = self.vm.promise_new();
        r.map_err(|e| self.fail(e))
    }

    /// (S3) settle a promise inside a native (see Vm::resolve): the invocations that wait for
    /// it are ready; they go on when the host wakes them, after this invocation
    pub fn resolve(&mut self, id: u64, v: Value) -> bool {
        self.vm.resolve(id, v)
    }

    /// (S3) reject a promise inside a native (see Vm::reject)
    pub fn reject(&mut self, id: u64, reason: Value) -> bool {
        self.vm.reject(id, reason)
    }
}
