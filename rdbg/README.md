# rdbg

[![Crate](https://img.shields.io/crates/v/rdbg)](https://crates.io/crates/rdbg)
[![Docs](https://docs.rs/rdbg/badge.svg)](https://docs.rs/rdbg)
[![MSRV](https://img.shields.io/badge/msrv-1.63-blue.svg)](https://crates.io/crates/rdbg)

Quick and dirty Rust remote debugging. This crate is more or less equivalent to 
[dbg](https://doc.rust-lang.org/std/macro.dbg.html) and 
[println](https://doc.rust-lang.org/std/macro.println.html) in the stdlib
but delivers the payloads via a TCP socket to a remote 
[viewer](https://github.com/nu11ptr/rdbg/tree/main/rdbg-view)
