use std::io::Write;
use std::mem::size_of;
use std::net::{TcpListener, TcpStream};
use std::process::exit;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{io, thread};

#[cfg(feature = "insecure-remote")]
const BIND_ADDR: &str = "0.0.0.0";
#[cfg(not(feature = "insecure-remote"))]
const BIND_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 13579;

const CHAN_MAX_MESSAGES: usize = 32;
const LEN_FIELD_SIZE: usize = size_of::<u32>();
const WIRE_PROTOCOL_VERSION: u8 = 1;
const THREAD_ID_PREFIX: &str = "ThreadId";

static REMOTE_DEBUG: Mutex<Option<RemoteDebug>> = Mutex::new(None);

// *** msg / vals macros ***

/// Send a debug message to the remote viewer
///
/// ```dontrun
/// // Default port
/// let world = "world!";
/// rdbg::msg!("Hello {}", world);
/// flush();
///
/// // Custom port
/// let debug = rdbg::port(5000);
/// rdbg::msg!(&debug, ["Hello {}", world]);
/// debug.flush();
/// ```
#[cfg(feature = "enabled")]
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

#[cfg(not(feature = "enabled"))]
#[macro_export]
macro_rules! msg {
    ($port:expr, [ $($arg:tt)* ]) => {};
    ($($arg:tt)*) => {};
}

/// Send debug expression name/value pairs to the remote viewer
///
/// ```dontrun
/// // Default port
/// let world = "world!";
/// rdbg::vals!(world, 1 + 1);
/// flush();
///
/// // Custom port
/// let debug = rdbg::port(5000);
/// rdbg::vals!(&debug, [world, 1 + 1]);
/// debug.flush();
/// ```
#[cfg(feature = "enabled")]
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

#[cfg(not(feature = "enabled"))]
#[macro_export]
macro_rules! vals {
    ($port:expr, [ $($value:expr),+ $(,)? ]) => {};
    ($($value:expr),+ $(,)?) => {};
}

// *** Message related functions ***

fn current_thread() -> String {
    // This has to be made into a string as there doesn't seem to be a way to get any
    // sort of integral version out of it (at least not in stable)
    parse_thread_id(format!("{:?}", thread::current().id()))
}

