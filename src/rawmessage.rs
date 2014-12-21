use std::rt::heap::allocate;
use std::rt::heap::deallocate;
use std::mem::size_of;
use std::sync::Mutex;
use std::intrinsics::copy_memory;
use std::intrinsics::transmute;
use std::mem::uninitialized;

pub struct RawMessage {
    pub srcsid:     u64,        // source server id
    pub srceid:     u64,        // source endpoint id
    pub dstsid:     u64,        // destination server id
    pub dsteid:     u64,        // destination endpoint id
    buf:            *mut u8,    // message buffer
    len:            uint,       // specified buffer length
    cap:            uint,       // message buffer length
    refcnt:         *mut uint,  // reference count
}

impl Drop for RawMessage {
    fn drop(&mut self) {
        unsafe {
            //println!("drop called for {:p} with ref cnt {}", self, *self.refcnt);
            if *self.refcnt < 1 {
                panic!("drop called for {:p} with reference count {}!", self, *self.refcnt);
            } 

            *self.refcnt = *self.refcnt - 1;

            if *self.refcnt < 1 {
                if self.cap > 0 {
                    deallocate(self.buf, self.cap, size_of::<uint>());
                }
            }
        }
    }
}

impl Clone for RawMessage {
    fn clone(&self) -> RawMessage {
        
        unsafe {
            *self.refcnt += 1;
        }

        RawMessage {
            srcsid: self.srcsid, srceid: self.srceid,
            dstsid: self.dstsid, dsteid: self.dsteid,
            buf: self.buf,
            cap: self.cap,
            len: self.len,
            refcnt: self.refcnt,
        }
    }
}

impl RawMessage {
    pub fn new(cap: uint) -> RawMessage {
        unsafe {
            let refcnt: *mut uint = allocate(size_of::<uint>(), size_of::<uint>()) as *mut uint;
            *refcnt = 1;

            RawMessage {
                srcsid: 0, srceid: 0, dstsid: 0, dsteid: 0,
                buf: allocate(cap, size_of::<uint>()),
                len: cap,
                cap: cap,
                refcnt: refcnt,
            }
        }
    }

    pub fn cap(&self) -> uint {
        return self.cap;
    }

    pub fn len(&self) -> uint {
        return self.len;
    }

    pub fn dup(&self) -> RawMessage {
        unsafe {
            let refcnt: *mut uint = allocate(size_of::<uint>(), size_of::<uint>()) as *mut uint;
            *refcnt = 1;

            let nbuf: *mut u8 = allocate(self.cap, size_of::<uint>());
            copy_memory(nbuf, self.buf as *const u8, self.cap);

            RawMessage {
                srcsid: self.srcsid, srceid: self.srceid,
                dstsid: self.dstsid, dsteid: self.dsteid,
                buf: nbuf,
                cap: self.cap,
                len: self.len,
                refcnt: refcnt,
            }
        }
    }

    pub fn resize(&mut self, newcap: uint) {
        unsafe {
            if newcap == 0 {
                return;
            }

            let nbuf: *mut u8 = allocate(newcap, size_of::<uint>());
            
            if newcap < self.len {
                copy_memory(nbuf, self.buf as *const u8, newcap);
            } else {
                copy_memory(nbuf, self.buf as *const u8, self.cap);
            }

            deallocate(self.buf, self.cap, size_of::<uint>());
            self.buf = nbuf;

            self.cap = newcap;

            if self.len > self.cap {
                self.len = self.cap;
            }
        }
    }

    pub fn writestructref<T>(&self, offset: uint, t: &T) {
        if offset + size_of::<T>() > self.len {
            panic!("write past end of buffer!")
        }

        unsafe { copy_memory((self.buf as uint + offset) as *mut u8, transmute(t), size_of::<T>()); }        
    }

    pub fn writestruct<T>(&self, offset: uint, t: T) {
        if offset + size_of::<T>() > self.len {
            panic!("write past end of buffer!")
        }

        unsafe { copy_memory((self.buf as uint + offset) as *mut u8, transmute(&t), size_of::<T>()); }
    }

    pub unsafe fn readstruct<T>(&self, offset: uint) -> T {
        let mut out = uninitialized::<T>();

        if offset + size_of::<T>() > self.len {
            panic!("read past end of buffer!")
        }

        copy_memory(&mut out, (self.buf as uint + offset) as *const T, size_of::<T>());

        out
    }

    pub unsafe fn readstructref<T>(&self, offset: uint, t: &mut T) {
        if offset + size_of::<T>() > self.len {
            panic!("read past end of buffer!")
        }

        copy_memory(t, (self.buf as uint + offset) as *const T, size_of::<T>());
    }

    pub fn writeu8(&self, offset: uint, value: u8) { self.writestruct(offset, value); }
    pub fn writeu16(&self, offset: uint, value: u16) { self.writestruct(offset, value); }
    pub fn writeu32(&self, offset: uint, value: u32) { self.writestruct(offset, value); }
    pub fn writei8(&self, offset: uint, value: i8) { self.writestruct(offset, value); }
    pub fn writei16(&self, offset: uint, value: i16) { self.writestruct(offset, value); }
    pub fn writei32(&self, offset: uint, value: i32) { self.writestruct(offset, value); }
    pub fn readu8(&self, offset: uint) -> u8 { unsafe { self.readstruct(offset) } }
    pub fn readu16(&self, offset: uint) -> u16 { unsafe { self.readstruct(offset) } }
    pub fn readu32(&self, offset: uint) -> u32 { unsafe { self.readstruct(offset) } }
    pub fn readi8(&self, offset: uint) -> i8 { unsafe { self.readstruct(offset) } }
    pub fn readi16(&self, offset: uint) -> i16 { unsafe { self.readstruct(offset) } }
    pub fn readi32(&self, offset: uint) -> i32 { unsafe { self.readstruct(offset) } }
}
