# JFScript

A small, dynamically typed scripting language written in Rust for programs built with
[JFLib](https://jflib.org): a compiler to bytecode and a stack VM with fuel (virtual CPU time), a
heap of its own with a mark-and-sweep collector, hard limits for memory, depth and steps, modules,
a debug interface and language services for editors. Everything is deterministic: no clock, no
randomness and float arithmetic in software, so a script gives the same bits on every target.

JFScript runs the `.jfs` scripts of [JFNetworker](https://github.com/JFLibrary/JFNetworker), where
virtual hosts run services written in it. It came out of JFLib's repository.

```text
let greet = fn(name) { return "Hallo, ${name}!" }
for n in config.args { print(greet(n)) }
```

## What is here

| Folder | What it is |
|---|---|
| [`script/`](script/lib.rs) | the language: the crate `jfscript`, used as `script` (`script::compile`, `script::Vm`) |
| [`jfs/`](jfs/README.md) | the runner `jfs`: `.jfs` files on the command line, like `python file.py` |
| [`scripttest/`](scripttest/src/main.rs) | the self-checks: lexer, parser, VM, standard library, language services, modules |
| [`docs/LANGUAGE.md`](docs/LANGUAGE.md) | the language, its standard library, the host interface, modules, the debugger |
| [`tools/`](tools) | `linux_test.py` (the tests on Linux from Windows), `script_math_check.py` (the math against Python's decimal module) |

## Use it in a program

JFScript is not on crates.io. A program names this repository:

```toml
[dependencies]
script = { package = "jfscript", git = "https://github.com/JFLibrary/JFScript" }
```

`Cargo.lock` keeps the commit; `cargo update -p jfscript` takes a newer one. The crate is
`no_std` and needs only `jflib-agent-core` from JFLib.

## Build and test

You need Rust through rustup (1.98 or later). JFLib comes from crates.io. The tests run through
[`cargo jflib`](https://crates.io/crates/cargo-jflib) (`cargo install cargo-jflib`); the helper
scripts need Python 3.11.

```sh
cargo build --workspace                       # jfs and scripttest
cargo run -p jfs -- -e "print(1 + 2)"          # a one-liner
cargo jflib test -p jfs                       # the runner's example cases
cargo jflib test -p scripttest                # every self-check
python tools/linux_test.py                    # both on Linux, from Windows through WSL
```

On Linux, build with `--target x86_64-unknown-linux-gnu` (the flags in `.cargo/config.toml` then
reach only the programs): `cargo jflib test -p jfs --target x86_64-unknown-linux-gnu`.

## License

MIT or Apache-2.0, like JFLib ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
