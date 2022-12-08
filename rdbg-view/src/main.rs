use rdbg_client::{Error, Event, MsgIterator, DEFAULT_ADDR, DEFAULT_PORT};

fn main() {
    eprintln!("*** Trying to connect to {DEFAULT_ADDR}:{DEFAULT_PORT}... ***");

    let iterator = MsgIterator::default();

    for event in iterator {
        match event {
            Ok(event) => match event {
                Event::Connected(addr) => {
                    eprintln!("*** Connected to {addr} ***");
                }
                Event::Disconnected(addr) => {
                    eprintln!("*** Disconnected from {addr} ***");
                }
                Event::Message(msg) => {
                    println!("{msg:#?}");
                }
            },
            Err(err) => match err {
                Error::BadVersion => {
                    eprintln!("*** Bad version (we only understand wire protocol 1) ***");
                    break;
                }
                Error::BadUtf8(err) => {
                    eprintln!("*** Bad UTF8 found in string ({err}) ***");
                }
                Error::CorruptMsg => {
                    eprintln!("*** Corrupt message received ***");
                }
            },
        }
    }

    eprintln!("*** Exiting... ***");
}
