use clap::Parser;
use rdbg_client::{Error, Event, Message, MsgIterator, MsgPayload, DEFAULT_ADDR, DEFAULT_PORT};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Remote hostname of debugged program
    #[arg(default_value_t = DEFAULT_ADDR.to_string())]
    hostname: String,

    /// Remote port on debugged program
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Use debug formatting for messages (:#? formatting style)
    #[arg(short, long, default_value_t = false)]
    debug_fmt: bool,
}

fn main() {
    let args = Args::parse();
    eprintln!(
        "*** Trying to connect to {}:{}... ***",
        &args.hostname, args.port
    );

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
                    if args.debug_fmt {
                        println!("{msg:#?}");
                    } else {
                        print_message(&msg);
                    }
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

fn print_message(msg: &Message) {
    print!(
        "T:{} THR:{} {}:{}",
        msg.time, msg.thread_id, msg.filename, msg.line
    );

    match &msg.payload {
        MsgPayload::Message(msg) => {
            // If it contains a newline (but not at end) then move to next line to give alignment
            if !msg.is_empty() && msg[..msg.len()].contains('\n') {
                println!();
            }

            println!(" {}", msg);
        }
        MsgPayload::Values(values) => {
            for (key, value) in values {
                print!(" |{key}->{value}|")
            }
            println!();
        }
    }
}
