use packet::route::{MutableIfInfoPacket,IfAddrPacket,MutableIfAddrPacket,RtAttrIterator,RtAttrPacket,MutableRtAttrPacket,RtAttrMtuPacket};
use packet::route::link::Link;
use packet::netlink::{MutableNetlinkPacket,NetlinkPacket,NetlinkErrorPacket};
use packet::netlink::{NLM_F_ACK,NLM_F_REQUEST,NLM_F_DUMP,NLM_F_MATCH,NLM_F_EXCL,NLM_F_CREATE};
use packet::netlink::{NLMSG_NOOP,NLMSG_ERROR,NLMSG_DONE,NLMSG_OVERRUN};
use packet::netlink::{NetlinkBuf,NetlinkBufIterator,NetlinkReader,NetlinkRequestBuilder};
use socket::{NetlinkSocket,NetlinkProtocol};
use packet::netlink::NetlinkConnection;
use pnet::packet::MutablePacket;
use pnet::packet::Packet;
use pnet::packet::PacketSize;
use pnet::util::MacAddr;
use libc;
use std::io::{Read,Cursor,self};
use byteorder::{LittleEndian, BigEndian, ReadBytesExt};
use std::net::{Ipv4Addr,Ipv6Addr};

pub const RTM_NEWADDR: u16 = 20;
pub const RTM_DELADDR: u16 = 21;
pub const RTM_GETADDR: u16 = 22;


/* link flags */
bitflags! {
    pub flags IfAddrFlags: u8 {
        const SECONDARY = 0x01,
        const TEMPORARY = SECONDARY.bits,
        const NODAD = 0x02,
        const OPTIMISTIC = 0x04,
        const DADFAILED = 0x08,
        const HOMEADDRESS = 0x10,
        const DEPRECATED = 0x20,
        const TENTATIVE = 0x40,
        const PERMANENT = 0x80,
    }
}

pub const IFA_UNSPEC: u16 = 0;
pub const IFA_ADDRESS: u16 = 1;
pub const IFA_LOCAL: u16 = 2;
pub const IFA_LABEL: u16 = 3;
pub const IFA_BROADCAST: u16 = 4;
pub const IFA_ANYCAST: u16 = 5;
pub const IFA_CACHEINFO: u16 = 6;
pub const IFA_MULTICAST: u16 = 7;

pub struct AddrsIterator<R: Read> {
    iter: NetlinkBufIterator<R>,
}

impl<R: Read> Iterator for AddrsIterator<R> {
    type Item = Addr;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(pkt) => {
                let kind = pkt.get_packet().get_kind();
                if kind != RTM_NEWADDR {
                    return None;
                }
                return Some(Addr { packet: pkt });
            },
            None => None,
        }
    }
}

#[derive(Debug,Eq,PartialEq)]
pub enum IpAddr {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

#[derive(Clone,Debug)]
pub struct Addr {
    packet: NetlinkBuf,
}

struct AddrManager<'a> {
    conn: &'a mut NetlinkConnection,
}

impl<'a> AddrManager<'a> {
    fn new(conn: &'a mut NetlinkConnection) -> Self {
        AddrManager { conn: conn }
    }

    pub fn get_link_addrs<'b>(&'a mut self, link: &'b Link) -> Box<Iterator<Item = Addr> + 'a> {
        let idx = link.get_index();
        Box::new(self.make_addr_iter(None).filter(move |addr| addr.with_ifaddr(|ifa| ifa.get_index() == idx)))
    }

