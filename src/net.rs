#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]

//!
//! The `Net` forms the central point in which endpoints can be created from, and
//! used to communicate with other endpoints on the same net or with endpoints on
//! another net through the usage of bridges.
//!
//! _At this time a TCP bridge has been implemented._
//!

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
use std::thread::Thread;

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

unsafe impl Send for Net { }

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
                let mut i = net.i.lock().unwrap();
                wokesomeone = false;
                for ep in i.endpoints.iter_mut() {
                    // The `ctime` check is for sleeping who want to timeout after
                    // a certain amount of time. The `hasmessages()` and `sleepercount()`
                    // clause is to help wake sleepers that did not get properly woken. This
                    // is due to changes and problems with the changes so the second clause
                    // is a workaround which i would love to get rid of
                    if ctime > ep.getwaketime() || (ep.hasmessages() && ep.sleepercount() > 0) {
                        println!("sleeper trying to wake {}", ep.id());
                        ep.wakeonewaiter();
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
    pub fn getepcount(&self) -> usize {
        self.i.lock().unwrap().endpoints.len()
    }

    /// Create new net with the specified ID.
    ///
    /// You can use the same ID for multiple nets and they will
    /// properly bridge except traffic will not be segmented 
    /// between the two nets. This may be desired. You should
    /// use an ID that is `100` or above. 
    ///
    ///     use water::Net;
    ///     let net = Net::new(100);
    ///
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
        // TODO: uncomment this
        //let netclone = net.clone();
        //Thread::spawn(move || { Net::sleeperthread(netclone) }).detach();

        net
    }

    // Listens for and accepts TCP connections from remote
    // networks and performs simple routing between the two
    // networks.
    ///
    ///      use water::Net;
    ///      let net = Net::new(100);
    ///      net.tcplisten(String::from_str("localhost:40100"))
    ///
    pub fn tcplisten(&self, addr: String) -> TcpBridgeListener {
        tcp::listener::TcpBridgeListener::new(self, addr)
    }

    // Tries to maintain a TCP connecton to the specified remote
    // network and performs simple routing between the two 
    // networks.
    ///
    ///      use water::Net;
    ///      let net = Net::new(100);
    ///      net.tcpconnect(String::from_str("localhost:40100"))
    ///    
    pub fn tcpconnect(&self, addr: String) -> TcpBridgeConnector {
        tcp::connector::TcpBridgeConnector::new(self, addr)
    } 

    /// Send message with specified from addresses.
    pub fn sendas(&self, mut msg: Message, fromsid: ID, fromeid: ID) -> usize {
        msg.srcsid = fromsid;
        msg.srceid = fromeid;
        self.send(msg)
    }

    /// Send message.
    pub fn send(&self, msg: Message) -> usize {
        if msg.is_raw() {
            // Duplicate it to not share the buffer with the sender.
            self.send_internal(msg.dup())
        } else {
            self.send_internal(msg)
        }
    }

    // Try to give the message to all endpoints. The endpoints do the logic
    // to determine if they will recieve the message.
    fn send_internal(&self, msg: Message) -> usize {
        let mut ocnt = 0us;

        // Just to be safe I only want to call immutable methods
        // on endpoints since we are not locking and as long as
        // it is sync this is okay, but I do need to force the
        // reference to a mutable one.
        let mut local = self.i.lock().unwrap().endpoints.clone();

        for ep in local.iter_mut() {
            if ep.give(&msg) {
                ocnt += 1;
            }
        }

        ocnt
    }

    /// Returns a ID that is unique to this net. It does this by tracking
    /// all IDs used and always returns one higher than the highest ID endpoint
    /// that is currently on the network.
    ///
    ///
    ///      use water::Net;
    ///      let mut net = Net::new(100);
    ///      let id = net.get_neweid();
    ///      let ep = net.new_endpoint_withid(id);                   // SAME
    ///      let ep = net.new_endpoint();                            // SAME
    ///
    pub fn get_neweid(&self) -> ID {
        let mut i = self.i.lock().unwrap();
        let eid = i.hueid;
        i.hueid += 1;
        eid
    }

    /// Return a new endpoint with the specified ID.
    ///
    ///      use water::Net;
    ///      let mut net = Net::new(100);
    ///      let id = net.get_neweid();
    ///      let ep = net.new_endpoint_withid(id);     // SAME
    pub fn new_endpoint_withid(&self, eid: ID) -> Endpoint {
        let mut i = self.i.lock().unwrap();

        if eid > i.hueid {
            i.hueid = eid + 1;
        }

        let ep = Endpoint::new(self.sid, eid, self.clone());

        i.endpoints.push(ep.clone());

        ep
    }

    /// Not recommend for usage.
    pub fn add_endpoint(&self, ep: Endpoint) {
        self.i.lock().unwrap().endpoints.push(ep);
    }

    /// Not recommened for usage.
    pub fn drop_endpoint(&self, thisep: &Endpoint) {
        let mut lock = self.i.lock().unwrap();
        let endpoints = &mut lock.endpoints;
        let mut ndx = 0us;
        let mut fnd = false;
        for i in range(0us, endpoints.len()) {
            if endpoints[i].id() == thisep.id() {
                // The drop method of Endpoint likely called this method
                // so hopefully it is capable of handling a nested call
                // back into it's self.
                ndx = i;
                if fnd {
                    panic!("multiple ep with same id");
                }
                fnd = true;
            }
        }

        if fnd {
            //println!("thread:{} drop_endpoint thisep:{:x} refcnt:{}", Thread::current().name().unwrap_or("none"), thisep.id(), thisep.getrefcnt());
            endpoints.remove(ndx);
        }
    }

    /// Return a new endpoint with an automatically assigned unique ID.
    pub fn new_endpoint(&self) -> Endpoint {
        let mut i = self.i.lock().unwrap();

        let ep = Endpoint::new(self.sid, i.hueid, self.clone());
        i.hueid += 1;

        let epclone = ep.clone();

        i.endpoints.push(epclone);

        ep
    }
    
    /// Return the network ID.
    pub fn getserveraddr(&self) -> ID {
        self.sid
    }
}
