// scripttest -- self-checks of JFScript (the crate jfscript, `script::`): known answers, round
// trips and edge cases of the lexer, parser and diagnostics (scriptt.rs), the compiler and VM with
// fuel, the heap and its collector (scriptvmt.rs), the standard library and its math
// (scriptlibt.rs), the language services and the formatter (scriptlst.rs), modules, the debug
// interface and the test functions (scriptmodt.rs).  They came from JFLib's coretest.
// Console program; exit code 1 when a check fails.
//
//   cargo run -p scripttest -- [--only NAME]   a group alone: script, scriptvm, scriptlib, scriptls, scriptmod
//   cargo run -p scripttest -- --bench vm      the VM's benchmark (scriptmodt.rs)
//   cargo run -p scripttest -- --math FILE     answers for tools/script_math_check.py
//   (cargo jflib test -p scripttest runs it through tests/all.uis; --script is ignored)
#![no_std]
#![no_main]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::String;

mod scriptlibt;
mod scriptlst;
mod scriptmodt;
mod scriptt;
mod scriptvmt;

/// the result collector of a check group
pub struct T {
    pub group: &'static str,
    pub ok: u32,
    pub failed: u32,
}

impl T {
    pub fn check(&mut self, name: &str, cond: bool) {
        if cond {
            self.ok += 1;
        } else {
            self.failed += 1;
            plat::write_stdout(format!("FAIL {}: {}\n", self.group, name).as_bytes());
        }
    }
    pub fn eq<A: PartialEq + core::fmt::Debug>(&mut self, name: &str, got: A, want: A) {
        if got == want {
            self.ok += 1;
        } else {
            self.failed += 1;
            plat::write_stdout(format!("FAIL {}: {}: got {:?}, want {:?}\n", self.group, name, got, want).as_bytes());
        }
    }
}

fn run() -> i32 {
    let only = plat::arg_value("--only").map(String::from);
    if let Some(f) = plat::arg_value("--math") {
        return scriptlibt::math_file(&f);
    }
    if plat::arg_value("--bench").as_deref() == Some("vm") || plat::args().iter().any(|a| a == "--bench") {
        scriptmodt::bench();
        return 0;
    }
    let groups: [(&'static str, fn(&mut T)); 5] = [
        ("script", scriptt::run),
        ("scriptvm", scriptvmt::run),
        ("scriptlib", scriptlibt::run),
        ("scriptls", scriptlst::run),
        ("scriptmod", scriptmodt::run),
    ];
    let mut failed = 0;
    let mut total = 0;
    for (name, f) in groups {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let mut t = T { group: name, ok: 0, failed: 0 };
        f(&mut t);
        plat::write_stdout(format!("{:<10} {} ok, {} failed\n", name, t.ok, t.failed).as_bytes());
        failed += t.failed;
        total += t.ok + t.failed;
    }
    plat::write_stdout(format!("scripttest: {} of {} checks passed\n", total - failed, total).as_bytes());
    if failed == 0 { 0 } else { 1 }
}

rt::main!(run());
