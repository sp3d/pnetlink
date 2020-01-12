//! Netlink is a linux kernel interface used for communication between
//! kernel and userspace.
//!
//! `socket` module can be used to establish Netlink socket
//! `packet` contains high level functions and traits
#![allow(non_camel_case_types,non_upper_case_globals,non_snake_case,dead_code)]

#[macro_use]
extern crate bitflags;
extern crate pnet;
extern crate pnet_macros_support;
extern crate libc;
extern crate byteorder;
extern crate bytes;
extern crate mio;
extern crate tokio as tokio_crate;
extern crate futures;

pub mod socket;
pub mod packet;
pub mod tokio;
pub mod util;
