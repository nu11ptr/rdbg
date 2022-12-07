use std::fmt::{Debug, Display, Formatter};
use std::io::Read;
use std::mem::size_of;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::str::Utf8Error;
use std::time::Duration;
use std::{io, thread};

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 13579;

const CONNECT_WAIT_TIME: u64 = 100; // Milliseconds
const BUFFER_SIZE: usize = 4096;
const LEN_FIELD_SIZE: usize = 4;
const WIRE_PROTOCOL_VERSION: u8 = 1;

#[repr(u8)]
enum MsgPayloadVal {
    Message = 1,
    Values = 2,
}

impl MsgPayloadVal {
    #[inline]
    fn from_buffer(buffer: &mut ByteBuffer) -> Result<MsgPayloadVal, Error> {
        buffer.read_u8()?.try_into()
    }
}

impl TryFrom<u8> for MsgPayloadVal {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(MsgPayloadVal::Message),
            2 => Ok(MsgPayloadVal::Values),
            _ => Err(Error::CorruptMsg),
        }
    }
}

struct ByteBuffer {
    buffer: Vec<u8>,
    idx: usize,
}

impl ByteBuffer {
    #[inline]
    fn new(capacity: usize) -> Self {
        Self::from_vec(Vec::with_capacity(capacity))
    }

    #[inline]
    fn from_vec(buffer: Vec<u8>) -> Self {
        Self { buffer, idx: 0 }
    }

    fn read_from_stream(&mut self, stream: &mut TcpStream, size: usize) -> io::Result<()> {
        self.buffer.resize(size, 0);
        stream.read_exact(&mut self.buffer)?;
        // We start over every type we read
        self.idx = 0;
        Ok(())
    }

    fn as_slice(&mut self, len: usize) -> Result<&[u8], Error> {
        if self.idx + len <= self.buffer.len() {
            self.idx += len;
            Ok(&self.buffer[(self.idx - len)..self.idx])
        } else {
            Err(Error::CorruptMsg)
        }
    }

    fn read_u8(&mut self) -> Result<u8, Error> {
        Ok(u8::from_be_bytes(
            self.as_slice(size_of::<u8>())?.try_into().unwrap(),
        ))
    }

