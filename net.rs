#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]

extern crate core;

use std::rt::heap::allocate;
use std::rt::heap::deallocate;
use std::mem::size_of;
use std::sync::Mutex;
use std::intrinsics::copy_memory;
use std::intrinsics::transmute;
use std::io::timer::sleep;
use std::time::duration::Duration;

use time::get_time;
use time::Timespec;

use rawmessage::RawMessage;
use endpoint::Endpoint;

pub enum NetExternalAddress {
    TCP(String, u16)
}

pub enum NetAddress {
    Everyone,
    GlobalSingle(u64),
    ServerSingle(u64, u64),
    GlobalGroup(u64),
    ServerGroup(u64, u64),
}

struct Internal {
    lock:           Mutex<uint>,
    endpoints:      Vec<Endpoint>,
    refcnt:         uint,
    hueid:          u64,              // highest unused endpoint id
}

pub struct Net {
    i:      *mut Internal,
    sid:    u64,                // this net/server id
}

impl Drop for Net {
    fn drop(&mut self) {
        unsafe {
            let locked = (*self.i).lock.lock();
            if (*self.i).refcnt == 0 {
                panic!("drop called with refcnt zero");
            }
            
            (*self.i).refcnt -= 1;
            if (*self.i).refcnt == 0 {
                deallocate(self.i as *mut u8, size_of::<Internal>(), size_of::<uint>());
            }
        }
    }
}

impl Clone for Net {
    fn clone(&self) -> Net {
        unsafe {
            let locked = (*self.i).lock.lock();
            (*self.i).refcnt += 1;
            Net {
                i:      self.i,
                sid:    self.sid,
            }
        }
    }
}

const NET_MAXLATENCY: i64 = 100000;    //  100ms
const NET_MINLATENCY: i64 = 100;      //   0.1ms

impl Net {
    // The problem is we have no good way for a thread to sleep with
    // a timeout until a message arrives. The timeout is the problem.
    // So this thread helps by checking each endpoint for new messages
    // and if any are found it's condition variable is activated which
    // will let a sleeping thread wake. Also, if the timeout has happen
    // the thread will also wake.
    //
    // We also use a dynamic wait period which will increase or decrease 
    // depending on the load. If it finds that it spends a lot of cycles 
    // doing nothing it will increase the sleep period, and if it finds 
    // there is work every time it wakes it will decrease the sleep 
    // period. This will create an initial high latency which will 
    // decrease with load.
    fn sleeperthread(net: Net) {
        let mut latency: i64 = NET_MAXLATENCY;
        let mut wokesomeone: bool;

        loop {
            sleep(Duration::microseconds(latency));
            let ctime: Timespec = get_time();
            {
                let lock = unsafe { (*net.i).lock.lock() };
                wokesomeone = false;
                for ep in unsafe { (*net.i).endpoints.iter_mut() } {
                    if ctime > ep.getwaketime() {
                        ep.wakeonewaiter();
                        wokesomeone = true;
                    }
                }
            }
            // Do dynamic adjustment of latency and CPU usage.
            // TODO: overflow check.. maybe?
            if wokesomeone {
                latency = latency / 2;
                if latency < NET_MINLATENCY {
                    latency = NET_MINLATENCY;
                }
            } else {
                latency = latency * 2;
                if latency > NET_MAXLATENCY {
                    latency = NET_MAXLATENCY;
                }
            }
        }
    }

    pub fn new(sid: u64) -> Net {
        let i: *mut Internal;
        
        unsafe {
            // Allocate memory then manually and dangerously initialize each field. If
            // the structure changes and you forget to initialize it here then you have
            // potentially a bug.
            i = allocate(size_of::<Internal>(), size_of::<uint>()) as *mut Internal;
            (*i).lock = Mutex::new(0);
            (*i).endpoints = Vec::new();
            (*i).refcnt = 1;
            (*i).hueid = 0x10000;
        }
        
        let net = Net {
            i:      i,
            sid:    sid,
        };

        // Spawn a helper thread which provides the ability
        // for threads to wait for messages and have a timeout. 
        let netclone = net.clone();
        spawn(move || { Net::sleeperthread(netclone) });

        net
    }
    
    unsafe fn clone_nolock(&self) -> Net {
        (*self.i).refcnt += 1;
        Net {
            i:      self.i,
            sid:    self.sid,
        }
    }

    pub fn connect(&self, addr: NetExternalAddress) {
    } 

    pub fn send(&self, rawmsg: &RawMessage) {
        match rawmsg.dstsid {
            0 => {
                // broadcast to everyone
                unsafe {
                    // Lock this because it is shared between
                    // threads and it is *not* thread safe.
                    let lock = (*self.i).lock.lock();
                    for ep in (*self.i).endpoints.iter_mut() {
                        ep.give(rawmsg);
                    }
                }                
            },
            1 => {
                // ourself only
                unsafe {
                    // Lock this because it is shared between
                    // threads and it is *not* thread safe.
                    let lock = (*self.i).lock.lock();
                    // Make sure we do not share our buffer with
                    // the caller, since they *likely* may alter
                    // it by reusing it before we get it sent!
                    let dupedrawmsg = rawmsg.dup();
                    // Attempt to send to each endpoint. The endpoint
                    // has logic to decide to accept or ignore it.
                    for ep in (*self.i).endpoints.iter_mut() {
                        ep.give(&dupedrawmsg);
                    }
                }
            },
            _ => {
                // specific server
            }
        }
    }
    
    pub fn new_endpoint_withid(&mut self, eid: u64) -> Endpoint {
        unsafe {
            let lock = (*self.i).lock.lock();

            if eid > (*self.i).hueid {
                (*self.i).hueid = eid + 1;
            }

            let ep = Endpoint::new(self.sid, eid, self.clone_nolock());

            (*self.i).endpoints.push(ep.clone());

            ep
        }
    }

    pub fn new_endpoint(&mut self) -> Endpoint {
        unsafe {
            let ep: Endpoint;
            {
                let lock = (*self.i).lock.lock();

                ep = Endpoint::new(self.sid, (*self.i).hueid, self.clone_nolock());

                (*self.i).hueid += 1;
            }

            // Well, we can not clone the Endpoint while holding the
            // Net lock because the endpoint will clone the Net which
            // will try to reacquire the non-re-entrant lock. So we unlock
            // do the clone, then re-lock and add it.
            let epcloned = ep.clone();

            {
                let lock = (*self.i).lock.lock();
                (*self.i).endpoints.push(epcloned);
            }

            ep
        }
    }
    
    pub fn getserveraddr(&self) -> u64 {
        self.sid
    }
}
