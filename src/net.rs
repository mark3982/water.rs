#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]

extern crate core;

use std::ptr;
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
use message::Message;
use message::MessagePayload;

use tcp;
use tcp::TcpBridgeListener;
use tcp::TcpBridgeConnector;

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
            let mut dealloc = false;
            {
                let locked = (*self.i).lock.lock();
                if (*self.i).refcnt == 0 {
                    panic!("drop called with refcnt zero");
                }
                
                (*self.i).refcnt -= 1;
                if (*self.i).refcnt == 0 {
                    dealloc = true;
                }
            }

            if dealloc {
                drop(ptr::read(&*self.i));
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
                        if ep.wakeonewaiter() {
                            wokesomeone = true;
                        }
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

    // Listens for and accepts TCP connections from remote
    // networks and performs simple routing between the two
    // networks.
    pub fn tcplisten(&self, host: String, port: u16) -> TcpBridgeListener {
        tcp::_TcpBridgeListener::new(self, host, port)
    }

    // Tries to maintain a TCP connecton to the specified remote
    // network and performs simple routing between the two 
    // networks.
    pub fn tcpconnect(&self, host: String, port: u16) -> TcpBridgeConnector {
        tcp::_TcpBridgeConnector::new(self, host, port)
    } 

    pub fn sendas(&self, rawmsg: &Message, frmsid: u64, frmeid: u64) {
        let mut duped = rawmsg.dup();
        duped.srcsid = frmsid;
        duped.srceid = frmeid;
        self.send_internal(&duped);
    }

    pub fn send(&self, msg: &Message) {
        match msg.payload {
            MessagePayload::Sync(ref payload) => {
                panic!("use `sendsync` instead of `send` for sync messages");
            }
            _ => {
            }
        }

        let duped = msg.dup();
        self.send_internal(&duped);
    }

    pub fn sendcloneas(&self, msg: &mut Message, fromsid: u64, fromeid: u64) {
        if !msg.is_clone() {
            panic!("`sendclone` can only be used with clone messages!")
        }
        msg.srcsid = fromsid;
        msg.srceid = fromeid;
        self.sendclone(msg);
    }

    pub fn sendclone(&self, msg: &Message) {
        if msg.dstsid != 1 && msg.dstsid != self.sid {
            panic!("you can only send clone message to local net!")
        }

        unsafe {
            let lock = (*self.i).lock.lock();
            for ep in (*self.i).endpoints.iter_mut() {
                if msg.dsteid == ep.eid {
                    ep.give(msg);
                }
            }
        }
    }

    pub fn sendsyncas(&self, mut msg: Message, frmsid: u64, frmeid: u64) {
        msg.srcsid = frmsid;
        msg.srceid = frmeid;
        self.sendsync(msg);
    }

    // A sync type message needs to be consumed because it
    // can not be duplicated. It also needs special routing
    // to handle sending it only to the local net which should
    // have only threads running in this same process.
    pub fn sendsync(&self, msg: Message) {
        if msg.dstsid != 1 && msg.dstsid != self.sid {
            panic!("you can only send sync message to local net!")
        }

        if msg.dsteid == 0 {
            panic!("you can only send sync message to a single endpoint!");
        }

        // Find who we need to send the message to, and only them.
        unsafe {
            let lock = (*self.i).lock.lock();
            for ep in (*self.i).endpoints.iter_mut() {
                if msg.dsteid == ep.eid {
                    ep.givesync(msg);
                    return;
                }
            }
        }
    }

    fn send_internal(&self, rawmsg: &Message) {
        match rawmsg.dstsid {
            0 => {
                // broadcast to everyone
                unsafe {
                    // Lock this because it is shared between
                    // threads and it is *not* thread safe.
                    let lock = (*self.i).lock.lock();
                    for ep in (*self.i).endpoints.iter_mut() {
                        if rawmsg.dsteid == 0 || rawmsg.dsteid == ep.eid {
                            ep.give(rawmsg);
                        }
                    }
                }                
            },
            1 => {
                // ourself only
                unsafe {
                    // Lock this because it is shared between
                    // threads and it is *not* thread safe.
                    let lock = (*self.i).lock.lock();
                    // Attempt to send to each endpoint. The endpoint
                    // has logic to decide to accept or ignore it.
                    for ep in (*self.i).endpoints.iter_mut() {
                        if ep.sid == self.sid {
                            if rawmsg.dsteid == 0 || rawmsg.dsteid == ep.eid {
                                ep.give(rawmsg);
                            }
                        }
                    }
                }
            },
            dstsid => {
                // specific server
                unsafe {
                    // Lock this because it is shared between
                    // threads and it is *not* thread safe.
                    let lock = (*self.i).lock.lock();
                    // Attempt to send to each endpoint. The endpoint
                    // has logic to decide to accept or ignore it.
                    for ep in (*self.i).endpoints.iter_mut() {
                        if ep.sid == dstsid {
                            if rawmsg.dsteid == 0 || rawmsg.dsteid == ep.eid {
                                ep.give(rawmsg);
                            }
                        }
                    }
                }                
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
