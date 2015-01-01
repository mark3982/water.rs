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

use get_time;
use Timespec;

use rawmessage::RawMessage;
use endpoint::Endpoint;
use message::Message;
use message::MessagePayload;

use tcp;
use tcp::TcpBridgeListener;
use tcp::TcpBridgeConnector;

/// We use this to be able to easily, but maybe dangerously
/// change the actual type that ID represents. Hopefully,
/// any dangerous change will produce compile errors!
pub type ID = u64;

/// This is the ID that is considered unused and to comply
/// with the specification it should _not_ be used to send
/// messages across the network even though it may work.
pub const UNUSED_ID: ID = !0u64;

struct Internal {
    endpoints:      Vec<Endpoint>,
    hueid:          ID,              // highest unused endpoint id
}

/// Forms a group of endpoints that can all communicate locally. All
/// endpoints under a single net are considered under the same process
/// and can all share memory which means that sync and clone messages
/// can be sent to each other. To bridge two different nets you can use
/// the bridges provided as listeners and connectors.
///
/// _You will normally not call most your methods directly on a `Net`, but
/// instead on your `Endpoint` that you create. Some of the methods here are
/// intended to only be used by `Endpoint` and not called directly. Hopefully
/// in the future I can start removing these from being visible outside the
/// crate._
pub struct Net {
    i:      Arc<Mutex<Internal>>,
    sid:    ID,
}

impl Clone for Net {
    fn clone(&self) -> Net {
        Net {
            i:      self.i.clone(),
            sid:    self.sid,
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

    /// Return the number of endpoints on this net.
    pub fn getepcount(&self) -> uint {
        self.i.lock().endpoints.len()
    }

    /// Create new net with the specified ID.
    pub fn new(sid: ID) -> Net {
        let net = Net {
            i:  Arc::new(Mutex::new(Internal {
                endpoints:      Vec::new(),
                hueid:          0x10000,
            })),
            sid:    sid,
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
        tcp::connector::TcpBridgeConnector::new(self, addr)
    } 

    /// Send message with specified from addresses.
    pub fn sendas(&self, msg: &mut Message, fromsid: ID, fromeid: ID) -> uint {
        msg.srcsid = fromsid;
        msg.srceid = fromeid;
        self.send(msg)
    }

    /// Send message.
    pub fn send(&self, msg: &Message) -> uint {
        if msg.is_sync() {
            panic!("You must send a sync message with `sendsync` or `sendsyncas`!");
        }

        if msg.is_raw() {
            // Duplicate it to not share the buffer with the sender.
            self.send_internal(&(msg.dup()))
        } else {
            self.send_internal(msg)
        }
    }

    /// Consume the sync message and send with the specified from addresses.
    pub fn sendsyncas(&self, mut msg: Message, frmsid: ID, frmeid: ID) -> uint {
        msg.srcsid = frmsid;
        msg.srceid = frmeid;
        self.sendsync(msg)
    }

    /// Consume the sync message and send.
    pub fn sendsync(&self, msg: Message) -> uint {
        self.send_internal(&msg)
    }

    // Try to give the message to all endpoints. The endpoints do the logic
    // to determine if they will recieve the message.
    fn send_internal(&self, msg: &Message) -> uint {
        let mut ocnt = 0u;

        let mut i = self.i.lock();
        for ep in i.endpoints.iter_mut() {
            if ep.give(msg) {
                ocnt += 1;
            }
        }

        ocnt
    }

    pub fn get_neweid(&mut self) -> ID {
        let mut i = self.i.lock();
        let eid = i.hueid;
        i.hueid += 1;
        eid
    }
    
    pub fn new_endpoint_withid(&mut self, eid: ID) -> Endpoint {
        let mut i = self.i.lock();

        if eid > i.hueid {
            i.hueid = eid + 1;
        }

        let ep = Endpoint::new(self.sid, eid, self.clone());

        i.endpoints.push(ep.clone());

        ep
    }

    pub fn add_endpoint(&mut self, ep: Endpoint) {
        self.i.lock().endpoints.push(ep);
    }

    pub fn drop_endpoint(&mut self, thisep: &Endpoint) {
        let mut lock = self.i.lock();
        let endpoints = &mut lock.endpoints;

        for i in range(0u, endpoints.len()) {
            if endpoints[i].id() == thisep.id() {
                // The drop method of Endpoint likely called this method
                // so hopefully it is capable of handling a nested call
                // back into it's self.
                endpoints.remove(i);
                return;
            }
        }
    }

    pub fn new_endpoint(&mut self) -> Endpoint {
        let mut i = self.i.lock();

        let ep = Endpoint::new(self.sid, i.hueid, self.clone());
        i.hueid += 1;

        let epclone = ep.clone();

        i.endpoints.push(epclone);

        ep
    }
    
    pub fn getserveraddr(&self) -> ID {
        self.sid
    }
}
