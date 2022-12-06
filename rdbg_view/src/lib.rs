use std::fmt::{Debug, Display, Formatter};
use std::io::Read;
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
    fn from_buffer(buffer: &[u8]) -> Result<MsgPayloadVal, Error> {
        if !buffer.is_empty() {
            Ok(buffer[0].try_into()?)
        } else {
            Err(Error::CorruptMsg)
        }
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

fn read_u64(buffer: &[u8]) -> Result<u64, Error> {
    Ok(u64::from_be_bytes(
        buffer.try_into().map_err(|_| Error::CorruptMsg)?,
    ))
}

fn read_u32(buffer: &[u8]) -> Result<u32, Error> {
    Ok(u32::from_be_bytes(
        buffer.try_into().map_err(|_| Error::CorruptMsg)?,
    ))
}

fn read_str(buffer: &[u8]) -> Result<(String, usize), Error> {
    let len = read_u32(&buffer[0..])?;
    let next_idx = len as usize + LEN_FIELD_SIZE;

    if buffer.len() >= next_idx {
        match std::str::from_utf8(&buffer[LEN_FIELD_SIZE..next_idx]) {
            Ok(str) => Ok((str.to_string(), next_idx)),
            Err(err) => Err(Error::BadUtf8(err)),
        }
    } else {
        Err(Error::CorruptMsg)
    }
}

#[derive(Debug)]
pub enum MsgPayload {
    /// A formatted string
    Message(String),
    /// A list of name/value pairs from expressions
    Values(Vec<(String, String)>),
}

impl MsgPayload {
    fn from_buffer(buffer: &[u8]) -> Result<Self, Error> {
        match MsgPayloadVal::from_buffer(buffer)? {
            MsgPayloadVal::Message => {
                let (s, _) = read_str(buffer)?;
                Ok(MsgPayload::Message(s))
            }
            MsgPayloadVal::Values => {
                let len = read_u32(buffer)?;
                // TODO: Do we need to protect against VERY large values here? We will still check
                // bounds but not before a LOT of memory could be allocated
                let mut values = Vec::with_capacity(len as usize);

                let mut idx = LEN_FIELD_SIZE;
                for _ in 0..len {
                    let (name, next_idx) = read_str(&buffer[idx..])?;
                    let (val, next_idx) = read_str(&buffer[next_idx..])?;
                    values.push((name, val));
                    idx = next_idx;
                }

                Ok(MsgPayload::Values(values))
            }
        }
    }
}

#[derive(Debug)]
pub struct Message {
    pub time: u64,
    pub thread_id: String,
    pub filename: String,
    pub line: u32,
    pub payload: MsgPayload,
}

impl Message {
    fn from_buffer(buffer: &[u8]) -> Result<Message, Error> {
        let time = read_u64(&buffer[0..])?;
        let (thread_id, next_idx) = read_str(&buffer[8..])?;
        let (filename, next_idx) = read_str(&buffer[next_idx..])?;
        let line = read_u32(&buffer[next_idx..])?;
        let payload = MsgPayload::from_buffer(&buffer[next_idx + 4..])?;

        Ok(Self {
            time,
            thread_id,
            filename,
            line,
            payload,
        })
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
    buffer: Vec<u8>,
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
            buffer: Vec::with_capacity(BUFFER_SIZE),
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
            Some(stream) => match read_from_stream(&mut self.buffer, stream, LEN_FIELD_SIZE) {
                Ok(_) => {
                    let len = u32::from_be_bytes((&*self.buffer).try_into().unwrap());

                    match read_from_stream(&mut self.buffer, stream, len as usize) {
                        Ok(_) => match Message::from_buffer(&self.buffer) {
                            Ok(msg) => Some(Event::Message(msg)),
                            Err(_) => {
                                self.stream = None;
                                Some(Event::Disconnected(self.addr))
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
                    match read_from_stream(&mut self.buffer, &mut stream, 1) {
                        Ok(_) if self.buffer[0] == WIRE_PROTOCOL_VERSION => {
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

fn read_from_stream(buffer: &mut Vec<u8>, stream: &mut TcpStream, size: usize) -> io::Result<()> {
    buffer.resize(size, 0);
    stream.read_exact(buffer)
}