fn parse_thread_id(thread_id: String) -> String {
    // We optimistically assume the current format, but if it isn't just return
    // the initial string as-is
    let mut split = thread_id.split(&['(', ')']);
    if split.next() == Some(THREAD_ID_PREFIX) {
        match split.next() {
            Some(thread_id) if split.next() == Some("") => thread_id.to_string(),
            _ => thread_id,
        }
    } else {
        thread_id
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_thread_id;

    #[test]
    fn parse_thread_current() {
        assert_eq!(parse_thread_id("ThreadId(1)".to_string()), "1".to_string());
    }

    #[test]
    fn parse_thread_changed() {
        assert_eq!(
            parse_thread_id("Thread(1)".to_string()),
            "Thread(1)".to_string()
        );
    }
}

fn current_time() -> u64 {
    // This can only really fail if time goes to before the epoch, which likely isn't possible
    // on today's system clocks
    // While this returns a u128, u64 ought to be large enough to hold all ms since the epoch
    // for our lifetimes
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
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
        let thread_id = current_thread();

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

// *** Event ***

enum Event {
    NewMessage(Message),
    Flush,
}

// *** Flushed ***

#[derive(Clone)]
struct Flushed(Arc<(Condvar, Mutex<bool>)>);

impl Flushed {
    #[inline]
    fn new() -> Self {
        Self(Arc::new((Default::default(), Mutex::new(false))))
    }

    fn flushed(&self) {
        let (var, lock) = &*self.0;
        // Panic if mutex is poisoned
        let mut flushed = lock.lock().unwrap();
        *flushed = true;
        var.notify_one();
    }

    fn flush_and_wait(&self, sender: &SyncSender<Event>) {
        match sender.send(Event::Flush) {
            Ok(_) => {
                let (var, lock) = &*self.0;
                // Panic if mutex is poisoned
                let mut flushed = lock.lock().unwrap();

                while !*flushed {
                    // Panic if mutex is poisoned
                    flushed = var.wait(flushed).unwrap();
                }

                // Reset before releasing lock
                *flushed = false;
            }
            Err(err) => {
                eprintln!("Unable to send quit event: {err}");
            }
        }
    }
}

// *** RemoteDebug ***

#[doc(hidden)]
#[derive(Clone)]
pub struct RemoteDebug {
    sender: SyncSender<Event>,
    flush: Flushed,
}

impl RemoteDebug {
    fn from_sender(sender: SyncSender<Event>) -> Self {
        Self {
            sender,
            flush: Flushed::new(),
        }
    }

    fn from_port(port: u16) -> Self {
        // Panic if mutex is poisoned
        let remote_debug = &mut *REMOTE_DEBUG.lock().unwrap();

        // If our global var is already inited, just return it otherwise do one time thread creation
        match remote_debug {
            Some(remote_debug) => remote_debug.clone(),
            None => {
                let debug = handle_connections(port);
                *remote_debug = Some(debug.clone());
                debug
            }
        }
    }

    pub fn send_message(&self, filename: &str, line: u32, payload: MsgPayload) {
        if let Err(err) = self
            .sender
            .send(Event::NewMessage(Message::new(filename, line, payload)))
        {
            eprintln!("Unable to send new message event: {err}");
        }
    }

    #[inline]
    pub fn flush(&self) {
        self.flush.flush_and_wait(&self.sender);
    }
}

impl Default for RemoteDebug {
    fn default() -> Self {
        Self::from_port(DEFAULT_PORT)
    }
}

// NOTE: This function isn't a part of RemoteDebug simply to make it a few less key strokes for the
// user in case they want to include this on every macro invocation.
/// Specify a custom port for the TCP socket to listen on when using the [msg] and [vals] macros.
///
/// NOTE: The first time this function or [msg] or [vals] macros are processed determines the port.
/// Once it is established it will not change no matter what `port` value is used.
///
/// ```dontrun
/// let world = "world!";
/// rdbg::msg!(rdbg::port(5000), ["Hello {}", world]);
///
/// rdbg::vals!(rdbg::port(5000), [world, 1 + 1]);
/// rdbg::flush();
/// ```
#[cfg(feature = "enabled")]
#[inline]
pub fn port(port: u16) -> RemoteDebug {
    RemoteDebug::from_port(port)
}

#[cfg(not(feature = "enabled"))]
#[inline]
pub fn port(_port: u16) {}

/// Flush msg/val queue for default port
///
/// NOTE: The first time this function or [msg] or [vals] macros are processed determines the port.
/// Once it is established it will not change even if custom port macros or functions are called.
///
/// ```dontrun
/// let world = "world!";
/// rdbg::msg!(rdbg::port(5000), ["Hello {}", world]);
///
/// rdbg::vals!(rdbg::port(5000), [world, 1 + 1]);
/// rdbg::flush();
/// ```
#[cfg(feature = "enabled")]
#[inline]
pub fn flush() {
    RemoteDebug::default().flush();
}

#[cfg(not(feature = "enabled"))]
#[inline]
pub fn flush() {}

// *** Connection related functions ***

fn handle_connections(port: u16) -> RemoteDebug {
    let (sender, receiver) = sync_channel::<Event>(CHAN_MAX_MESSAGES);
    let debug = RemoteDebug::from_sender(sender);
    let debug_clone = debug.clone();

    thread::spawn(move || {
        let mut curr_msg = None;

        loop {
            // We have no good way to report errors, so just unwrap and panic, if needed
            // (likely due to 'address in use' or 'permission denied', so we want to know about that
            // not mysteriously just not receive messages)
            match TcpListener::bind((BIND_ADDR, port)) {
                Ok(listener) => {
                    // Per docs, 'incoming' will always return an entry
                    if let Ok(mut stream) = listener.incoming().next().unwrap() {
                        if process_stream(&mut stream, &receiver, &mut curr_msg, &debug_clone) {
                            // Quit signalled - we are done
                            break;
                        }
                    }
                }
                Err(err) => {
                    eprintln!("Unable to listen on {BIND_ADDR}:{port}: {err}");
                    // We exit instead of panic because this is a separate thread. We want it very
                    // obvious if for some reason it can't listen on this port so we exit immediately
                    exit(1);
                }
            }
        }
    });

    debug
}

fn process_stream(
    stream: &mut TcpStream,
    receiver: &Receiver<Event>,
    curr_msg: &mut Option<Message>,
    debug: &RemoteDebug,
) -> bool {
    // If we hit an error writing out the version just return since we have no good way to report
    if write_to_stream(&WIRE_PROTOCOL_VERSION.to_be_bytes(), stream).is_err() {
        return false;
    }

    loop {
        // If we were interrupted sending last message then resend otherwise wait for a new message
        let msg = match &curr_msg {
            Some(msg) => msg,
            None => {
                // We have no good way to report errors, so just unwrap and panic, if needed
                // (this is likely impossible since our SyncSender is in a global var)
                match receiver.recv().unwrap() {
                    Event::NewMessage(msg) => {
                        *curr_msg = Some(msg);
                        // Can't fail, stored above
                        curr_msg.as_ref().unwrap()
                    }
                    Event::Flush => {
                        debug.flush.flushed();
                        continue;
                    }
                }
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

    false
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
