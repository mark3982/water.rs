#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]

extern crate core;

use std::sync::Arc;
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

// We use this to be able to easily, but maybe dangerously
// change the actual type that ID represents. Hopefully,
// any dangerous change will produce compile errors!
pub type ID = u64;

// This is the ID that is considered unused and to comply
// with the specification it should _not_ be used to send
// messages across the network even though it may work.
pub const UNUSED_ID: ID = !0u64;

struct Internal {
    endpoints:      Vec<Endpoint>,
    hueid:          u64,              // highest unused endpoint id
    sid:            u64,
}

pub struct Net {
    i:      Arc<Mutex<Internal>>,
}

impl Clone for Net {
    fn clone(&self) -> Net {
        Net {
            i:      self.i.clone(),
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
                let mut i = net.i.lock();

                wokesomeone = false;
                for ep in i.endpoints.iter_mut() {
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
        let net = Net {
            i:  Arc::new(Mutex::new(Internal {
                endpoints:      Vec::new(),
                hueid:          0x10000,
                sid:            sid,
            })),
        };

        // Spawn a helper thread which provides the ability
        // for threads to wait for messages and have a timeout. 
        let netclone = net.clone();
        spawn(move || { Net::sleeperthread(netclone) });

        net
    }

    // Listens for and accepts TCP connections from remote
    // networks and performs simple routing between the two
    // networks.
    pub fn tcplisten(&self, addr: String) -> TcpBridgeListener {
        tcp::listener::TcpBridgeListener::new(self, addr)
    }

    // Tries to maintain a TCP connecton to the specified remote
    // network and performs simple routing between the two 
    // networks.
    pub fn tcpconnect(&self, addr: String) -> TcpBridgeConnector {
        tcp::connector::_TcpBridgeConnector::new(self, addr)
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
        let mut i = self.i.lock();

        if msg.dstsid != 1 && msg.dstsid != i.sid {
            panic!("you can only send clone message to local net!")
        }

        for ep in i.endpoints.iter_mut() {
            if msg.dsteid == ep.geteid() {
                ep.give(msg);
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
        let mut i = self.i.lock();

        if msg.dstsid != 1 && msg.dstsid != i.sid {
            panic!("you can only send sync message to local net!")
        }

        if msg.dsteid == 0 {
            panic!("you can only send sync message to a single endpoint!");
        }

        // Find who we need to send the message to, and only them.
        for ep in i.endpoints.iter_mut() {
            if msg.dsteid == ep.geteid() {
                ep.givesync(msg);
                return;
            }
        }
    }

    fn send_internal(&self, rawmsg: &Message) {
        match rawmsg.dstsid {
            0 => {
                // broadcast to everyone
                let mut i = self.i.lock();
                for ep in i.endpoints.iter_mut() {
                    if rawmsg.dsteid == 0 || rawmsg.dsteid == ep.geteid() {
                        ep.give(rawmsg);
                    }
                }
            },
            1 => {
                // ourself only
                let mut i = self.i.lock();
                let sid = i.sid;
                for ep in i.endpoints.iter_mut() {
                    if ep.getsid() == sid {
                        if rawmsg.dsteid == 0 || rawmsg.dsteid == ep.geteid() {
                            ep.give(rawmsg);
                        }
                    }
                }
            },
            dstsid => {
                // specific server
                let mut i = self.i.lock();                    
                for ep in i.endpoints.iter_mut() {
                    println!("trying to send {} to {}", dstsid, ep.getsid());
                    if ep.getsid() == dstsid {
                        if rawmsg.dsteid == 0 || rawmsg.dsteid == ep.geteid() {
                            ep.give(rawmsg);
                        }
                    }
                }
            }
        }
    }

    pub fn get_neweid(&mut self) -> u64 {
        let mut i = self.i.lock();
        let eid = i.hueid;
        i.hueid += 1;
        eid
    }
    
    pub fn new_endpoint_withid(&mut self, eid: u64) -> Endpoint {
        let mut i = self.i.lock();

        if eid > i.hueid {
            i.hueid = eid + 1;
        }

        let ep = Endpoint::new(i.sid, eid, self.clone());

        i.endpoints.push(ep.clone());

        ep
    }

    pub fn add_endpoint(&mut self, ep: Endpoint) {
        self.i.lock().endpoints.push(ep);
    }

    pub fn new_endpoint(&mut self) -> Endpoint {
        let mut i = self.i.lock();

        let ep = Endpoint::new(i.sid, i.hueid, self.clone());
        i.hueid += 1;

        let epclone = ep.clone();

        i.endpoints.push(epclone);

        ep
    }
    
    pub fn getserveraddr(&self) -> u64 {
        self.i.lock().sid
    }
}
