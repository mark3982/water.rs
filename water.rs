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

use std::io::timer::sleep;
use std::time::duration::Duration;

mod endpoint;
mod net;
mod rawmessage;
mod timespec;

fn funnyworker(mut net: Net, dbgid: uint) {
    // Create our endpoint.
    let ep: Endpoint = net.new_endpoint();
    let mut rawmsg: RawMessage;

    loop {
        sleep(Duration::seconds(1));
        // Read anything we can.
        loop { 
            let result = ep.recv();
            match result {
                Ok(msg) => {
                    println!("thread[{}] got message", dbgid);
                },
                Err(err) => {
                    println!("thread[{}] no more messages", dbgid);
                    break;
                }
            }
        }
        // Send something random.
        println!("thread[{}] sending random message", dbgid);
        {
            rawmsg = RawMessage::new(32);
            rawmsg.dstsid = 0;
            rawmsg.dsteid = 0;
            ep.send(&rawmsg);
        }
    }
}

fn main() {
    // Create net with ID 234.
    let mut net: Net = Net::new(234);

    // Spawn threads.
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 0); });
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 1); });
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 2); });

    println!("main thread done");
    loop { }
}