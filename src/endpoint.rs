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
use net::Net;
use net::ID;
use net::UNUSED_ID;
use rawmessage::RawMessage;
use message::Message;
use message::MessagePayload;

pub enum IoErrorCode {
    TimedOut,
    NoMessages,
}

pub struct IoError {
    pub code:   IoErrorCode,
}

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

pub struct Endpoint {
    i:          Arc<Mutex<Internal>>,

}


impl Clone for Endpoint {
    fn clone(&self) -> Endpoint {
        Endpoint {
            i:          self.i.clone(),
        }
    }
}

impl Internal {
    fn neverwakeme(&mut self) {
        self.wakeupat = Timespec { sec: 0x7fffffffffffffffi64, nsec: 0i32 };
    }

    fn recv(&mut self) -> IoResult<Message> {
        if self.messages.len() < 1 {
            return IoResult::Err(IoError { code: IoErrorCode::NoMessages });
        }

        let msg = self.messages.remove(0).unwrap();

        self.memoryused -= msg.cap();

        match msg.payload {
            MessagePayload::Raw(_) => IoResult::Ok(msg.dup()),
            MessagePayload::Sync(_) => IoResult::Ok(msg),
            MessagePayload::Clone(_) => IoResult::Ok(msg),
        }
    }
}

impl Endpoint {
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

    pub fn getwaketime(&self) -> Timespec {
        let i = self.i.lock();

        i.wakeupat
    }

    pub fn givesync(&mut self, msg: Message) {
        let mut i = self.i.lock();
        i.memoryused += msg.cap();
        i.messages.push(msg);
        drop(i);

        self.wakeonewaiter();
    }

    pub fn give(&mut self, msg: &Message) {
        let mut i = self.i.lock();

        if (i.eid == msg.dsteid || msg.dsteid == 0) || (i.gid == msg.dsteid) {
            if i.sid == msg.dstsid || msg.dstsid == 0 {
                i.messages.push((*msg).clone());
                i.memoryused += msg.cap();
                drop(i);

                self.wakeonewaiter();
                return;                
            }
        }
    }

    pub fn hasmessages(&self) -> bool {
        if self.i.lock().messages.len() > 0 {
            true
        } else {
            false
        }
    }

    // Wake one thread waiting on this endpoint.
    pub fn wakeonewaiter(&self) -> bool {
        let mut i = self.i.lock();

        if !i.wakeinprogress {
            i.cwaker.notify_all();
            i.wakeinprogress = true;
            true
        } else {
            false
        }
    }

    pub fn setlimitpending(&mut self, limit: uint) {
        self.i.lock().limitpending = limit;
    }

    pub fn setlimitmemory(&mut self, limit: uint) {
        self.i.lock().limitmemory = limit;
    }

    pub fn getsid(&self) -> ID {
        self.i.lock().sid
    }

    pub fn geteid(&self) -> ID {
        self.i.lock().eid
    }

    pub fn getgid(&self) -> ID {
        self.i.lock().gid
    }

    pub fn setgid(&mut self, id: ID) {
        self.i.lock().gid = id;
    }

    pub fn setsid(&mut self, id: ID) {
        self.i.lock().sid = id;
    }

    pub fn seteid(&mut self, id: ID) {
        self.i.lock().eid = id;
    }

    pub fn send(&self, msg: &Message) {
        let i = self.i.lock();
        let net = i.net.clone();
        let sid = i.sid;
        let eid = i.eid;
        drop(i);
        net.sendas(msg, sid, eid);
    }

    pub fn sendsync(&self, msg: Message) {
        let i = self.i.lock();
        let net = i.net.clone();
        let sid = i.sid;
        let eid = i.eid;
        drop(i);
        net.sendsyncas(msg, sid, eid);
    }

    pub fn sendclone(&self, msg: &mut Message) {
        let i = self.i.lock();
        let net = i.net.clone();
        let sid = i.sid;
        let eid = i.eid;
        drop(i);
        net.sendcloneas(msg, sid, eid);
    }

    pub fn sendorblock(&self, msg: &Message) {
        unimplemented!();
    }

    pub fn recvorblock(&self, duration: Timespec) -> IoResult<Message> {
        let mut when: Timespec = get_time();

        when = timespec::add(when, duration);

        let mut i = self.i.lock();

        while i.messages.len() < 1 {
            // The wakeup thread will wake everyone up at or beyond
            // this specified time. Then anyone who needs to sleep
            // longer will go through this process again of setting
            // the time to the soonest needed wakeup.
            if i.wakeupat > when {
                i.wakeupat = when;
            }

            i.cwaker.wait(&i);

            let ctime: Timespec = get_time();

            if ctime > when {
                return IoResult::Err(IoError { code: IoErrorCode::TimedOut });
            }
        }

        // If another thread was sleeping too it will wake
        // after we return and it will set the wake value.
        i.wakeinprogress = false;
        i.neverwakeme();
        i.recv()
    }
    
    pub fn recv(&self) -> IoResult<Message> {
        self.i.lock().recv()
    }
}
