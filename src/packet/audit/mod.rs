use packet::netlink::NetlinkMsgFlags;
use packet::netlink::{NetlinkReader,NetlinkRequestBuilder};
use packet::netlink::NetlinkConnection;
use pnet::packet::Packet;
use std::io::{Read,Write};

/*
use packet::netlink::{MutableNetlinkPacket,NetlinkPacket};
use packet::netlink::NetlinkBufIterator;
use pnet::packet::PacketSize;
use ::socket::{NetlinkSocket,NetlinkProtocol};
use pnet::packet::MutablePacket;
use libc;
*/

include!("audit.rs");

pub trait Audit where Self: Read + Write {
    fn audit_enable<'a>(&'a mut self) -> ::std::io::Result<()>;
}

impl Audit for NetlinkConnection {
    fn audit_enable<'a>(&'a mut self) -> ::std::io::Result<()> {
        let mut buf = vec![0; MutableAuditStatusPacket::minimum_packet_size()];
        let req = NetlinkRequestBuilder::new(1001, NetlinkMsgFlags::NLM_F_REQUEST | NetlinkMsgFlags::NLM_F_ACK)
            .append({
                let mut status = MutableAuditStatusPacket::new(&mut buf).unwrap();
                status.set_mask(1);
                status.set_enabled(1);
                status
            }).build();
        self.write(req.packet())?;
        let reader = NetlinkReader::new(self);
        reader.read_to_end()
    }
}
