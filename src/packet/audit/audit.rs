use pnet_macros_support::types::*;
use pnet_macros::packet;

#[packet]
pub struct AuditStatus {
    mask: u32he,
    enabled: u32he,
    failure: u32he,
    pid: u32he,
    rate_limit: u32he,
    backlog_limit: u32he,
    lost: u32he,
    backlog: u32he,
    feature_bitmap: u32he,
    backlog_wait_time: u32he,
    #[payload]
    #[length="0"]
    _zero: Vec<u8>,
}

