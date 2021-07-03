//! Rules operations
use packet::route::{FibRulePacket,MutableIfInfoPacket,RtAttrIterator};
use packet::netlink::{NetlinkPacket};
use packet::netlink::NetlinkMsgFlags;
use packet::netlink::{NetlinkBufIterator,NetlinkRequestBuilder};
use packet::netlink::NetlinkConnection;
use pnet::packet::Packet;

/*
use socket::{NetlinkSocket,NetlinkProtocol};
use packet::netlink::{NetlinkReader};
use packet::netlink::{MutableNetlinkPacket,NetlinkErrorPacket};
use packet::route::{MutableRtMsgPacket,RtAttrPacket,MutableRtAttrPacket};
use pnet::packet::MutablePacket;
use libc;
use pnet::util::MacAddr;
use pnet::packet::PacketSize;
use packet::route::link::Link;
use std::net::Ipv4Addr;
*/

use std::io::{Read,Cursor};
use byteorder::{LittleEndian, ReadBytesExt};

pub const RTM_NEWRULE: u16 = 32;
pub const RTM_DELRULE: u16 = 33;
pub const RTM_GETRULE: u16 = 34;

pub const FRA_UNSPEC: u16 = 0;
pub const FRA_DST: u16 = 1;        /* destination address */
pub const FRA_SRC: u16 = 2;        /* source address */
pub const FRA_IFNAME: u16 = 3;    /* interface name */
pub const FRA_GOTO: u16 = 4;       /* target to jump to (FR_ACT_GOTO) */
pub const FRA_UNUSED2: u16 = 5;
pub const FRA_PRIORITY: u16 = 6;   /* priority/preference */
pub const FRA_UNUSED3: u16 = 7;
pub const FRA_UNUSED4: u16 = 8;
pub const FRA_UNUSED5: u16 = 9;
pub const FRA_FWMARK: u16 = 10;     /* mark */
pub const FRA_FLOW: u16 = 11;       /* flow/class id */
pub const FRA_TUN_ID: u16 = 12;
pub const FRA_SUPPRESS_IFGROUP: u16 = 13;
pub const FRA_SUPPRESS_PREFIXLEN: u16 = 14;
pub const FRA_TABLE: u16 = 15;      /* Extended table id */
pub const FRA_FWMASK: u16 = 16;     /* mask for netfilter mark */
pub const FRA_OIFNAME: u16 = 17;

#[derive(Debug)]
pub struct Rule {
    packet: NetlinkPacket<'static>,
}

impl Rule {
    /// iterate over rules
    pub fn iter_rules(conn: &mut NetlinkConnection) -> RulesIterator<&mut NetlinkConnection> {
        let mut buf = vec![0; MutableIfInfoPacket::minimum_packet_size()];
        let req = NetlinkRequestBuilder::new(RTM_GETRULE, NetlinkMsgFlags::NLM_F_DUMP)
            .append({
                let mut ifinfo = MutableIfInfoPacket::new(&mut buf).unwrap();
                ifinfo.set_family(0 /* AF_UNSPEC */);
                ifinfo
            }).build();
        let reply = conn.send(req);
        RulesIterator { iter: reply.into_iter() }
    }

    fn dump_rule(msg: NetlinkPacket) {
        if msg.get_kind() != RTM_NEWRULE {
            return;
        }
        //println!("NetLink pkt {:?}", msg);
        if let Some(rtm) = FibRulePacket::new(&msg.payload()[0..]) {
            println!("├ rtm: {:?}", rtm);
            let payload = &rtm.payload()[0..];
            let iter = RtAttrIterator::new(payload);
            for rta in iter {
                match rta.get_rta_type() {
                    FRA_PRIORITY => {
                        let mut cur = Cursor::new(rta.payload());
                        let prio = cur.read_u32::<LittleEndian>().unwrap();
                        println!(" ├ PRIORITY: {:?}", prio);
                    },
                    FRA_SUPPRESS_PREFIXLEN => {
                        println!(" ├ SUPPRESS PREFIXLEN: {:?}", rta.payload());
                    },
                    FRA_TABLE => {
                        let mut cur = Cursor::new(rta.payload());
                        let table = cur.read_u32::<LittleEndian>().unwrap();
                        println!(" ├ TABLE: {:?}", table);
                    },
                    _ => println!(" ├ {:?}", rta),
                }
            }
        }
    }
}

pub struct RulesIterator<R: Read> {
    iter: NetlinkBufIterator<R>,
}

impl<R: Read> Iterator for RulesIterator<R> {
    type Item = Rule;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(pkt) => {
                let kind = pkt.get_kind();
                if kind != RTM_NEWRULE {
                    return None;
                }
                return Some(Rule { packet: pkt });
            },
            None => None,
        }
    }
}

#[test]
fn dump_rules() {
    let mut conn = NetlinkConnection::new();
    for rule in Rule::iter_rules(&mut conn) {
        Rule::dump_rule(rule.packet);
    }
}
