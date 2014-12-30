#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::Arc;
use std::sync::atomic;
use std::ptr;
use std::rt::heap::allocate;
use std::mem::size_of;
use std::mem::align_of;
use std::rt::heap::deallocate;
use std::sync::Mutex;
use std::sync::Condvar;
use std::time::duration::Duration;
use std::mem::transmute_copy;
use std::mem::uninitialized;
use std::intrinsics::copy_memory;
use std::intrinsics::transmute;

use time::Timespec;
use time::get_time;

use timespec;
/// Here we implement and export the Endpoint and IoResult<T> which provide
/// the core of the water library. These are to be some of the most used
/// facilities when working with water.

use net::Net;
use net::ID;
use net::UNUSED_ID;
use rawmessage::RawMessage;
use message::Message;
use message::MessagePayload;
 
/// This represents the exact failure code of the operation.
pub enum IoErrorCode {
    /// If the operation reaches the specified time to fail.
    TimedOut,
    /// There were no messages for the operation to succeed with.
    NoMessages,
}

impl Copy for IoErrorCode { }

/// This is returned by IoResult<T> when a error has occured. Some errors are normal
/// and expected. You can read the `code` field to determine the exact error.
pub struct IoError {
    pub code:   IoErrorCode,
}

impl Copy for IoError { }

/// Represents a successful return, or an I/O error. This deviates from the
/// standard library IoResult and should not be confused. It deviates to
/// provide a more specialized error value to indicate the exact problem.
pub enum IoResult<T> {
    Err(IoError),
    Ok(T),
}

impl<T> IoResult<T> {
    pub fn ok(self) -> T {
        match self {
            IoResult::Ok(v) => v,
            IoResult::Err(e) => panic!("not `IoResult::Ok`!"),
        }
    }

    pub fn err(self) -> IoError {
        match self {
            IoResult::Ok(v) => panic!("not `IoResult::Err`!"),
            IoResult::Err(e) => e,
        }
    }

    pub fn is_ok(&self) -> bool {
        match *self {
            IoResult::Ok(_) => true,
            IoResult::Err(_) => false,
        }
    }

    pub fn is_err(&self) -> bool {
        match *self {
            IoResult::Ok(_) => false,
            IoResult::Err(_) => true,
        }
    }
}

struct Internal {
    cwaker:         Condvar,
    messages:       Vec<Message>,
    wakeupat:       Timespec,
    wakeinprogress: bool,
    limitpending:   uint,
    limitmemory:    uint,
    memoryused:     uint,

    eid:            u64,
    sid:            u64,
    gid:            u64,
    net:            Net,
}

/// A endpoint represents a node on a net that can transmit and recieve messages.
///
/// The endpoint can transmit and recieve messages. It can be cloned so that multiple owners
/// can share an instance of it, and it is thread safe to multiple threads at the same time.
/// You can set the system identifier, endpoint identifier, and group identifier during run-time
/// at any time you wish making the endpoint versitile and flexible. The endpoint provides
/// synchronous and asynchronous I/O for `send` and `recv` method. It also has the ability to set
/// limits to prevent memory exhaustion by it's internal buffers.
pub struct Endpoint {
    i:          Arc<Mutex<Internal>>,
}


impl Clone for Endpoint {
    /// This will clone the endpoint allowing a second owner to have access. This does not
    /// duplicate the endpoint, but rather gives you a second handle to access it. This is
    /// a common operation.
    fn clone(&self) -> Endpoint {
        Endpoint {
            i:          self.i.clone(),
        }
    }
}

/// Represents the internal state of the endpoint. This is protected by a Mutex externally. The
/// methods here expect that an external locking mechanism will prevent multiple threads from
/// entering at the same time.
impl Internal {
    /// Sets the wake up time to be so far in the future that it will never be woken.
    fn neverwakeme(&mut self) {
        self.wakeupat = Timespec { sec: 0x7fffffffffffffffi64, nsec: 0i32 };
    }

    /// Takes one message from the queue and returns it. It also attempts to duplicate
    /// the message if that is supported to prevent giving access to shared buffers.
    fn recv(&mut self) -> IoResult<Message> {
        // The loop is needed for the sync type messages. We may have to discard
        // a message and try to read another one. This performs that function.
        loop {
            if self.messages.len() < 1 {
                return IoResult::Err(IoError { code: IoErrorCode::NoMessages });
            }

            // Takes the message out of the queue and duplicates it if possible.
            let msg = self.messages.remove(0).unwrap().dup_ifok();

            self.memoryused -= msg.cap();

            match msg.payload {
                MessagePayload::Raw(_) => { return IoResult::Ok(msg.dup()); },
                MessagePayload::Sync(_) => {
                    // To support first recv for sync message we need to try
                    // to take the message first. If we can not take the message
                    // we ignore it and throw it away.
                    if msg.get_syncref().takeasvalid() {
                        return IoResult::Ok(msg);
                    }
                },
                MessagePayload::Clone(_) => { return IoResult::Ok(msg); },
            }
        }
    }
}

