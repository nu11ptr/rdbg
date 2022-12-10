# rdbg-view

[![Crate](https://img.shields.io/crates/v/rdbg-view)](https://crates.io/crates/rdbg-view)
[![Docs](https://docs.rs/rdbg-view/badge.svg)](https://docs.rs/rdbg-view)
[![MSRV](https://img.shields.io/badge/msrv-1.58-blue.svg)](https://crates.io/crates/rdbg-view)

A basic command-line viewer for [rdbg](https://crates.io/crates/rdbg)

## Install

```bash
cargo install rdbg-view
```

## Example output

The current version provides a hierarchical output, but a future version
should offer a more traditional log viewer as well.

```bash
*** Connected to 127.0.0.1:13579 ***
Message {
    time: 1670471290453,
    thread_id: "ThreadId(1)",
    filename: "rdbg/examples/hello_world.rs",
    line: 4,
    payload: Message(
        "hello world",
    ),
}
Message {
    time: 1670471290453,
    thread_id: "ThreadId(1)",
    filename: "rdbg/examples/hello_world.rs",
    line: 5,
    payload: Values(
        [
            (
                "world",
                "\"world\"",
            ),
            (
                "1 + 5",
                "6",
            ),
        ],
    ),
}
*** Disconnected from 127.0.0.1:13579 ***
```
