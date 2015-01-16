//! Here we implement the Endpoint and IoResult<T> which provide
//! the core of the water library. These are to be some of the most used
//! facilities when working with the water library. An endpoint forms
//! the point of communication with other endpoints.

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]

use std;
use std::sync::Arc;
use std::sync::atomic;
use std::sync::atomic::AtomicUint;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::ptr;
use std::rt::heap::allocate;
use std::mem::size_of;
use std::mem::align_of;
use std::rt::heap::deallocate;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::Condvar;
use std::time::duration::Duration;
use std::mem::uninitialized;
use std::intrinsics::copy_memory;
use std::mem::transmute;
use std::mem::transmute_copy;
use std::thread::Thread;

use SafeQueue;
use SizedRingQueue;

use time::Timespec;
use time::get_time;

use timespec;

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

struct SleepToken {
    condvar:        Condvar,
    mutex:          Mutex<()>,
}

impl SleepToken {
    pub fn new() -> SleepToken {
        SleepToken {
            condvar:    Condvar::new(),
            mutex:      Mutex::new(()),
        }
    }

    pub fn wait(&self) {
        let lock = self.mutex.lock().unwrap();
        self.condvar.wait(lock).unwrap();
    }

    pub fn notify_all(&self) {
        self.condvar.notify_all();
    }
}

struct AddressData {
    eid:            ID,
    sid:            ID,
    gid:            ID,
}

struct Internal {
    stoken:         SleepToken,
    messages:       SafeQueue<Message>,
    wakeupat:       Mutex<Timespec>,
    memoryused:     AtomicUint,
    net:            Net,
    dropped:        AtomicBool,
    refcnt:         AtomicUint,
    slpcnt:         AtomicUint,
    limitpending:   AtomicUint,
    limitmemory:    AtomicUint,
    address:        Mutex<AddressData>
}

unsafe impl Send for Internal { }

/// A endpoint represents a node on a net that can transmit and recieve messages.
///
/// The endpoint can transmit and recieve messages. It can be cloned so that multiple owners
/// can share an instance of it, and it is thread safe to multiple threads at the same time.
/// You can set the system identifier, endpoint identifier, and group identifier during run-time
/// at any time you wish making the endpoint versitile and flexible. The endpoint provides
/// synchronous and asynchronous I/O for receiving and asynchrnous I/O for sending. It also has
/// the ability to set limits to prevent memory exhaustion by it's internal buffers.
///
/// An endpoint is identifier currently with three
///
/// An endpoint can be created with an automatically assigned unique ID, or you can specify
/// the endpoint ID. Endpoints may also share the same ID which makes directly addressing
/// one of them impossible except using a sync message. 
///
/// An example of using an endpoint like a channel:
/// 
///     #![allow(unused_results)]
///     #![allow(unused_must_use)]
///     #![allow(unstable)]
///     use water::Net;
///     use water::Duration;
///     use std::sync::mpsc::channel;
///
///     // Create a Rust channel.     
///     let (tx, rx) = channel::<usize>();
///
///     // Create the equivilent of a Rust channel (but bi-directional).
///     let mut net = Net::new(200);
///     let mut ep1 = net.new_endpoint();
///     let mut ep2 = net.new_endpoint();
//
///     // Here we send and recv using water.
///     ep1.sendclonetype(3us);
///     ep2.recvorblock(Duration::seconds(9));
///
///     // Here we send and recv using Rust channels.
///     tx.send(3us);
///     rx.recv();
///
/// As you can see it would be fairly easy to create a helper function to make creation of
/// a water bi-direction channel as easy as the Rust channel. However, the water channel is
/// much more powerful in that it is bi-directional and can handle joining many endpoints into
/// a single net where they can all communicate together.
pub struct Endpoint {
    i:          Arc<Internal>,
}

unsafe impl Send for Endpoint { }

impl Clone for Endpoint {
    /// This will clone the endpoint allowing a second owner to have access. This does not
    /// duplicate the endpoint, but rather gives you a second handle to access it. This is
    /// a common operation.
    fn clone(&self) -> Endpoint {
        self.i.refcnt.fetch_add(1, Ordering::Relaxed);
        Endpoint {
            i:          self.i.clone(),
        }
    }
}