impl Endpoint {
    /// Return the unique identifier for this endpoint. This will
    /// be unique except across process boundaries. This is the
    /// actual memory address of the internal structure.
    pub fn id(&self) -> uint {
        unsafe { transmute(&*self.i.lock()) }
    }

    /// Create a new endpoint by specifying the system ID, endpoint ID, and the
    /// network.
    pub fn new(sid: u64, eid: u64, net: Net) -> Endpoint {
        Endpoint {
            i:      Arc::new(Mutex::new(Internal {
                messages:       Vec::new(),
                cwaker:         Condvar::new(),
                wakeupat:       Timespec { nsec: 0i32, sec: 0x7fffffffffffffffi64},
                wakeinprogress: false,
                limitpending:   1024,
                limitmemory:    1024 * 1024 * 512,
                memoryused:     0,
                sid:            sid,
                eid:            eid,
                net:            net,
                gid:            UNUSED_ID,
            })),
        }
    }

    /// Get the time at which this endpoint should be woken. This is used to see when to
    /// wake the endpoint by the wake thread when it is in blocking mode.
    pub fn getwaketime(&self) -> Timespec {
        let i = self.i.lock();

        i.wakeupat
    }

    /// (internal usage) Give the endpoint a sync message.
    pub fn givesync(&mut self, msg: Message) -> bool {
        let mut i = self.i.lock();
        i.memoryused += msg.cap();
        i.messages.push(msg);
        drop(i);

        self.wakeonewaiter();
        true
    }

    /// (internal usage) Give the endpoint a raw message.
    pub fn give(&mut self, msg: &Message) -> bool {
        let mut i = self.i.lock();

        //println!("ep[{:p}] thinking about taking message {:p}", &*i, msg);
        //println!("msg.srcsid:{} msg.srceid:{} msg.dstsid:{} msg.dsteid:{}", msg.srcsid, msg.srceid, msg.dstsid, msg.dsteid);
        //println!("my.sid:{} my.eid:{} net.sid:{}", i.sid, i.eid, i.net.getserveraddr());

        // Do not send to the endpoint that it originated from.
        if !msg.canloop && msg.srceid == i.eid && msg.srcsid == i.sid {
            return false;
        }

        if msg.dstsid != 0 {
            if msg.dstsid != 1 {
                // It must be to a specific net and we are not it.
                if msg.dstsid != i.sid {
                    return false;
                }
            } else {
                // If its too the local net, but we are not part of
                // the local net. We are likely a type of bridge then
                // let us ignore it.
                if i.sid !=  i.net.getserveraddr() {
                    return false;
                }
            }
        }

        // If it is to a secific endpoint and we are not it.
        if msg.dsteid != 0 && msg.dsteid != i.eid {
            return false;
        }

        //println!("ep[{:p}] took message {:p}", &*i, msg);
        if msg.is_sync() {
            // The sync has to use a protected clone method.
            i.messages.push((*msg).internal_clone(0x879));
        } else {
            // Everything else can be cloned like normal.
            i.messages.push((*msg).clone());
        }

        i.memoryused += msg.cap();
        drop(i);
        self.wakeonewaiter();
        true
    }

    /// Return true if the endpoint has messages that recv will not fail on getting. Beware
    /// that is another threads call recv before you do that it may fail.
    pub fn hasmessages(&self) -> bool {
        if self.i.lock().messages.len() > 0 {
            true
        } else {
            false
        }
    }

    /// Wake one thread waiting on this endpoint.
    pub fn wakeonewaiter(&self) -> bool {
        let mut i = self.i.lock();

        if !i.wakeinprogress {
            //println!("ep.wakeonwaiter");
            i.cwaker.notify_all();
            i.wakeinprogress = true;
            true
        } else {
            false
        }
    }

    /// Sets the limit for pending messages in the queue.
    pub fn setlimitpending(&mut self, limit: uint) {
        self.i.lock().limitpending = limit;
    }

    /// Sets the maximum amount of memory messages in the queue may consume.
    pub fn setlimitmemory(&mut self, limit: uint) {
        self.i.lock().limitmemory = limit;
    }

    /// Get the system/net identifier.
    pub fn getsid(&self) -> ID {
        self.i.lock().sid
    }

