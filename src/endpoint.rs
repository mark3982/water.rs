#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::atomic;
use std::ptr;
use std::rt::heap::allocate;
use std::mem::size_of;
use std::rt::heap::deallocate;
use std::sync::Mutex;
use std::sync::Condvar;
use std::time::duration::Duration;
use std::mem::transmute_copy;
use std::mem::uninitialized;
use std::intrinsics::copy_memory;

use time::Timespec;
use time::get_time;

use timespec;
use net::Net;
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
    lock:           Mutex<uint>,
    refcnt:         uint,
    messages:       Vec<Message>,
    cwaker:         Condvar,
    wakeupat:       Timespec,
    wakeinprogress: bool,
    limitpending:   uint,
    limitmemory:    uint,
    memoryused:     uint,
}

pub struct Endpoint {
    i:          *mut Internal,
    pub eid:    u64,    // endpoint
    pub sid:    u64,    // net/system
    pub gid:    u64,    // group 
    net:        Net,

}

impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe {
            let mut dealloc: bool = false;

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

            // We can not deallocate the mutex because once `locked` drops out of
            // scope it may access the memory supporting the type therefore we must
            // first unlock the mutex then deallocate. We can be sure we do not need
            // a mutex, because obviously we were the last user of it.
            if dealloc {
                drop(ptr::read(&*self.i));
                
                // Not sure if I really need something like this. But.. I could
                // see reordering maybe being a problem?
                //atomic::fence(atomic::Acquire);

                deallocate(self.i as *mut u8, size_of::<Internal>(), size_of::<uint>());
            }
        }
    }
}

impl Clone for Endpoint {
    fn clone(&self) -> Endpoint {
        unsafe {
            let locked = (*self.i).lock.lock();
            (*self.i).refcnt += 1;
            Endpoint {
                i:      self.i,
                eid:    self.eid,
                sid:    self.sid,
                gid:    self.gid,
                net:    self.net.clone(),
            }
        }
    }
}

impl Endpoint {
    pub fn new(sid: u64, eid: u64, net: Net) -> Endpoint {
        let i: *mut Internal;
        
        unsafe {
            // Allocate memory then manually and dangerously initialize each field. If
            // the structure changes and you forget to initialize it here then you have
            // potentially a bug.
            i = allocate(size_of::<Internal>(), size_of::<uint>()) as *mut Internal;
            (*i).lock = Mutex::new(0);
            (*i).refcnt = 1;
            (*i).messages = Vec::new();
            (*i).cwaker = Condvar::new();
            (*i).wakeupat = Timespec { nsec: 0i32, sec: 0x7fffffffffffffffi64 };
            (*i).wakeinprogress = false;
            (*i).limitpending = 1024; 
            (*i).limitmemory = 1024 * 1024 * 512;
            (*i).memoryused = 0;
        }
        
        Endpoint {
            i:      i,
            eid:    eid,
            sid:    sid,
            gid:    UNUSED_ID,
            net:    net,
        }
    }

    pub fn getwaketime(&self) -> Timespec {
        unsafe {
            let lock = (*self.i).lock.lock();
            (*self.i).wakeupat
        }
    }

    pub fn givesync(&mut self, msg: Message) {
        unsafe {
            let lock = (*self.i).lock.lock();
            (*self.i).memoryused += msg.cap();
            (*self.i).messages.push(msg);
            // Let us wake anything that is already waiting.
            self.wakeonewaiter_nolock();            
        }
    }

    pub fn give(&mut self, msg: &Message) {
        unsafe {
            // only if it is addressed to us or to anyone
            if (self.eid == msg.dsteid || msg.dsteid == 0) || (self.gid == msg.dsteid) {
                if self.sid == msg.dstsid || msg.dstsid == 0 {
                    // TODO: add limit to prevent memory overflow
                    let lock = (*self.i).lock.lock();
                    (*self.i).messages.push((*msg).clone());
                    (*self.i).memoryused += msg.cap();
                    // Let us wake anything that is already waiting.
                    self.wakeonewaiter_nolock();
                }
            }
        }
    }

    pub fn hasmessages(&self) -> bool {
        unsafe {
            let lock = (*self.i).lock.lock();
            if (*self.i).messages.len() > 0 {
                return true;
            }
        }
        false
    }

    // Wake one thread waiting on this endpoint.
    pub fn wakeonewaiter(&self) -> bool {
        unsafe {
            let lock = (*self.i).lock.lock();
            if !(*self.i).wakeinprogress {
                (*self.i).cwaker.notify_all();
                (*self.i).wakeinprogress = true;
                return true;
            }
            false
        }
    }

    fn wakeonewaiter_nolock(&self) {
        unsafe {
            (*self.i).cwaker.notify_one();
        }
    }

    pub fn setlimitpending(&mut self, limit: uint) {
        unsafe {
            (*self.i).limitpending = limit;
        }
    }

    pub fn setlimitmemory(&mut self, limit: uint) {
        unsafe {
            (*self.i).limitmemory = limit;
        }
    }

    pub fn send(&self, msg: &Message) {
        self.net.sendas(msg, self.sid, self.eid);
    }

    pub fn sendsync(&self, msg: Message) {
        self.net.sendsyncas(msg, self.sid, self.eid);
    }

    pub fn sendclone(&self, msg: &mut Message) {
        self.net.sendcloneas(msg, self.sid, self.eid);
    }

    pub fn sendorblock(&self, msg: &Message) {
        unimplemented!();
    }

    fn neverwakeme_nolock(&self) {
        unsafe {
            (*self.i).wakeupat = Timespec { sec: 0x7fffffffffffffffi64, nsec: 0i32 };
        }
    }

    pub fn recvorblock(&self, duration: Timespec) -> IoResult<Message> {
        let mut when: Timespec = get_time();

        when = timespec::add(when, duration);

        unsafe {
            let lock = (*self.i).lock.lock();

            while (*self.i).messages.len() < 1 {
                // The wakeup thread will wake everyone up at or beyond
                // this specified time. Then anyone who needs to sleep
                // longer will go through this process again of setting
                // the time to the soonest needed wakeup.
                if (*self.i).wakeupat > when {
                    (*self.i).wakeupat = when;
                }

                (*self.i).cwaker.wait(&lock);

                let ctime: Timespec = get_time();

                if ctime > when {
                    return IoResult::Err(IoError { code: IoErrorCode::TimedOut });
                }
            }

            // If another thread was sleeping too it will wake
            // after we return and it will set the wake value.
            self.neverwakeme_nolock();
            (*self.i).wakeinprogress = false;

            self.recv_nolock()
        }
    }
    
    pub fn recv(&self) -> IoResult<Message> {
        unsafe {
            let lock = (*self.i).lock.lock();
            self.recv_nolock()
        }
    }

    pub fn recv_nolock(&self) -> IoResult<Message> {
        unsafe {
            if (*self.i).messages.len() < 1 {
                return IoResult::Err(IoError { code: IoErrorCode::NoMessages });
            }

            let msg = (*self.i).messages.remove(0).unwrap();

            (*self.i).memoryused -= msg.cap();

            match msg.payload {
                MessagePayload::Raw(_) => IoResult::Ok(msg.dup()),
                MessagePayload::Sync(_) => IoResult::Ok(msg),
                MessagePayload::Clone(_) => IoResult::Ok(msg),
            }
        }
    }    
}