    fn read_u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_be_bytes(
            self.as_slice(size_of::<u64>())?.try_into().unwrap(),
        ))
    }

    fn read_u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(
            self.as_slice(size_of::<u32>())?.try_into().unwrap(),
        ))
    }

    fn read_str(&mut self) -> Result<String, Error> {
        let len = self.read_u32()?;

        match std::str::from_utf8(self.as_slice(len as usize)?) {
            Ok(str) => Ok(str.to_string()),
            Err(err) => Err(Error::BadUtf8(err)),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum MsgPayload {
    /// A formatted string
    Message(String),
    /// A list of name/value pairs from expressions
    Values(Vec<(String, String)>),
}

impl MsgPayload {
    fn from_buffer(buffer: &mut ByteBuffer) -> Result<Self, Error> {
        match MsgPayloadVal::from_buffer(buffer)? {
            MsgPayloadVal::Message => {
                let s = buffer.read_str()?;
                Ok(MsgPayload::Message(s))
            }
            MsgPayloadVal::Values => {
                let len = buffer.read_u32()?;
                // TODO: Do we need to protect against VERY large values here? We will still check
                // bounds but not before a LOT of memory could be allocated
                let mut values = Vec::with_capacity(len as usize);

                for _ in 0..len {
                    let name = buffer.read_str()?;
                    let val = buffer.read_str()?;
                    values.push((name, val));
                }

                Ok(MsgPayload::Values(values))
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Message {
    pub time: u64,
    pub thread_id: String,
    pub filename: String,
    pub line: u32,
    pub payload: MsgPayload,
}

impl Message {
    fn from_buffer(buffer: &mut ByteBuffer) -> Result<Message, Error> {
        let time = buffer.read_u64()?;
        let thread_id = buffer.read_str()?;
        let filename = buffer.read_str()?;
        let line = buffer.read_u32()?;
        let payload = MsgPayload::from_buffer(buffer)?;

        Ok(Self {
            time,
            thread_id,
            filename,
            line,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use crate::{ByteBuffer, LEN_FIELD_SIZE};

    #[test]
    fn deserialize_msg() {
        let filename = file!();
        let line: u32 = line!();
        let message = "message".to_string();

        let raw_msg =
            rdbg::Message::new(filename, line, rdbg::MsgPayload::Message(message.clone()));

        let expected_msg = crate::Message {
            time: 42,
            thread_id: format!("{:?}", thread::current().id()),
            filename: filename.to_string(),
            line,
            payload: crate::MsgPayload::Message(message),
        };
        let mut buffer = ByteBuffer::from_vec(raw_msg.as_slice()[LEN_FIELD_SIZE..].to_vec());
        let mut actual_msg = crate::Message::from_buffer(&mut buffer).expect("Corrupt message");

        // Cheat on time since we have no way to know exact time
        actual_msg.time = expected_msg.time;
        assert_eq!(expected_msg, actual_msg);
    }

    #[test]
    fn deserialize_vals() {
        let filename = file!();
        let line: u32 = line!();
        let values = vec![("name1", "val1".to_string()), ("name2", "val2".to_string())];

        let raw_msg = rdbg::Message::new(filename, line, rdbg::MsgPayload::Values(values.clone()));

        let expected_msg = crate::Message {
            time: 42,
            thread_id: format!("{:?}", thread::current().id()),
            filename: filename.to_string(),
            line,
            payload: crate::MsgPayload::Values(
                values
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ),
        };
        let mut buffer = ByteBuffer::from_vec(raw_msg.as_slice()[LEN_FIELD_SIZE..].to_vec());
        let mut actual_msg = crate::Message::from_buffer(&mut buffer).expect("Corrupt message");

        // Cheat on time since we have no way to know exact time
        actual_msg.time = expected_msg.time;
        assert_eq!(expected_msg, actual_msg);
    }
}

pub enum Error {
    BadVersion,
    BadUtf8(Utf8Error),
    CorruptMsg,
}

impl Debug for Error {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BadVersion => f.write_str("This library only supports protocol version 1"),
            Error::BadUtf8(err) => std::fmt::Display::fmt(err, f),
            Error::CorruptMsg => f.write_str("The message payload was corrupted"),
        }
    }
}

impl std::error::Error for Error {}

pub enum Event {
    Connected(SocketAddr),
    Disconnected(SocketAddr),
    Message(Message),
    Error(Error),
}

pub struct MsgIterator {
    addr: SocketAddr,
    stream: Option<TcpStream>,
    buffer: ByteBuffer,
}

impl MsgIterator {
    #[inline]
    pub fn from_socket_addr<A: ToSocketAddrs<Iter = SocketAddr>>(addr: A) -> io::Result<Self> {
        Ok(Self::new(addr.to_socket_addrs()?))
    }

    #[inline]
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            stream: None,
            buffer: ByteBuffer::new(BUFFER_SIZE),
        }
    }
}

impl Default for MsgIterator {
    #[inline]
    fn default() -> Self {
        Self::new(SocketAddr::new(DEFAULT_ADDR.parse().unwrap(), DEFAULT_PORT))
    }
}

impl Iterator for MsgIterator {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.stream {
            Some(stream) => match self.buffer.read_from_stream(stream, LEN_FIELD_SIZE) {
                Ok(_) => {
                    // We know this is long enough - guaranteed by read above
                    let len = self.buffer.read_u32().unwrap();

                    match self.buffer.read_from_stream(stream, len as usize) {
                        Ok(_) => match Message::from_buffer(&mut self.buffer) {
                            Ok(msg) => Some(Event::Message(msg)),
                            Err(err) => {
                                self.stream = None;
                                Some(Event::Error(err))
                            }
                        },
                        Err(_) => {
                            self.stream = None;
                            Some(Event::Disconnected(self.addr))
                        }
                    }
                }
                Err(_) => {
                    self.stream = None;
                    Some(Event::Disconnected(self.addr))
                }
            },
            None => loop {
                if let Ok(mut stream) = TcpStream::connect(self.addr) {
                    match self.buffer.read_from_stream(&mut stream, size_of::<u8>()) {
                        // We know this is long enough - guaranteed by read above
                        Ok(_) if self.buffer.read_u8().unwrap() == WIRE_PROTOCOL_VERSION => {
                            self.stream = Some(stream);
                            return Some(Event::Connected(self.addr));
                        }
                        Ok(_) => return Some(Event::Error(Error::BadVersion)),
                        Err(_) => {
                            // No op
                        }
                    }
                }

                thread::sleep(Duration::from_millis(CONNECT_WAIT_TIME));
            },
        }
    }
}
