use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::{io, thread};

const BIND_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 13579;
const BUFFER_SIZE: usize = 2048;

static SENDER: Mutex<Option<Sender<Message>>> = Mutex::new(None);

struct Message {
    filename: &'static str,
    line: u32,
    msg: String,
}

impl Message {
    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        // For each string, write length first so we know boundaries on the other side
        buffer.extend(self.filename.len().to_be_bytes());
        buffer.extend(self.filename.as_bytes());

        buffer.extend(self.line.to_be_bytes());

        buffer.extend(self.msg.len().to_be_bytes());
        buffer.extend(self.msg.as_bytes());
    }
}

pub struct RemoteDebug {
    sender: Sender<Message>,
}

impl RemoteDebug {
    fn new(port: u16) -> Self {
        let sender = &mut *SENDER.lock().expect("Mutex poisoned!");

        // If our global var is already inited, just return it otherwise do one time thread creation
        let sender = match sender {
            Some(sender) => sender.clone(),
            None => {
                let new_sender = handle_connections(port);
                *sender = Some(new_sender.clone());
                new_sender
            }
        };

        Self { sender }
    }

    pub fn output(&self, msg: &str) {
        // We have no good way to report errors, so just unwrap and panic, if needed
        // (can likely only happen if our thread panics freeing the receiver)
        self.sender
            .send(Message {
                filename: file!(),
                line: line!(),
                msg: msg.to_string(),
            })
            .unwrap();
    }
}

#[inline]
pub fn default() -> RemoteDebug {
    RemoteDebug::new(DEFAULT_PORT)
}

#[inline]
pub fn port(port: u16) -> RemoteDebug {
    RemoteDebug::new(port)
}

fn handle_connections(port: u16) -> Sender<Message> {
    let (sender, receiver) = channel::<Message>();

    thread::spawn(move || {
        let mut curr_msg = None;

        loop {
            let mut buffer = Vec::with_capacity(BUFFER_SIZE);

            // We have no good way to report errors, so just unwrap and panic, if needed
            // (likely due to 'address in use' or 'permission denied', so we want to know about that
            // not mysteriously just not receive messages)
            let listener = TcpListener::bind((BIND_ADDR, port)).unwrap();

            // Per docs, 'incoming' will always return an entry
            if let Ok(mut stream) = listener.incoming().next().unwrap() {
                process_stream(&mut stream, &receiver, &mut curr_msg, &mut buffer);
            }
        }
    });

    sender
}

fn process_stream(
    stream: &mut TcpStream,
    receiver: &Receiver<Message>,
    curr_msg: &mut Option<Message>,
    buffer: &mut Vec<u8>,
) {
    loop {
        // If we were interrupted sending last message then resend otherwise wait for a new message
        let msg = match &curr_msg {
            Some(msg) => msg,
            None => {
                // We have no good way to report errors, so just unwrap and panic, if needed
                // (this is likely impossible since our Sender is in a global var)
                *curr_msg = Some(receiver.recv().unwrap());
                // Can't fail, stored above
                curr_msg.as_ref().unwrap()
            }
        };

        buffer.clear();
        msg.write_to_buffer(buffer);

        match write_to_stream(buffer, stream) {
            Ok(_) => {
                // Success, don't resend this message again
                *curr_msg = None;
            }
            Err(_) => {
                // Preserve current message and resend on next connection
                break;
            }
        }
    }
}

fn write_to_stream(buffer: &[u8], stream: &mut TcpStream) -> io::Result<()> {
    let mut written = 0;

    // Keep writing until everything in the buffer has been written or we get an error
    while written < buffer.len() {
        match stream.write(buffer) {
            Ok(wrote) => {
                written += wrote;
            }
            Err(err) => {
                return Err(err);
            }
        }
    }

    Ok(())
}
