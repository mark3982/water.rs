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
fn syncio() {
    // Create net with ID 234.
    let mut net: Net = Net::new(234);

    let ep = net.new_endpoint();

    // Spawn threads.
    let netclone = net.clone();
    let eid = ep.eid;
    spawn(move || { funnyworker(netclone, 0, eid); });

    let mut completedcnt: u32 = 0u32;

    let result = ep.recvorblock(Timespec { sec: 3, nsec: 0 });

    let msg: Arc<Bar> = result.ok().get_sync().get_payload();

    // If you want to properly check for a result you can do
    // this (below).
    /*
    match result {
        Ok(msg) => {
            match msg.payload {
                let foo: Foo = msg.get_sync().get_payload();

        },
        Err(err) => {
            panic!("never got message!");
        }
    }
    */
}

fn main() {
    syncio();
}