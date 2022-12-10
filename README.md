# rdbg

This is is a small set of crates for quick and dirty debugging.
It is more or less equivalent to [dbg](https://doc.rust-lang.org/std/macro.dbg.html) and
[println](https://doc.rust-lang.org/std/macro.println.html) in the stdlib
but delivers the payload via a TCP socket to a remote
[viewer](https://crates.io/crates/rdbgp-view).

There are three crates currently:
* [rdbg](crates.io/crates/rdbg) - Used by the debugged program
* [rdbg-view](crates.io/crates/rdbg-view) - A very basic command line viewer
* [rdbg-client](crates.io/crates/rdbg-client) - A crate that makes it very easy to write your own viewer 
