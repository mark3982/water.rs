#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_must_use)]

extern crate time;

use std::io::{TcpListener, TcpStream};
use std::cell::RefCell;
use std::sync::Arc;
use std::rt::heap::allocate;
use std::mem::size_of;
use std::mem::transmute;
use std::rt::heap::deallocate;
use std::sync::Mutex;

use net::Net;
use endpoint::Endpoint;
use rawmessage::RawMessage;

mod endpoint;
mod net;
mod rawmessage;
mod timespec;

fn main() {
    let mut net: Net = Net::new(234);

    let ep1: Endpoint = net.new_endpoint();
    let ep2: Endpoint = net.new_endpoint();

    let mut rm: RawMessage = RawMessage::new(32);

    rm.dstsid = 0;
    rm.dsteid = 0;
    rm.srcsid = 0;
    rm.srceid = 0;
    ep1.send(&rm);

    println!("done");
}