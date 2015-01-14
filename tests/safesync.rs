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
use water::Duration;

use std::thread::Thread;
use std::rc::Rc;
use std::sync::Arc;
use std::io::timer::sleep;

struct Foo {
    a:      uint,
}

struct Bar {
    a:      uint,
}

unsafe impl Sync for Foo { }

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

    ep.send(msg);

    println!("thread[{}]: exiting", dbgid);
}

#[test]
fn syncio() {
    // Create net with ID 234.
    let mut net: Net = Net::new(234);

    let ep = net.new_endpoint();

    // Spawn threads.
    let netclone = net.clone();
    let eid = ep.geteid();
    let ta = Thread::spawn(move || { funnyworker(netclone, 0, eid); });

    let result = ep.recvorblock(Duration::seconds(3));

    let msg: Arc<Foo> = result.ok().get_sync().get_payload();

    drop(ta);
}

fn main() {
    syncio();
}