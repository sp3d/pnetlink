use ::socket::{NetlinkSocket,NetlinkProtocol};
use libc;
use std::mem;
use std::io;
use std::io::{Read,BufRead,BufReader};
use std::marker::PhantomData;
use pnet::packet::{Packet,PacketSize,FromPacket};

include!(concat!(env!("OUT_DIR"), "/netlink.rs"));

bitflags! {
    pub flags NetlinkMsgFlags: u16 {
        /* It is request message. 	*/
        const NLM_F_REQUEST = 1,
        /* Multipart message, terminated by NLMSG_DONE */
        const NLM_F_MULTI = 2,
        /* Reply with ack, with zero or error code */
        const NLM_F_ACK = 4,
        /* Echo this request 		*/
        const NLM_F_ECHO = 8,
        /* Dump was inconsistent due to sequence change */
        const NLM_F_DUMP_INTR = 16,

        /* Modifiers to GET request */
        const NLM_F_ROOT =	0x100,	/* specify tree	root	*/
        const NLM_F_MATCH = 0x200,	/* return all matching	*/
        const NLM_F_ATOMIC = 0x400,	/* atomic GET		*/
        const NLM_F_DUMP =	(NLM_F_ROOT.bits|NLM_F_MATCH.bits),

        /* Modifiers to NEW request */
        const NLM_F_REPLACE = 0x100,   /* Override existing            */
        const NLM_F_EXCL =    0x200,   /* Do not touch, if it exists   */
        const NLM_F_CREATE =  0x400,   /* Create, if it does not exist */
        const NLM_F_APPEND =  0x800,   /* Add to end of list           */
    }
}

impl NetlinkMsgFlags {
    pub fn new(val: u16) -> Self {
        NetlinkMsgFlags::from_bits_truncate(val)
    }
}

/* message types */
pub const NLMSG_NOOP: u16 = 1;
pub const NLMSG_ERROR: u16 = 2;
pub const NLMSG_DONE: u16 = 3;
pub const NLMSG_OVERRUN: u16 = 4;

fn align(len: usize) -> usize {
    const RTA_ALIGNTO: usize = 4;

    ((len)+RTA_ALIGNTO-1) & !(RTA_ALIGNTO-1)
}

impl<'a> NetlinkIterable<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        NetlinkIterable { buf: buf }
    }
}

#[test]
fn read_ip_link_dump() {
    use std::fs::File;
    use std::io::prelude;
    use std::io::BufReader;
    use std::io::BufRead;
    use std::io::Read;

    let f = File::open("dumps/ip_link.bin").unwrap();
    let mut r = BufReader::new(f);
    let mut data = vec![];
    r.read_to_end(&mut data).unwrap();

    let it = NetlinkIterable::new(&data);
    for pkt in it {
        println!("{:?}", pkt);
    }
}

#[test]
fn read_ip_link_dump_2() {
    use std::fs::File;
    use std::io::prelude;
    use std::io::BufReader;
    use std::io::BufRead;
    use std::io::Read;

    let f = File::open("dumps/ip_link.bin").unwrap();
    let mut r = BufReader::new(f);
    let mut reader = NetlinkReader::new(r);
    while let Ok(Some(pkt)) = reader.read_netlink() {
        let pkt = pkt.get_packet();
        println!("{:?}", pkt);
        if pkt.get_kind() == NLMSG_DONE {
            break;
        }
    }
}

#[test]
fn read_ip_link_sock() {
    use std::fs::File;
    use std::io::prelude;
    use std::io::BufReader;
    use std::io::BufRead;
    use std::io::Read;

    let mut r = NetlinkSocket::bind(NetlinkProtocol::Route, 0 as u32).unwrap();
    let mut reader = NetlinkReader::new(r);
    while let Ok(Some(pkt)) = reader.read_netlink() {
        let pkt = pkt.get_packet();
        println!("{:?}", pkt);
    }
}

#[derive(Debug,Clone)]
pub struct NetlinkBuf {
    data: Vec<u8>,
}

impl NetlinkBuf {
    fn new(data: &[u8]) -> Self {
        NetlinkBuf { data: data.to_owned() }
    }

    pub fn get_packet(&self) -> NetlinkPacket {
        NetlinkPacket::new(&self.data[..]).unwrap()
    }
}

impl From<Vec<u8>> for NetlinkBuf {
    fn from(v: Vec<u8>) -> Self {
        NetlinkBuf { data: v }
    }
}

pub struct NetlinkReader<R: Read> {
    reader: R,
    buf: Vec<u8>,
    read_at: usize,
    state: NetlinkReaderState,
}

