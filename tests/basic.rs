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

use std::io::timer::sleep;
use std::time::duration::Duration;
use time::Timespec;

fn funnyworker(mut net: Net, dbgid: uint) {
    // Create our endpoint.
    let ep: Endpoint = net.new_endpoint();
    let mut rawmsg: RawMessage;

    sleep(Duration::seconds(1));

    for cycle in range(0u, 1000u) {
        // Read anything we can.
        loop { 
            println!("thread[{}] recving", dbgid);
            let result = ep.recvorblock(Timespec { sec: 1, nsec: 0 });
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

#[test]
fn simpletest() {
    // Create net with ID 234.
    let net: Net = Net::new(234);

    // Spawn threads.
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 0); });
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 1); });
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 2); });

    println!("main thread done");
}