    /// Get the endpoint identifier.
    pub fn geteid(&self) -> ID {
        self.i.lock().eid
    }

    /// Get the group identifier (like the endpoint identifier).
    pub fn getgid(&self) -> ID {
        self.i.lock().gid
    }

    /// Set the group identifier.
    pub fn setgid(&mut self, id: ID) {
        self.i.lock().gid = id;
    }

    /// Set the system/net identifier.
    pub fn setsid(&mut self, id: ID) {
        self.i.lock().sid = id;
    }

    /// Set the endpoint identifier.
    pub fn seteid(&mut self, id: ID) {
        self.i.lock().eid = id;
    }

    pub fn sendx(&self, msg: &Message) -> uint {
        let i = self.i.lock();
        let net = i.net.clone();
        drop(i);
        net.send(msg)
    }

    /// Send a message of raw type.
    pub fn send(&self, msg: &mut Message) -> uint {
        let i = self.i.lock();
        let net = i.net.clone();
        let sid = i.sid;
        let eid = i.eid;
        drop(i);
        net.sendas(msg, sid, eid)
    }

    /// Easily sends a sync message by wrapping it into a
    /// message. Using this function is the equivilent of:
    /// 
    /// `endpoint.sendsync(Message::new_sync(t))`
    ///
    /// This is a helper function to make sending easier
    /// and code cleaner looking.
    pub fn sendsynctype<T: Send>(&self, t: T) -> uint {
        let mut msg = Message::new_sync(t);
        msg.dstsid = 1; // only local net
        msg.dsteid = 0; // everyone
        self.sendsync(msg)
    }

    /// Easily sends a clone message by wrapping it into a
    /// message. Using this function is the equivilent of:
    /// 
    /// `endpoint.send(&Message::new_sync(t))`
    ///
    /// This is a helper function to make sending easier
    /// and code cleaner looking.
    pub fn sendclonetype<T: Send + Clone>(&self, t: T) -> uint {
        let mut msg = Message::new_clone(t);
        msg.dstsid = 1; // only local net
        msg.dsteid = 0; // everyone
        self.send(&mut msg)
    }


    /// Send a sync message. This requires a special call since a 
    /// sync message is unique and can not be cloned therefore we
    /// must consume the argument passed to prevent the caller from
    /// holding a copy or clone.
    ///
    /// See `sendsyncbytype` for easier calling.
    pub fn sendsync(&self, msg: Message) -> uint {
        let i = self.i.lock();
        let net = i.net.clone();
        let sid = i.sid;
        let eid = i.eid;
        drop(i);
        net.sendsyncas(msg, sid, eid)
    }

    /// Send a sync message but do not set from fields.
    pub fn sendsyncx(&self, msg: Message) -> uint {
        let i = self.i.lock();
        let net = i.net.clone();
        drop(i);
        net.sendsync(msg)
    }


    /// Recieve a message or block until the specifie duration expires then return an error condition.
    /// ```
    ///     let result = endpoint.recvorblock( Timespec { sec: 5i64, nsec: 0i32 } );
    ///     if result.is_err() { do_something(); }
    ///     let message = result.ok();
    /// ```
    /// See `Message` for API dealing with messages.
    pub fn recvorblock(&self, duration: Timespec) -> IoResult<Message> {
        let mut when: Timespec = get_time();

        when = timespec::add(when, duration);

        let mut i = self.i.lock();

        i.wakeinprogress = false;

        while i.messages.len() < 1 {
            // The wakeup thread will wake everyone up at or beyond
            // this specified time. Then anyone who needs to sleep
            // longer will go through this process again of setting
            // the time to the soonest needed wakeup.
            if i.wakeupat > when {
                i.wakeupat = when;
            }

            //println!("ep.id:{:p} sleeping", &*i);
            i.cwaker.wait(&i);
            //println!("ep.id:{:p} woke", &*i);
            i.wakeinprogress = false;

            let ctime: Timespec = get_time();

            if ctime > when && i.messages.len() < 1 {
                //println!("{:p} NO MESSAGES", &*i);
                // BugFix: Allow any other sleeping threads which will
                // wake once we unlock this mutex to set their
                // wake time. 
                i.neverwakeme();
                return IoResult::Err(IoError { code: IoErrorCode::TimedOut });
            }
        }

        // If another thread was sleeping too it will wake
        // after we return and it will set the wake value
        // if it is sooner than this or any value set after
        // this. Any other threads will wake as soon as `i`
        // which is the mutex guard gets dropped.
        i.neverwakeme();
        i.recv()
    }
    
    /// Recieve a message with out blocking and return an error condition if none.
    pub fn recv(&self) -> IoResult<Message> {
        self.i.lock().recv()
    }
}
