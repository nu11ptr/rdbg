use std::io::Write;
use std::mem::size_of;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{io, thread};

const BIND_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 13579;

const CHAN_MAX_MESSAGES: usize = 32;
const LEN_FIELD_SIZE: usize = size_of::<u32>();
const WIRE_PROTOCOL_VERSION: u8 = 1;

static SENDER: Mutex<Option<SyncSender<Message>>> = Mutex::new(None);

// *** msg / vals macros ***

/// Send a debug message to the remote viewer
///
/// ```
/// let world = "world!";
/// rdbg::msg!("Hello {}", world);
///
/// rdbg::msg!(rdbg::port(5000), ["Hello {}", world]);
/// ```
#[macro_export]
macro_rules! msg {
    ($port:expr, [ $($arg:tt)* ]) => {
        $port.send_message(file!(), line!(), $crate::MsgPayload::Message(
            std::fmt::format(format_args!($($arg)*))
        ))
    };

    ($($arg:tt)*) => {
        $crate::RemoteDebug::default().send_message(file!(), line!(), $crate::MsgPayload::Message(
            std::fmt::format(format_args!($($arg)*))
        ))
    };
}

/// Send debug expression name/value pairs to the remote viewer
///
/// ```
/// let world = "world!";
/// rdbg::vals!(world, 1 + 1);
///
/// rdbg::vals!(rdbg::port(5000), [world, 1 + 1]);
/// ```
#[macro_export]
macro_rules! vals {
    ($port:expr, [ $($value:expr),+ $(,)? ]) => {
        $port.send_message(file!(), line!(), $crate::MsgPayload::Values(vec![$((
            match $value {
                val => {
                    (stringify!($value), format!("{:#?}", &val))
                }
            }
        )),+]))
    };

    ($($value:expr),+ $(,)?) => {
        $crate::RemoteDebug::default().send_message(file!(), line!(), $crate::MsgPayload::Values(vec![$((
            match $value {
                val => {
                    (stringify!($value), format!("{:#?}", &val))
                }
            }
        )),+]))
    };
}

// *** Message related functions ***

