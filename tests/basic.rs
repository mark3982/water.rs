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
use water::MessagePayload;
use water::Message;
use water::IoResult;
use water::IoError;
use water::IoErrorCode;

use std::thread::Thread;
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

// Do not tag a structure has having no pointers when it does. This will
// violate the memory safety of Rust. If you are not sure if a structure
// has pointers then find out! Anything with pointers when read on the
// recieving end violates the memory safety of Rust.
impl NoPointers for SafeStructure {}

fn funnyworker(mut net: Net, dbgid: uint) {
    // Create our endpoint.
    let ep: Endpoint = net.new_endpoint();

    sleep(Duration::seconds(1));

    let limit = 100u32;

    let mut sentmsgcnt: u32 = 0u32;
    let mut recvmsgcnt: u32 = 0u32;

    println!("thread[{}] started", dbgid);

    // We only have to create it once in our situation here, since
    // once it is passed to the I/O sub-system in water it is 
    // duplication to prevent you from changing a packet before it
    // gets completely sent.
    let mut msgtosend = Message::new_raw(32);
    msgtosend.dstsid = 0;
    msgtosend.dsteid = 0;

    while recvmsgcnt < (limit + 1) {
        // Read anything we can.
        loop { 
            //println!("thread[{}] recving", dbgid);
            //let result = ep.recvorblock(Timespec { sec: 0, nsec: 1000000 });
            let result = ep.recv();

            if result.is_err() {
                //println!("thread timed out recving");
                break;
            }

            //println!("thread[{}] reading struct", dbgid);
            let safestruct: SafeStructure = result.ok().get_raw().readstruct(0);
            if safestruct.a != 0x10 {
                //println!("@@thread[{}] got {}/{} messages", dbgid, recvmsgcnt, limit);
                assert!(safestruct.a == safestruct.b as u64);
                assert!(safestruct.c == 0x12);
                recvmsgcnt += 1;
            }
        }

        // Send something.
        if sentmsgcnt < limit {
            //println!("thread[{}] sending message", dbgid);
            let safestruct = SafeStructure {
                a:  0x12345678,
                b:  0x12345678,
                c:  0x12,
            };
            //println!("thread sending something");
            msgtosend.get_rawmutref().writestructref(0, &safestruct);
            ep.send(&msgtosend);
            sentmsgcnt += 1;
        }
    }

    let safestruct = SafeStructure {
        a:  0x10,
        b:  dbgid as u32,
        c:  0x10,
    };
    msgtosend.get_rawmutref().writestructref(0, &safestruct);
    ep.send(&msgtosend);

    println!("thread[{}]: exiting (sent buffer {})", dbgid, msgtosend.get_rawref().getbufaddress());
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
fn rawmsgstress() {
    let mut v: Vec<RawMessage> = Vec::new();

    for i in range(0u, 10000u) {
        let rm = RawMessage::new(32);
        v.push(rm.dup());
        v.push(rm);
    }
}

#[test]
fn basicio() {
    // Try to repeat the test a number of times to hopefully
    // catching anything that might be missed if you only run
    // it once.
    for u in range(0u, 20u) {
        println!("making test");
        let t = Thread::spawn(move || { _basicio(); });
        t.join();
    }
}

fn _basicio() {
    println!("entered basicio");

    // Create net with ID 234.
    let mut net: Net = Net::new(234);

    // Create endpoint just to be sure we do not miss any messages
    // that will come from the threads. Although, we will likely
    // be in the loop below by the time the threads start sending.
    let ep = net.new_endpoint();
    let mut completedcnt: u32 = 0u32;

    println!("spawning threads");
    // Spawn threads.

    let mut threadterm = [0u, 0u, 0u];

    let netclone = net.clone();
    let ta = Thread::spawn(move || { funnyworker(netclone, 0); });
    let netclone = net.clone();
    let tb = Thread::spawn(move || { funnyworker(netclone, 1); });
    let netclone = net.clone();
    let tc = Thread::spawn(move || { funnyworker(netclone, 2); });

    let mut sectowait = 3i64;

    println!("main: entering loop");

    loop {
        let result = ep.recvorblock(Timespec { sec: sectowait, nsec: 0 });

        // It seems we need to wait just a bit I suppose for the threads
        // to actually get a message sent. Then after that we can read quite
        // fast.
        sectowait = 1i64;

        if result.is_err() {
            panic!("timed out waiting for messages likely..");
        }

        //println!("main: got message");

        let raw = result.ok().get_raw();
        let safestruct: SafeStructure = raw.readstruct(0);
        if safestruct.a == 0x10 {
            if threadterm[safestruct.b as uint] != 0 {
                //panic!("got termination message from same thread twice!");
            }
            threadterm[safestruct.b as uint] = 1;

            completedcnt += 1;
            println!("main: got termination message #{} from thread {} for buffer {}", completedcnt, safestruct.b, raw.getbufaddress());
            if completedcnt > 2 {
                break;
            }
        }
    }
}

fn main() {
    loop {
        println!("making test");
        let t = Thread::spawn(move || { _basicio(); });
        t.join();
    }
}