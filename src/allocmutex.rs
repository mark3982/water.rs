use std::sync::Mutex;
use std::mem::transmute;
use std::rt::heap::deallocate;
use std::mem::size_of;
use std::mem::align_of;
use std::sync::MutexGuard;
use std::ptr;

/*
struct Internal {
    a:  uint,
    b:  uint,
}


impl Drop for Internal {
    fn drop(&mut self) {
        println!("dropped");
    }
}

fn main() {
    let m = AllocMutex::new(Internal {
        a: 0,
        b: 0,
    });
    
    m.lock().a = 3;
    m.lock().b = 3;
    
    // optional (leak memory)
    m.free();
}
*/

pub struct AllocMutex<T: Send> {
    m:  Mutex<*mut T>,
}

unsafe impl<T: Send + 'static> Send for AllocMutex<T> { }

impl<T: Send> AllocMutex<T> {
    pub fn new(t: T) -> AllocMutex<T> {
        let r: *mut T = unsafe { transmute(box t) };
    
        AllocMutex {
            m:  Mutex::new(r),
        }
    }
    
    pub fn lock(&self) -> MutexGuard<&mut T> {
        unsafe { transmute( self.m.lock() ) }
    }

    pub fn rawmutref(&self) -> *mut T {
        *self.m.lock()
    }
    
    pub fn mutref(&self) -> &mut T {
        unsafe { transmute( (*self.m.lock()) ) }
    }
    
    pub fn free(&self) {
        unsafe {
            let o: *mut T = transmute(self.mutref());
            drop(ptr::read(&(*o)));
            deallocate(o as *mut u8, size_of::<T>(), align_of::<T>());
        }
    }
}