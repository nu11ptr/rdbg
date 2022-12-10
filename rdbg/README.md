# rdbg

[![Crate](https://img.shields.io/crates/v/rdbg)](https://crates.io/crates/rdbg)
[![Docs](https://docs.rs/rdbg/badge.svg)](https://docs.rs/rdbg)
[![MSRV](https://img.shields.io/badge/msrv-1.63-blue.svg)](https://crates.io/crates/rdbg)

Quick and dirty Rust remote debugging. This crate is more or less equivalent to 
[dbg](https://doc.rust-lang.org/std/macro.dbg.html) and 
[println](https://doc.rust-lang.org/std/macro.println.html) in the stdlib
but delivers the payloads via a TCP socket to a remote 
[viewer](https://crates.io/crates/rdbg-view).

### Features

* No dependencies
* Enabled and added in seconds
* Familiar API
* Can be quickly be removed or compiled into "no-op"

## Example

```rust
let world = "world!";
// More or less equivalent to `println`
rdbg::msg!("Hello {}", world);

// More or less equivalent to `dbg`
rdbg::vals!(world, 1 + 1);
```

That works fine for servers and long running programs, but since the messages are delivered
via a different thread there is an implicit race condition. As such, if your program
is not a server or long running you will likely need the `wait_and_quit` function at
the end of your program. This will pause execution until all messages have been sent
via the TCP socket.

```rust
let world = "world!";
// More or less equivalent to `println`
rdbg::msg!("Hello {}", world);

// More or less equivalent to `dbg`
rdbg::vals!(world, 1 + 1);
// Wait for messages to be transmitted before exiting
rdbg::quit_and_wait();
```

## Usage

```toml
[dependencies]
rdbg = "0.1.4"
```

## Features

* `enabled` (default) - enables debugging
* `insecure-remote` - Listens on 0.0.0.0 for remote debugging purposes (insecure, no auth)

Use `--no-default-features` option to quickly turn this crate into a no-op. Please note
that due to feature unification other uses of this crate within the same project could
turn it back on.