/// Implements the very special drop code needed when the only instance of the
/// endpoint left is owned by the `Net` instance it was associated with. This
/// does not contain any unsafe code, but performs an important function of removing
/// the last remaining instance of the smart pointer from `Net` allowing it to
/// be truly dropped and deallocated with out manual intervention.
impl Drop for Endpoint {
    fn drop(&mut self) {
        let refcnt = self.i.refcnt.fetch_sub(1, Ordering::Acquire) - 1;
        // When we call into `drop_endpoint` a clone could happen on another thread. The drop
        // that will happen from net could then cause `drop_endpoint` to be called again which
        // would form a deadlock since at the moment `drop_endpoint` is not re-entrant.
        if refcnt == 1 && !self.i.dropped.compare_and_swap(false, true, Ordering::SeqCst) {
            self.i.net.drop_endpoint(self);
        }
    }
}

/// Represents the internal state of the endpoint. This is protected by a Mutex externally. The
/// methods here expect that an external locking mechanism will prevent multiple threads from
/// entering at the same time.
impl Internal {
    /// Sets the wake up time to be so far in the future that it will never be woken.
    fn neverwakeme(&self) {
        *self.wakeupat.lock().unwrap() = Timespec { sec: 0x7fffffffffffffffi64, nsec: 0i32 };
    }

