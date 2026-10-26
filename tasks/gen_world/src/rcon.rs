use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

pub const SERVERDATA_EXECCOMMAND: i32 = 2;
pub const SERVERDATA_AUTH: i32 = 3;
#[cfg(test)]
const SERVERDATA_RESPONSE_VALUE: i32 = 0;
const SERVERDATA_AUTH_RESPONSE: i32 = 2;

/// Vanilla splits RCON payloads at this many characters; a full-size packet
/// usually means another packet follows.
const MAX_PAYLOAD_CHARS: usize = 4096;
const MAX_BODY_BYTES: usize = MAX_PAYLOAD_CHARS * 4 + 10;
const MAX_PACKETS_PER_REPLY: usize = 64;
const DEFAULT_REPLY_DEADLINE: Duration = Duration::from_secs(600);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("RCON connection closed by server")]
    Closed,
    #[error("RCON authentication failed (check rcon.password)")]
    AuthFailed,
    #[error("malformed RCON packet: {0}")]
    Malformed(&'static str),
    #[error("timed out waiting for RCON response")]
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub request_id: i32,
    pub kind: i32,
    pub payload: String,
}

pub fn encode_packet(request_id: i32, kind: i32, payload: &str) -> Vec<u8> {
    let body_len = 4 + 4 + payload.len() + 2;
    let mut out = Vec::with_capacity(4 + body_len);
    out.extend_from_slice(&(body_len as i32).to_le_bytes());
    out.extend_from_slice(&request_id.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(payload.as_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

pub fn decode_packet(bytes: &[u8]) -> Result<Option<(Packet, usize)>, Error> {
    if bytes.len() < 4 {
        return Ok(None);
    }
    let body_len = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if body_len < 10 {
        return Err(Error::Malformed("packet length below minimum"));
    }
    if body_len as usize > MAX_BODY_BYTES {
        return Err(Error::Malformed("packet length above maximum"));
    }
    let total = 4 + body_len as usize;
    if bytes.len() < total {
        return Ok(None);
    }
    let request_id = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let kind = i32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let payload = String::from_utf8_lossy(&bytes[12..total - 2]).into_owned();
    Ok(Some((
        Packet {
            request_id,
            kind,
            payload,
        },
        total,
    )))
}

enum ReadOutcome {
    Packet(Packet),
    Idle,
    Closed,
}

pub struct RconClient {
    stream: TcpStream,
    addr: SocketAddr,
    next_request_id: i32,
    read_buf: Vec<u8>,
}

impl RconClient {
    pub fn connect(addr: SocketAddr, password: &str) -> Result<Self, Error> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
        stream.set_nodelay(true).ok();
        stream.set_read_timeout(Some(POLL_INTERVAL))?;
        let mut client = Self {
            stream,
            addr,
            next_request_id: 0,
            read_buf: Vec::new(),
        };
        client.authenticate(password)?;
        Ok(client)
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn authenticate(&mut self, password: &str) -> Result<(), Error> {
        self.next_request_id += 1;
        let request_id = self.next_request_id;
        let packet = encode_packet(request_id, SERVERDATA_AUTH, password);
        self.stream.write_all(&packet)?;

        let started = Instant::now();
        let mut seen = 0;
        loop {
            if started.elapsed() >= Duration::from_secs(120) {
                return Err(Error::TimedOut);
            }
            match self.read_packet()? {
                ReadOutcome::Idle => continue,
                ReadOutcome::Closed => return Err(Error::Closed),
                ReadOutcome::Packet(packet) => {
                    seen += 1;
                    if seen > 8 {
                        return Err(Error::Malformed("no auth response received"));
                    }
                    if packet.kind == SERVERDATA_AUTH_RESPONSE {
                        return if packet.request_id == -1 {
                            Err(Error::AuthFailed)
                        } else if packet.request_id == request_id {
                            Ok(())
                        } else {
                            Err(Error::Malformed("unexpected auth request id"))
                        };
                    }
                }
            }
        }
    }

    /// Send a command and collect its (possibly multi-packet) reply.
    ///
    /// Replies are synchronous with command execution, so long commands
    /// (`save-all flush`) legitimately take minutes; `deadline` bounds the
    /// wait for the first packet.
    pub fn run(&mut self, command: &str) -> Result<String, Error> {
        self.run_with_deadline(command, DEFAULT_REPLY_DEADLINE)
    }

    pub fn run_with_deadline(
        &mut self,
        command: &str,
        deadline: Duration,
    ) -> Result<String, Error> {
        self.next_request_id += 1;
        let request_id = self.next_request_id;
        let packet = encode_packet(request_id, SERVERDATA_EXECCOMMAND, command);
        self.stream.write_all(&packet)?;

        let started = Instant::now();
        let mut out = String::new();
        let mut packets = 0;
        loop {
            if started.elapsed() >= deadline {
                self.read_buf.clear();
                return Err(Error::TimedOut);
            }
            match self.read_packet()? {
                ReadOutcome::Idle => continue,
                ReadOutcome::Closed => {
                    self.read_buf.clear();
                    return Err(Error::Closed);
                }
                ReadOutcome::Packet(packet) => {
                    if packet.request_id != request_id {
                        continue;
                    }
                    packets += 1;
                    if packets > MAX_PACKETS_PER_REPLY {
                        return Err(Error::Malformed("too many packets in one reply"));
                    }
                    out.push_str(&packet.payload);
                    if packet.payload.chars().count() < MAX_PAYLOAD_CHARS {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    fn read_packet(&mut self) -> Result<ReadOutcome, Error> {
        loop {
            if self.read_buf.len() < 4 {
                match self.fill_read_buf(4)? {
                    PollRead::Complete => continue,
                    PollRead::Idle => return Ok(ReadOutcome::Idle),
                    PollRead::Closed => return Ok(ReadOutcome::Closed),
                }
            }

            let body_len = i32::from_le_bytes(self.read_buf[..4].try_into().unwrap());
            if body_len < 10 || body_len as usize > MAX_BODY_BYTES {
                return Err(Error::Malformed("implausible packet length"));
            }
            let total = 4 + body_len as usize;
            if self.read_buf.len() < total {
                match self.fill_read_buf(total)? {
                    PollRead::Complete => continue,
                    PollRead::Idle => return Ok(ReadOutcome::Idle),
                    PollRead::Closed => return Ok(ReadOutcome::Closed),
                }
            }

            let packet = match decode_packet(&self.read_buf[..total])? {
                Some((packet, consumed)) => {
                    debug_assert_eq!(consumed, total);
                    packet
                }
                None => return Err(Error::Malformed("decoded packet is incomplete")),
            };
            self.read_buf.drain(..total);
            return Ok(ReadOutcome::Packet(packet));
        }
    }

    fn fill_read_buf(&mut self, needed: usize) -> Result<PollRead, Error> {
        while self.read_buf.len() < needed {
            let mut chunk = [0u8; 8192];
            match self.stream.read(&mut chunk) {
                Ok(0) => return Ok(PollRead::Closed),
                Ok(n) => self.read_buf.extend_from_slice(&chunk[..n]),
                Err(err) if is_timeout(&err) => return Ok(PollRead::Idle),
                Err(err) => return Err(Error::Io(err)),
            }
        }
        Ok(PollRead::Complete)
    }
}

enum PollRead {
    Complete,
    Idle,
    Closed,
}

fn is_timeout(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_packet_codec() {
        let encoded = encode_packet(7, SERVERDATA_EXECCOMMAND, "/forceload add 0 0 15 15");
        let (packet, consumed) = decode_packet(&encoded).unwrap().unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(packet.request_id, 7);
        assert_eq!(packet.kind, SERVERDATA_EXECCOMMAND);
        assert_eq!(packet.payload, "/forceload add 0 0 15 15");
    }

    #[test]
    fn handles_empty_payload() {
        let encoded = encode_packet(-1, SERVERDATA_AUTH_RESPONSE, "");
        let (packet, consumed) = decode_packet(&encoded).unwrap().unwrap();
        assert_eq!(consumed, 14);
        assert_eq!(packet.request_id, -1);
        assert_eq!(packet.payload, "");
    }

    #[test]
    fn waits_for_complete_packets() {
        let encoded = encode_packet(1, SERVERDATA_RESPONSE_VALUE, "abc");
        assert_eq!(decode_packet(&encoded[..3]).unwrap(), None);
        assert_eq!(decode_packet(&encoded[..encoded.len() - 1]).unwrap(), None);
        assert!(decode_packet(&encoded).unwrap().is_some());
    }

    #[test]
    fn rejects_implausible_lengths() {
        let mut encoded = encode_packet(1, SERVERDATA_RESPONSE_VALUE, "x");
        encoded[0..4].copy_from_slice(&5i32.to_le_bytes());
        assert!(matches!(
            decode_packet(&encoded),
            Err(Error::Malformed("packet length below minimum"))
        ));
    }

    #[test]
    fn decodes_only_one_packet_at_a_time() {
        let first = encode_packet(1, SERVERDATA_RESPONSE_VALUE, "a");
        let second = encode_packet(2, SERVERDATA_RESPONSE_VALUE, "b");
        let mut both = first.clone();
        both.extend_from_slice(&second);
        let (packet, consumed) = decode_packet(&both).unwrap().unwrap();
        assert_eq!(packet.payload, "a");
        assert_eq!(consumed, first.len());
    }

    #[test]
    fn two_thousand_byte_reply_terminates_immediately() {
        let (mut client, mut server) = connected_pair();
        let reply = "x".repeat(2000);
        write_packet(&mut server, &reply);

        let started = Instant::now();
        let out = client
            .run_with_deadline("test", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, reply);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn multibyte_reply_uses_char_count() {
        let (mut client, mut server) = connected_pair();
        let reply = "é".repeat(1500);
        write_packet(&mut server, &reply);

        let started = Instant::now();
        let out = client
            .run_with_deadline("test", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, reply);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn full_chunks_concatenate() {
        let (mut client, mut server) = connected_pair();
        let first = "a".repeat(MAX_PAYLOAD_CHARS);
        let second = "b".repeat(MAX_PAYLOAD_CHARS);
        write_packet(&mut server, &first);
        write_packet(&mut server, &second);
        write_packet(&mut server, "");

        let out = client
            .run_with_deadline("test", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out.chars().count(), MAX_PAYLOAD_CHARS * 2);
        assert!(out.starts_with(&first));
        assert!(out.ends_with(&second));
    }

    #[test]
    fn deadline_does_not_accept_an_incomplete_chunked_reply() {
        let (mut client, mut server) = connected_pair();
        write_packet(&mut server, &"x".repeat(MAX_PAYLOAD_CHARS));

        assert!(matches!(
            client.run_with_deadline("test", Duration::from_millis(150)),
            Err(Error::TimedOut)
        ));
    }

    #[test]
    fn decodes_full_size_body() {
        let payload = "c".repeat(MAX_PAYLOAD_CHARS);
        let encoded = encode_packet(1, SERVERDATA_RESPONSE_VALUE, &payload);
        assert_eq!(encoded.len(), 4 + MAX_PAYLOAD_CHARS + 10);
        let (packet, consumed) = decode_packet(&encoded).unwrap().unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(packet.payload, payload);
    }

    #[test]
    fn resumes_after_partial_read() {
        let (mut client, mut server) = connected_pair();
        let payload = "partial reply";
        let encoded = encode_packet(1, SERVERDATA_RESPONSE_VALUE, payload);
        let split = encoded.len() / 2;
        server.write_all(&encoded[..split]).unwrap();
        server.flush().unwrap();
        std::thread::sleep(Duration::from_millis(150));
        server.write_all(&encoded[split..]).unwrap();
        server.flush().unwrap();

        let out = client
            .run_with_deadline("test", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, payload);
    }

    fn connected_pair() -> (RconClient, TcpStream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || listener.accept().unwrap().0);
        let stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let client = RconClient {
            stream,
            addr,
            next_request_id: 0,
            read_buf: Vec::new(),
        };
        (client, server.join().unwrap())
    }

    fn write_packet(stream: &mut TcpStream, payload: &str) {
        stream
            .write_all(&encode_packet(1, SERVERDATA_RESPONSE_VALUE, payload))
            .unwrap();
        stream.flush().unwrap();
    }
}
