use rdbg_view::{Error, Event, MsgIterator};

fn main() {
    eprintln!("*** Waiting for connection... ***");

    let iterator = MsgIterator::default();

    for event in iterator {
        match event {
            Event::Connected(addr) => {
                eprintln!("*** Connected to {addr} ***");
            }
            Event::Disconnected(addr) => {
                eprintln!("*** Disconnected from {addr} ***");
            }
            Event::Message(msg) => {
                println!("{msg:#?}");
            }
            Event::Error(err) => match err {
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
