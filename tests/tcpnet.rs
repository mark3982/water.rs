#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_must_use)]
#![allow(deprecated)]

extern crate time;
extern crate water;

use water::Net;
use water::Endpoint;
use water::RawMessage;
use water::NoPointers;
use water::Message;
use water::tcp;

use std::rc::Rc;
use std::sync::Arc;
use std::io::timer::sleep;
use std::time::duration::Duration;
use time::Timespec;

struct Foo {
    a:      uint,
}

struct Bar {
    a:      uint,
}

impl Sync for Foo { }

fn funnyworker(mut net: Net, dbgid: uint, dsteid: u64) {
    // Create our endpoint.
    let ep: Endpoint = net.new_endpoint();

    println!("thread[{}] started", dbgid);

    let mut msg = Message::new_sync(Arc::new(Foo { a: 0 }));

    // A sync message must be to the local net and have
    // only endpoint. You can have multiple endpoints with
    // the same EID, but only one of them will get the message.
    msg.dsteid = dsteid;    // specific end point
    msg.dstsid = 1;         // local net only

    ep.sendsync(msg);

    println!("thread[{}]: exiting", dbgid);
}

#[test]
fn tcpio() {
    std::thread::Thread::spawn(move || {
        // Create two nets then link then with TCP.
        let mut net1: Net = Net::new(234);
        let ep1 = net1.new_endpoint();
        let mut net2: Net = Net::new(875);
        let ep2 = net2.new_endpoint();

        // This will be asynchronous. So let us wait
        // until it actually completes.
        let mut listener = net1.tcplisten(String::from_str("localhost:34200"));
        let mut connector = net2.tcpconnect(String::from_str("localhost:34200"));

        println!("waiting for connected");
        while !connector.connected() { }
        println!("waiting for client count > 0");
        while listener.getclientcount() < 1 { }
        println!("waiting for negotiation to complete");
        while listener.getnegcount() < 1 { }

        println!("sending message");
        // Now, let us test sending a message from one
        // net to the other.
        let mut msg = Message::new_raw(32);
        msg.dstsid = 875;
        msg.dsteid = 0;
        {
            let rawmsg = msg.get_rawmutref();
            println!("raw buf pointer {}", rawmsg.id());
            let slice = rawmsg.as_mutslice();
            slice[0] = 13;
            slice[1] = 34;
            slice[2] = 56;
            slice[3] = 78;
        }
        ep1.send(&msg);

        println!("waiting for message that was sent");
        // Wait for the message to arrive.
        let result = ep2.recvorblock(Timespec { sec: 900i64, nsec: 0i32 });

        let rawmsg = result.ok().get_raw();
        let slice = rawmsg.as_slice();

        println!("id:{} {}:{}:{}:{}", rawmsg.id(), slice[0], slice[1], slice[2], slice[3]);

        assert!(slice[0] == 0x12);
        assert!(slice[1] == 0x34);
        assert!(slice[2] == 0x56);
        assert!(slice[3] == 0x78);

        println!("terminating listener and connector");
        listener.terminate();
        connector.terminate();
    });
}

fn main() {
    tcpio();
}