    /// Takes one message from the queue and returns it. It also attempts to duplicate
    /// the message if that is supported to prevent giving access to shared buffers.
    fn recv(&self) -> IoResult<Message> {
        // The loop is needed for the sync type messages. We may have to discard
        // a message and try to read another one. This performs that function.
        loop {
            // Takes the message out of the queue and duplicates it if possible.
            //let msg = self.messages.remove(0).unwrap().dup_ifok();
            let result = self.messages.get();

            if result.is_none() {
                return IoResult::Err(IoError { code: IoErrorCode::NoMessages });
            }

            let msg = result.unwrap(); 
            let msg = msg.dup_ifok();
            let sz = msg.cap();
            self.memoryused.fetch_sub(sz, Ordering::SeqCst);

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
    pub fn id(&self) -> usize {
        unsafe { transmute(&(*self.i)) }
    }

    /// Return the number of references to this endpoint. Also can be known
    /// as the number of currently valid clones of this endpoint. Once this
    /// reaches one the endpoint is deallocated.
    ///
    /// _This should always return at least the value two since the net instance
    /// holds a clone and the current instance you are calling this on is considered
    /// a clone._
    pub fn getrefcnt(&self) -> usize {
        self.i.refcnt.load(Ordering::Relaxed)
    }

    /// Create a new endpoint by specifying the system ID, endpoint ID, and the
    /// network.
    pub fn new(sid: u64, eid: u64, net: Net) -> Endpoint {
        Endpoint {
            i:  Arc::new(Internal {
                messages:       SafeQueue::new(10),
                stoken:         SleepToken::new(),
                wakeupat:       Mutex::new(Timespec { nsec: 0i32, sec: 0x7fffffffffffffffi64 }),
                limitpending:   AtomicUint::new(0),
                limitmemory:    AtomicUint::new(0),
                memoryused:     AtomicUint::new(0),
                address:        Mutex::new(AddressData {
                    sid:        sid,
                    eid:        eid,
                    gid:        UNUSED_ID,
                }),
                net:            net,
                refcnt:         AtomicUint::new(1),
                slpcnt:         AtomicUint::new(0),
                dropped:        AtomicBool::new(false),
            }),
        }
    }

    /// Get the time at which this endpoint should be woken. This is used to see when to
    /// wake the endpoint by the wake thread when it is in blocking mode.
    pub fn getwaketime(&self) -> Timespec {
        *self.i.wakeupat.lock().unwrap()
    }

    /// Return the number of endpoints that may receiver this message
    /// not counting ourself. Any sent message will be evaluated by this
    /// number of endpoints, but may not be received but by zero or more.
    pub fn getpeercount(&self) -> usize {
        self.i.net.getepcount()
    }

    /// _(internal usage)_ Give the endpoint a message.
    pub fn give(&mut self, msg: &Message) -> bool {
        let addr = self.i.address.lock().unwrap();
        let myeid = addr.eid;
        let mysid = addr.sid;
        let mygid = addr.gid;
        drop(addr);


        // Do not send to the endpoint that it originated from.
        if !msg.canloop && msg.srceid == myeid && msg.srcsid == mysid {
            return false;
        }

        if msg.dstsid != 0 {
            if msg.dstsid != 1 {
                // It must be to a specific net and we are not it.
                if msg.dstsid != mysid {
                    return false;
                }
            } else {
                // If its too the local net, but we are not part of
                // the local net. We are likely a type of bridge then
                // let us ignore it.
                if mysid !=  self.i.net.getserveraddr() {
                    return false;
                }
            }
        }

        // If it is to a secific endpoint and we are not it.
        if msg.dsteid != 0 && msg.dsteid != myeid {
            return false;
        }

        // Check limits for pending count and memory.
        let limitpending = self.i.limitpending.load(Ordering::Relaxed);
        if limitpending > 0 && self.i.messages.len() >= limitpending {
            return false;
        }

        let limitmemory = self.i.limitmemory.load(Ordering::Relaxed);
        if limitmemory > 0 && self.i.memoryused.load(Ordering::SeqCst) >= limitmemory {
            return false;
        }

        let cloned;

        //println!("ep[{:p}] took message {:p}", &*i, msg);
        if msg.is_sync() {
            // The sync has to use a protected clone method.
            cloned = (*msg).internal_clone(0x879);
            //i.messages.put((*msg).internal_clone(0x879));
        } else {
            // Everything else can be cloned like normal.
            //i.messages.put((*msg).clone());
            cloned = (*msg).clone();
        }

        self.i.messages.put(cloned);
        //println!("after put");
        self.i.memoryused.fetch_add(msg.cap(), Ordering::SeqCst);
        //println!("after msg cap");
        self.wakeonewaiter();
        //println!("returning");
        true
    }

    /// Return the number of sleeping threads. 
    ///
    /// _This will not be entirely accurate as
    /// it may account for a thread which has actually just been woken up. There is
    /// a number of instructions to be executed from wake up until the thread updates
    /// this to show it is not sleeping therefore it may not be accurate._
    pub fn sleepercount(&self) -> usize {
        self.i.slpcnt.load(Ordering::Relaxed)
    }

    /// Return true if the endpoint has messages that recv will not fail on getting. Beware
    /// that is another threads call recv before you do that it may fail.
    pub fn hasmessages(&self) -> bool {
        if self.i.messages.len() > 0 {
            true
        } else {
            false
        }
    }

    /// Wake one thread waiting on this endpoint.
    pub fn wakeonewaiter(&self) {
        // Really need this for performance.
        //self.i.stoken.notify_all();
    }

    /// Sets the limit for pending messages in the queue.
    pub fn setlimitpending(&mut self, limit: usize) {
        self.i.limitpending.store(limit, Ordering::Relaxed);
    }

    /// Sets the maximum amount of memory messages in the queue may consume.
    pub fn setlimitmemory(&mut self, limit: usize) {
        self.i.limitmemory.store(limit, Ordering::Relaxed);
    }

    /// Get the system/net identifier.
    pub fn getsid(&self) -> ID {
        self.i.address.lock().unwrap().sid
    }

    /// Get the endpoint identifier.
    pub fn geteid(&self) -> ID {
        self.i.address.lock().unwrap().eid
    }

    /// Get the group identifier (like the endpoint identifier).
    pub fn getgid(&self) -> ID {
        self.i.address.lock().unwrap().gid
    }

    /// Set the group identifier.
    pub fn setgid(&mut self, id: ID) {
        self.i.address.lock().unwrap().gid = id;
    }

    /// Set the system/net identifier.
    pub fn setsid(&mut self, id: ID) {
        self.i.address.lock().unwrap().sid = id;
    }

    /// Set the endpoint identifier.
    pub fn seteid(&mut self, id: ID) {
        self.i.address.lock().unwrap().eid = id;
    }

    /// Send a message, but leave from address fields alone.
    ///
    /// _If sync type use `syncsync` or `sendsyncx`._
    pub fn sendx(&self, msg: Message) -> usize {
        self.i.net.send(msg)
    }

    /// Send a message, but mutates message from address fields with correct return address.
    ///
    /// _If sync type use `sendsync` or `sendsyncx`._
    pub fn send(&self, msg: Message) -> usize {
        let lock = self.i.address.lock().unwrap();
        let sid = lock.sid;
        let eid = lock.eid;
        drop(lock);
        self.i.net.sendas(msg, sid, eid)
    }

    /// Easily sends a sync message by wrapping it into a
    /// message. Using this function is the same as doing:
    /// 
    /// `endpoint.sendsync(Message::new_sync(t))`
    ///
    /// _This is a helper function to make sending easier
    /// and code cleaner looking._
    pub fn sendsynctype<T: Send>(&self, t: T) -> usize {
        let mut msg = Message::new_sync(t);
        msg.dstsid = 1; // only local net
        msg.dsteid = 0; // everyone
        self.send(msg)
    }

    /// Easily sends a clone message by wrapping it into a
    /// message. Using this function is the same as doing:
    /// 
    /// `endpoint.send(&Message::new_sync(t))`
    ///
    /// _This is a helper function to make sending easier
    /// and code cleaner looking._
    pub fn sendclonetype<T: Send + Clone>(&self, t: T) -> usize {
        let mut msg = Message::new_clone(t);
        msg.dstsid = 1; // only local net
        msg.dsteid = 0; // everyone
        self.send(msg)
    }

    /// Return a message or block forever until one is received.
    pub fn recvorblockforever(&self) -> IoResult<Message> {
        loop {
            let r = self.i.recv();
            if r.is_ok() {
                return r;
            }
            Thread::yield_now();
        }
    }

    /// Recieve a message or block until the specified duration expires then return an error condition.
    /// 
    ///     #![allow(unstable)]
    ///     use water::Net;
    ///     use water::Duration;
    ///
    ///     let mut net = Net::new(123);
    ///     let mut ep1 = net.new_endpoint();
    ///     let mut ep2 = net.new_endpoint();
    ///     ep2.sendclonetype(3u);             
    ///     let result = ep1.recvorblock(Duration::seconds(5));
    ///     if result.is_err() { 
    ///         println!("no message");
    ///     } else {
    ///         println!("got message [{}]", result.ok().typeunwrap::<usize>());
    ///     }
    /// 
    /// See `Message` for API dealing with messages.
    pub fn recvorblock(&self, duration: Duration) -> IoResult<Message> {
        let mut when: Timespec = get_time();
        when = when + duration;

        loop {
            let result = self.i.recv();
            if result.is_ok() {
                return result;
            }

            Thread::yield_now();

            let ctime: Timespec = get_time();
            if ctime > when {
                return IoResult::Err(IoError { code: IoErrorCode::TimedOut });
            }
        }
    }
    
    /// Recieve a message with out blocking and return an error condition if none.
    ///
    ///     use water::Net;
    ///     use water::Duration;
    ///     
    ///     let mut net = Net::new(123);
    ///     let mut ep1 = net.new_endpoint();
    ///     let mut ep2 = net.new_endpoint();
    ///     ep2.sendclonetype(3u);             
    ///     let result = ep1.recv();
    ///     if result.is_err() { 
    ///         println!("no message");
    ///     } else {
    ///         println!("got message [{}]", result.ok().typeunwrap::<usize>());
    ///     }
    ///
    pub fn recv(&self) -> IoResult<Message> {
        self.i.recv()
    }
}

/// Will not return until it has a valid message or a critical errored occures.
///
/// _This function can wait on multiple endpoints._
pub fn recvorblockforever(list: &Vec<Endpoint>) -> IoResult<Message> {
    loop {
        let result = recv(list);

        if result.is_ok() {
            return result;
        }

        Thread::yield_now();
    }
}

/// Return immediantly if not message can be received.
///
/// _This function can wait on multiple endpoints._
pub fn recv(list: &Vec<Endpoint>) -> IoResult<Message> {
    for ep in list.iter() {
        let result = ep.i.recv();
        if result.is_ok() {
            return result;
        }
    }

    IoResult::Err(IoError { code: IoErrorCode::NoMessages })
}

/// Wait the specified duration for a message or return if a message received.
///
/// _This function can wait on multiple endpoints._
pub fn recvorblock(list: &Vec<Endpoint>, duration: Duration) -> IoResult<Message> {
    let when: Timespec = get_time() + duration;

    loop {
        let result = recv(list);

        if result.is_ok() {
            return result;
        }

        if get_time() > when {
            return IoResult::Err(IoError { code: IoErrorCode::TimedOut });
        }

        Thread::yield_now();
    }
}