fn current_time() -> u64 {
    // This can only really fail if time goes to before the epoch, which likely isn't possible
    // on today's system clocks
    // While this returns a u128, u64 ought to be large enough to hold all ms since the epoch
    // for our lifetimes
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

#[inline]
fn required_str_capacity(s: &str) -> usize {
    s.as_bytes().len() + LEN_FIELD_SIZE
}

// *** MsgPayloadVal ***

#[repr(u8)]
enum MsgPayloadVal {
    Message = 1,
    Values = 2,
}

// *** MsgPayload ***

#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum MsgPayload {
    // A formatted string
    Message(String),
    // A list of name/value pairs from expressions
    Values(Vec<(&'static str, String)>),
}

impl MsgPayload {
    fn required_capacity(&self) -> usize {
        (match self {
            MsgPayload::Message(msg) => required_str_capacity(msg),
            //  We start with 4 because we start by sending number of vec elements
            MsgPayload::Values(values) => {
                values.iter().fold(LEN_FIELD_SIZE, |acc, (name, value)| {
                    acc + required_str_capacity(name) + required_str_capacity(value)
                })
            }
        }) + size_of::<MsgPayloadVal>()
    }
}

// *** Message ***

#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct Message(Vec<u8>);

impl Message {
    pub fn new(filename: &str, line: u32, payload: MsgPayload) -> Self {
        let time = current_time();
        // This has to be made into a string as there doesn't seem to be a way to get any
        // sort of integral version out of it (at least not in stable)
        let thread_id = format!("{:?}", thread::current().id());

        // Msg length + time + thread id + filename len + line # + payload len
        let len = LEN_FIELD_SIZE // msg len
            + size_of::<u64>() // time
            + required_str_capacity(&thread_id)
            + required_str_capacity(filename)
            + size_of::<u32>() // line #
            + payload.required_capacity();

        let mut msg = Self(Vec::with_capacity(len));
        msg.write_u32(len as u32);
        msg.write_u64(time);
        msg.write_str(&thread_id);
        msg.write_str(filename);
        msg.write_u32(line);
        msg.write_payload(&payload);

        debug_assert_eq!(msg.0.len(), len, "Bad message length");
        msg
    }

    #[inline]
    fn new_bogus() -> Self {
        Self::new("<bogus>", 1, MsgPayload::Message("".to_string()))
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    #[inline]
    fn write_str(&mut self, s: &str) {
        self.write_u32(s.len() as u32);
        self.0.extend(s.as_bytes());
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.0.extend(i.to_be_bytes());
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.0.extend(i.to_be_bytes());
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0.extend(i.to_be_bytes());
    }

    fn write_payload(&mut self, payload: &MsgPayload) {
        match payload {
            MsgPayload::Message(msg) => {
                self.write_u8(MsgPayloadVal::Message as u8);
                self.write_str(msg);
            }
            MsgPayload::Values(values) => {
                self.write_u8(MsgPayloadVal::Values as u8);
                self.write_u32(values.len() as u32);

                for (name, value) in values {
                    self.write_str(name);
                    self.write_str(value);
                }
            }
        }
    }
}

// *** RemoteDebug ***

#[doc(hidden)]
pub struct RemoteDebug {
    sender: SyncSender<Message>,
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

    #[doc(hidden)]
    pub fn send_message(&self, filename: &str, line: u32, payload: MsgPayload) {
        // We have no good way to report errors, so just unwrap and panic, if needed
        // (can likely only happen if our thread panics freeing the receiver)
        self.sender
            .send(Message::new(filename, line, payload))
            .unwrap();
    }
}

impl Default for RemoteDebug {
    fn default() -> Self {
        Self::new(DEFAULT_PORT)
    }
}

// NOTE: This function isn't a part of RemoteDebug simply to make it a few less key strokes for the
// user in case they want to include this on every macro invocation.
/// Specify a custom port on the TCP socket when using the [msg] and [vals] macros
///
/// ```
/// let world = "world!";
/// rdbg::msg!(rdbg::port(5000), ["Hello {}", world]);
///
/// rdbg::vals!(rdbg::port(5000), [world, 1 + 1]);
/// ```
#[inline]
pub fn port(port: u16) -> RemoteDebug {
    RemoteDebug::new(port)
}

fn handle_connections(port: u16) -> SyncSender<Message> {
    let (sender, receiver) = sync_channel::<Message>(CHAN_MAX_MESSAGES);

    // Fill the channel the first time with bogus messages so that the first actual message will
    // block. This forces the program to wait until a viewer is connected avoiding a race condition.
    let msg = Message::new_bogus();
    for _ in 0..CHAN_MAX_MESSAGES {
        // We have no good way to report errors, so just unwrap and panic, if needed
        // (should never happen as there is no way receiver could be closed right now)
        sender.send(msg.clone()).unwrap();
    }

    thread::spawn(move || {
        let mut curr_msg = None;
        let mut first_time = true;

        loop {
            // We have no good way to report errors, so just unwrap and panic, if needed
            // (likely due to 'address in use' or 'permission denied', so we want to know about that
            // not mysteriously just not receive messages)
            let listener = TcpListener::bind((BIND_ADDR, port)).unwrap();

            // Per docs, 'incoming' will always return an entry
            if let Ok(mut stream) = listener.incoming().next().unwrap() {
                // Only on the very first time do we dump all our bogus messages before processing
                // the real ones
                if first_time {
                    for _ in 0..CHAN_MAX_MESSAGES {
                        // We have no good way to report errors, so just unwrap and panic, if needed
                        // (should never happen as there is no way sender could be closed right now)
                        receiver.recv().unwrap();
                    }

                    first_time = false;
                }

                process_stream(&mut stream, &receiver, &mut curr_msg);
            }
        }
    });

    sender
}

fn process_stream(
    stream: &mut TcpStream,
    receiver: &Receiver<Message>,
    curr_msg: &mut Option<Message>,
) {
    // If we hit an error writing out the version just return since we have no good way to report
    if write_to_stream(&WIRE_PROTOCOL_VERSION.to_be_bytes(), stream).is_err() {
        return;
    }

    loop {
        // If we were interrupted sending last message then resend otherwise wait for a new message
        let msg = match &curr_msg {
            Some(msg) => msg,
            None => {
                // We have no good way to report errors, so just unwrap and panic, if needed
                // (this is likely impossible since our SyncSender is in a global var)
                *curr_msg = Some(receiver.recv().unwrap());
                // Can't fail, stored above
                curr_msg.as_ref().unwrap()
            }
        };

        match write_to_stream(msg.as_slice(), stream) {
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
    let mut index = 0;

    // Keep writing until everything in the buffer has been written or we get an error
    while index < buffer.len() {
        match stream.write(&buffer[index..]) {
            Ok(wrote) => {
                index += wrote;
            }
            Err(err) => {
                return Err(err);
            }
        }
    }

    Ok(())
}