    pub fn iter_addrs(&'a mut self) -> AddrsIterator<&'a mut NetlinkConnection> {
        self.make_addr_iter(None)
    }

    fn make_addr_iter(&'a mut self, family: Option<u8>) -> AddrsIterator<&'a mut NetlinkConnection> {
        let mut buf = vec![0; MutableIfInfoPacket::minimum_packet_size()];
        let req = NetlinkRequestBuilder::new(RTM_GETADDR, NLM_F_DUMP)
            .append({
                let mut ifinfo = MutableIfInfoPacket::new(&mut buf).unwrap();
                ifinfo.set_family(family.unwrap_or(0));
                ifinfo
            }).build();
        let mut reply = self.conn.send(req.get_packet());
        AddrsIterator { iter: reply.into_iter() }
    }
}

impl Addr {
    pub fn get_family(&self) -> u8 {
        self.with_ifaddr(|ifa| ifa.get_family())
    }

    pub fn get_flags(&self) -> u8 {
        self.with_ifaddr(|ifa| ifa.get_flags())
    }

    pub fn get_prefix_len(&self) -> u8 {
        self.with_ifaddr(|ifa| ifa.get_prefix_len())
    }

    pub fn get_scope(&self) -> u8 {
        self.with_ifaddr(|ifa| ifa.get_flags())
    }

    pub fn get_index(&self) -> u32 {
        self.with_ifaddr(|ifa| ifa.get_index())
    }

    pub fn get_ip(&self) -> Option<IpAddr> {
        let family = self.with_ifaddr(|ifa| ifa.get_family());
        self.with_rta(IFA_ADDRESS, |rta| {
            Self::ip_from_family_and_bytes(family, rta.payload())
        })
    }

    pub fn get_local_ip(&self) -> Option<IpAddr> {
        let family = self.with_ifaddr(|ifa| ifa.get_family());
        self.with_rta(IFA_LOCAL, |rta| {
            Self::ip_from_family_and_bytes(family, rta.payload())
        })
    }

    pub fn get_broadcast_ip(&self) -> Option<IpAddr> {
        let family = self.with_ifaddr(|ifa| ifa.get_family());
        self.with_rta(IFA_BROADCAST, |rta| {
            Self::ip_from_family_and_bytes(family, rta.payload())
        })
    }

    // helper methods
    fn with_packet<T,F>(&self, cb: F) -> T
        where F: Fn(NetlinkPacket) -> T {
        cb(self.packet.get_packet())
    }

    fn with_ifaddr<T,F>(&self, cb: F) -> T
        where F: Fn(IfAddrPacket) -> T {
        self.with_packet(|pkt|
            cb(IfAddrPacket::new(pkt.payload()).unwrap())
        )
    }

    fn with_rta_iter<T,F>(&self, cb: F) -> T
        where F: Fn(RtAttrIterator) -> T {
            self.with_ifaddr(|ifa| {
                cb(RtAttrIterator::new(ifa.payload()))
            })
    }

    fn with_rta<T,F>(&self, rta_type: u16, cb: F) -> Option<T>
        where F: Fn(RtAttrPacket) -> T {
        self.with_rta_iter(|mut rti| {
            rti.find(|rta| rta.get_rta_type() == rta_type).map(|rta| cb(rta))
        })
    }

    fn ip_from_family_and_bytes(family: u8, bytes: &[u8]) -> IpAddr {
        let mut cur = Cursor::new(bytes);
        match family {
            2 /* AF_INET */ => IpAddr::V4(Ipv4Addr::from(cur.read_u32::<BigEndian>().unwrap())),
            10 /* AF_INET6 */ => {
                let mut ip6addr: [u8;16] = [0;16];
                &mut ip6addr[..].copy_from_slice(bytes);
                IpAddr::V6(Ipv6Addr::from(ip6addr))
            },
            _ => {
                panic!("not implemented")
            }
        }
    }

    fn dump_addr(msg: NetlinkPacket) {
        use std::ffi::CStr;
        if msg.get_kind() != RTM_NEWADDR {
            return;
        }
        //println!("NetLink pkt {:?}", msg);
        if let Some(ifa) = IfAddrPacket::new(&msg.payload()[0..]) {
            println!("├ ifa: {:?}", ifa);
            let payload = &ifa.payload()[0..];
            let iter = RtAttrIterator::new(payload);
            for rta in iter {
                match rta.get_rta_type() {
                    IFA_ADDRESS | IFA_LOCAL | IFA_BROADCAST => {
                        let mut cur = Cursor::new(rta.payload());
                        match rta.get_rta_type() {
                            IFA_ADDRESS => print!(" ├ ADDR: "),
                            IFA_LOCAL => print!(" ├ LOCAL: "),
                            IFA_BROADCAST => print!(" ├ BROADCAST: "),
                            _ => unreachable!(),
                        }
                        match ifa.get_family() {
                            2 => {
                                println!("{}", Ipv4Addr::from(cur.read_u32::<BigEndian>().unwrap()));
                            },
                            10 => {
                                let mut ip6addr: [u8;16] = [0;16];
                                &mut ip6addr[..].copy_from_slice(rta.payload());
                                println!("{}", Ipv6Addr::from(ip6addr));
                            },
                            _ => {
                                println!("{:?}", rta.payload());
                            }
                        }
                    },
                    /*
                    IFA_LOCAL => {
                        println!(" ├ LOCAL: {:?}", rta.payload());
                    },
                    IFA_BROADCAST => {
                        println!(" ├ BROADCAST: {:?}", rta.payload());
                    },
                    */
                    IFA_LABEL => {
                        println!(" ├ LABEL: {:?}", CStr::from_bytes_with_nul(rta.payload()));
                    },
                    IFA_CACHEINFO => {
                        println!(" ├ CACHEINFO: {:?}", rta.payload());
                    },
                    _ => println!(" ├ {:?}", rta),
                }
            }
        }
    }

    pub fn iter_addrs(conn: &mut NetlinkConnection) -> AddrsIterator<&mut NetlinkConnection> {
        let mut buf = vec![0; MutableIfInfoPacket::minimum_packet_size()];
        let req = NetlinkRequestBuilder::new(RTM_GETADDR, NLM_F_DUMP)
        .append({
            let mut ifinfo = MutableIfInfoPacket::new(&mut buf).unwrap();
            ifinfo.set_family(0 /* AF_UNSPEC */);
            ifinfo
        }).build();
        let mut reply = conn.send(req.get_packet());
        AddrsIterator { iter: reply.into_iter() }
    }
}

#[test]
fn dump_addrs() {
    let mut conn = NetlinkConnection::new();
    let mut addrs = AddrManager::new(&mut conn);
    for addr in addrs.iter_addrs() {
        Addr::dump_addr(addr.packet.get_packet());
        println!("{:?}", addr.get_ip());
    }
}

#[test]
fn check_lo_addr() {
    use packet::route::link::LinkManager;
    let mut conn = NetlinkConnection::new();
    let lo = LinkManager::new(&mut conn).get_link_by_name("lo").unwrap();
    let mut addrs = AddrManager::new(&mut conn);
    let mut addrs = addrs.get_link_addrs(&lo);
    assert!(addrs.find(|addr| addr.get_ip() == Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))).is_some());
}