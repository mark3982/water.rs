use std::rt::heap::allocate;
use std::rt::heap::deallocate;
use std::mem::size_of;
use std::sync::Mutex;
use std::intrinsics::transmute;
use std::mem::transmute_copy;
use std::mem::uninitialized;
use std::intrinsics::copy_memory;
use std::raw;
use std::sync::Arc;
use std;

// This is used to signify that a type is safe to
// be written and read from a RawMessage across
// process and machine boundaries. This trait can
// create dangerous code.
pub trait NoPointers { }

struct Internal {
    len:            uint,
    cap:            uint,
    buf:            *mut u8,
}

impl Drop for Internal {
    fn drop(&mut self) {
        unsafe { deallocate(self.buf, self.cap, size_of::<uint>()); }
    }
}

impl Internal {
    fn new(mut cap: uint) -> Internal {
        // This helps let sync and clone messages work. Since sometimes
        // they may use a zero size type just for conveying a signal of
        // some sort and the will try to allocate a message of zero bytes
        // which will result in undefined behavior.
        if cap == 0 {
            cap = 1;
        }

        Internal {
            len:        cap,
            cap:        cap,
            buf:        unsafe { allocate(cap, size_of::<uint>()) },
        }
    }

    fn dup(&self) -> Internal {
        let i = Internal {
            len:        self.cap,
            cap:        self.cap,
            buf:        unsafe { allocate(self.cap, size_of::<uint>() ) },
        };

        unsafe { copy_memory(i.buf, self.buf, self.cap); }

        i
    }

    fn resize(&mut self, newcap: uint) {
        unsafe {
            let nbuf = allocate(newcap, size_of::<uint>());

            if newcap <= self.cap {
                copy_memory(nbuf, self.buf, self.cap);
            } else {
                copy_memory(nbuf, self.buf, newcap);
            }

            deallocate(self.buf, self.cap, size_of::<uint>());

            self.cap = newcap;

            if self.len > self.cap {
                self.len = self.cap;
            }
        }
    }
}

pub struct RawMessage {
    i:              Arc<Mutex<Internal>>,   
}

impl Clone for RawMessage {
    fn clone(&self) -> RawMessage {
        RawMessage {
            i:          self.i.clone(),
        }
    }
}

impl RawMessage {
    pub fn new(cap: uint) -> RawMessage {
        RawMessage {i:  Arc::new(Mutex::new(Internal::new(cap)))}
    }

    pub fn dup(&self) -> RawMessage {
        RawMessage {i:  Arc::new(Mutex::new(self.i.lock().dup()))}
    }



    pub fn new_fromstr(s: &str) -> RawMessage {
        let m = RawMessage::new(s.len());
        unsafe {
            copy_memory(m.i.lock().buf, *(transmute::<&&str, *const uint>(&s)) as *const u8, s.len());
        }
        m
    }

    pub fn cap(&self) -> uint {
        self.i.lock().cap
    }

    pub fn len(&self) -> uint {
        self.i.lock().len
    }

    pub fn getbufaddress(&self) -> uint {
        self.i.lock().buf as uint
    }

    pub fn resize(&mut self, newcap: uint) {
        self.i.lock().resize(newcap);
    }

    pub fn write_from_slice(&mut self, mut offset: uint, f: &[u8]) {
        let mut i = self.i.lock();

        if offset + f.len() > i.cap {
            panic!("write past end of buffer");
        }

        for item in f.iter() {
            unsafe { *((i.buf as uint + offset) as *mut u8) = *item };
            offset += 1;
        }

        if offset > i.len {
            i.len = offset;
        }
    }

    pub fn as_slice(&mut self) -> &[u8] {
        unsafe {
            let i = self.i.lock();
            transmute(raw::Slice { data: i.buf as *const u8, len: i.len })
        }
    }

    pub fn writestructref<T>(&mut self, offset: uint, t: &T) {
        let i = self.i.lock();

        if offset + size_of::<T>() > i.cap {
            panic!("write past end of buffer!")
        }

        unsafe { copy_memory((i.buf as uint + offset) as *mut T, transmute(t), 1); }        
    }

    pub fn writestruct<T>(&mut self, offset: uint, t: T) {
        self.writestructref(offset, &t);
    }

    pub fn readstruct<T: NoPointers>(&self, offset: uint) -> T {
        unsafe { self.readstructunsafe(offset) }
    }

    pub fn readstructref<T: NoPointers>(&self, offset: uint, t: &mut T) {
        unsafe { self.readstructunsaferef(offset, t) };
    }

    pub unsafe fn readstructunsafe<T>(&self, offset: uint) -> T {
        let mut out: T = uninitialized::<T>();
        self.readstructunsaferef(offset, &mut out);
        out
    }

    pub unsafe fn readstructunsaferef<T>(&self, offset: uint, t: &mut T) {
        let i = self.i.lock();

        if offset + size_of::<T>() > i.cap {
            panic!("read past end of buffer!")
        }

        copy_memory(t, (i.buf as uint + offset) as *const T, 1);
    }

    pub fn writeu8(&mut self, offset: uint, value: u8) { self.writestruct(offset, value); }
    pub fn writeu16(&mut self, offset: uint, value: u16) { self.writestruct(offset, value); }
    pub fn writeu32(&mut self, offset: uint, value: u32) { self.writestruct(offset, value); }
    pub fn writei8(&mut self, offset: uint, value: i8) { self.writestruct(offset, value); }
    pub fn writei16(&mut self, offset: uint, value: i16) { self.writestruct(offset, value); }
    pub fn writei32(&mut self, offset: uint, value: i32) { self.writestruct(offset, value); }
    pub fn readu8(&self, offset: uint) -> u8 { unsafe { self.readstructunsafe(offset) } }
    pub fn readu16(&self, offset: uint) -> u16 { unsafe { self.readstructunsafe(offset) } }
    pub fn readu32(&self, offset: uint) -> u32 { unsafe { self.readstructunsafe(offset) } }
    pub fn readi8(&self, offset: uint) -> i8 { unsafe { self.readstructunsafe(offset) } }
    pub fn readi16(&self, offset: uint) -> i16 { unsafe { self.readstructunsafe(offset) } }
    pub fn readi32(&self, offset: uint) -> i32 { unsafe { self.readstructunsafe(offset) } }
}
