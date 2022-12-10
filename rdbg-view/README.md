# rdbg-view

[![Crate](https://img.shields.io/crates/v/rdbg-view)](https://crates.io/crates/rdbg-view)
[![Docs](https://docs.rs/rdbg-view/badge.svg)](https://docs.rs/rdbg-view)
[![MSRV](https://img.shields.io/badge/msrv-1.58-blue.svg)](https://crates.io/crates/rdbg-view)

A basic command-line viewer for [rdbg](https://crates.io/crates/rdbg)

## Install

```bash
cargo install rdbg-view
```

## Command-line Options

```bash
A basic command-line viewer for rdbg

Usage: rdbg-view [OPTIONS] [HOSTNAME]

Arguments:
  [HOSTNAME]  Remote hostname of debugged program [default: 127.0.0.1]

Options:
  -p, --port <PORT>  Remote port on debugged program [default: 13579]
  -d, --debug-fmt    Use debug formatting for messages (:#? formatting style)
  -h, --help         Print help information
  -V, --version      Print version information
```

## Example output

The current version provides a typical log viewer by default, but a Rust debug
style output (:#? formatting) is available as well with the `--debug-fmt`  flag

```bash
*** Trying to connect to 127.0.0.1:13579... ***
*** Connected to 127.0.0.1:13579 ***
T:1670688040648 THR:1 rdbg/examples/hello_world.rs:4 hello world
T:1670688040648 THR:1 rdbg/examples/hello_world.rs:5 |world->"world"| |1 + 5->6|
*** Disconnected from 127.0.0.1:13579 ***
```
