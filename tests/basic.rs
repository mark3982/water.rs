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

// A safe structure is one that has no pointers and uses only primitive
// types or types that are known to have primitive fields such as static
// sized arrays. Any type of pointer will not properly send to a remote
// machine. If this pointer is used on a remote machine you will likely
// crash. Also platform specific types like `uint` and `int` are not safe
// as they may be 64-bit on the sending machine and 32-bit on the recieving
// machine. This difference in size will cause the entire structure to
// be essentially corrupted unless steps are taken to ensure it is properly
// written or properly read.
struct SafeStructure {
    a:      u64,
    b:      u32,
    c:      u8,
}

fn funnyworker(mut net: Net, dbgid: uint) {
    // Create our endpoint.
    let ep: Endpoint = net.new_endpoint();
    let mut rawmsg: RawMessage;

    sleep(Duration::seconds(1));

    let mut sentmsgcnt: u32 = 0u32;
    let mut recvmsgcnt: u32 = 0u32;

    println!("thread[{}] started", dbgid);

    // We only have to create it once in our situation here, since
    // once it is passed to the I/O sub-system in water it is 
    // duplication to prevent you from changing a packet before it
    // gets completely sent.
    rawmsg = RawMessage::new(32);

    while recvmsgcnt < 1001u32 {
        // Read anything we can.
        loop { 
            println!("thread[{}] recving", dbgid);
            let result = ep.recvorblock(Timespec { sec: 0, nsec: 1000000 });
            match result {
                Ok(msg) => {
                    println!("reading message now");
                    let safestruct: SafeStructure = unsafe { msg.readstruct(0) };
                    println!("asserting on message");
                    assert!(safestruct.a == safestruct.b as u64);
                    assert!(safestruct.c == 0x12);
                    println!("thread[{}] got message", dbgid);
                    recvmsgcnt += 1;
                },
                Err(err) => {
                    println!("thread[{}] no more messages", dbgid);
                    break;
                }
            }
        }

        // Send something random.
        println!("thread[{}] sending random message", dbgid);
        if sentmsgcnt < 1000u32 {
            rawmsg.dstsid = 0;
            rawmsg.dsteid = 0;
            let safestruct = SafeStructure {
                a:  0x12345678,
                b:  0x12345678,
                c:  0x12,
            };
            // This will move the value meaning you can not use it afterwards, but
            // it should be optimized into a pointer so there is no performance
            // difference.
            // rawmsg.writestruct(0, safestruct);
            // This will copy the value using a reference into the message.
            rawmsg.writestructref(0, &safestruct);

            ep.send(&rawmsg);
            sentmsgcnt += 1;
        }
    }

    let safestruct = SafeStructure {
        a:  0x10,
        b:  0x10,
        c:  0x10,
    };

    rawmsg.writestructref(0, &safestruct);
    ep.send(&rawmsg);

    println!("thread[{}]: exiting", dbgid);
}

#[test]
fn rawmessage() {
    let m = RawMessage::new_fromstr("ABCDE");
    assert!(m.readu8(0) == 65);
    assert!(m.readu8(1) == 66);
    assert!(m.readu8(2) == 67);
    assert!(m.readu8(3) == 68);
    assert!(m.readu8(4) == 69);
    assert!(m.len() == 5);
}

#[test]
fn basicio() {
    // Create net with ID 234.
    let mut net: Net = Net::new(234);

    // Spawn threads.
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 0); });
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 1); });
    let netclone = net.clone();
    spawn(move || { funnyworker(netclone, 2); });

    let ep = net.new_endpoint();
    println!("main thread done");

    let mut completedcnt: u32 = 0u32;

    loop {
        let result = ep.recvorblock(Timespec { sec: 1, nsec: 0 });
        match result {
            Ok(msg) => {
                let safestruct: SafeStructure = unsafe { msg.readstruct(0) };
                if safestruct.a == safestruct.b as u64 && safestruct.b as u64 == safestruct.c as u64 {
                    println!("got finished!");
                    completedcnt += 1;
                    if completedcnt > 2 {
                        break;
                    }
                }
            },
            Err(err) => {
                continue;
            }
        }
    }
}

fn main() {
    basicio();
}