enum NetlinkReaderState {
    Done,
    NeedMore,
    Error,
    Parsing,
}

impl<R: Read> NetlinkReader<R> {
    fn new(reader: R) -> Self {
        NetlinkReader {
            reader: reader,
            buf: vec![],
            read_at: 0,
            state: NetlinkReaderState::NeedMore,
        }
    }
}

impl<R: Read> ::std::iter::IntoIterator for NetlinkReader<R> {
    type Item = NetlinkBuf;
    type IntoIter = NetlinkBufIterator<R>;

    fn into_iter(self) -> Self::IntoIter {
        NetlinkBufIterator { reader: self }
    }
}

impl<R: Read> NetlinkReader<R> {
    pub fn read_netlink(&mut self) -> io::Result<Option<NetlinkBuf>> {
        loop {
            match self.state {
                NetlinkReaderState::NeedMore => {
                    let mut buf = [0; 4096];
                    match self.reader.read(&mut buf) {
                        Ok(0) => {
                            self.state = NetlinkReaderState::Done;
                            return Ok(None);
                        },
                        Ok(len) =>{
                            self.buf.extend_from_slice(&buf[0..len]);
                        },
                        Err(e) => {
                            self.state = NetlinkReaderState::Error;
                            return Err(e);
                        }
                    }
                },
                NetlinkReaderState::Done => return Ok(None),
                NetlinkReaderState::Error => return Ok(None),
                NetlinkReaderState::Parsing => { },
            }
            loop {
                if let Some(pkt) = NetlinkPacket::new(&self.buf[self.read_at..]) {
                    let len = align(pkt.get_length() as usize);
                    if len == 0 {
                        return Ok(None);
                    }
                    match pkt.get_kind() {
                        NLMSG_ERROR => {
                            self.state = NetlinkReaderState::Error;
                        },
                        NLMSG_OVERRUN => {
                            panic!("overrun!");
                        },
                        NLMSG_DONE => {
                            self.state = NetlinkReaderState::Done;
                        },
                        NLMSG_NOOP => {
                            println!("noop")
                        },
                        _ => {
                            self.state = NetlinkReaderState::Parsing;
                        },
                    }
                    let slot = NetlinkBuf::new(&self.buf[self.read_at..self.read_at + pkt.get_length() as usize]);
                    self.read_at += len;
                    return Ok(Some(slot));
                } else {
                    self.state = NetlinkReaderState::NeedMore;
                    break;
                }
            }
        }
    }
}

pub struct NetlinkBufIterator<R: Read> {
    reader: NetlinkReader<R>,
}

impl<R: Read> Iterator for NetlinkBufIterator<R> {
    type Item = NetlinkBuf;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_netlink() {
            Ok(Some(slot)) => Some(slot),
            _ => None,
        }
    }
}

pub struct NetlinkConnection {
    sock: NetlinkSocket,
}

impl NetlinkConnection {
    pub fn new() -> Self {
        NetlinkConnection {
            sock: NetlinkSocket::bind(NetlinkProtocol::Route, 0 as u32).unwrap(),
        }
    }

    pub fn send<'a,'b>(&'a mut self, msg: NetlinkPacket<'b>) -> NetlinkReader<&'a mut NetlinkConnection> {
        self.sock.send(msg.packet()).unwrap();
        NetlinkReader::new(self)
    }
}

impl ::std::io::Read for NetlinkConnection {
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        self.sock.read(buf)
    }
}

pub struct NetlinkRequestBuilder {
    data: Vec<u8>,
}

impl NetlinkRequestBuilder {
    pub fn new(kind: u16, flags: NetlinkMsgFlags) -> Self {
        let len = MutableNetlinkPacket::minimum_packet_size();
        let mut data = vec![0; len];
        {
            let mut pkt = MutableNetlinkPacket::new(&mut data).unwrap();
            pkt.set_length(len as u32);
            pkt.set_kind(kind);
            pkt.set_flags(flags | NLM_F_REQUEST);
        }
        NetlinkRequestBuilder {
            data: data,
        }
    }

    pub fn append<P: PacketSize + Packet>(mut self, data: P) -> Self {
        let data = data.packet();
        let len = data.len();
        {
            let mut pkt = MutableNetlinkPacket::new(&mut self.data).unwrap();
            let len = pkt.get_length();
            pkt.set_length(len + len as u32);
        }
        self.data.extend_from_slice(data);
        self
    }

    pub fn build(self) -> NetlinkBuf {
        NetlinkBuf::from(self.data)
    }
}