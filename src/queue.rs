use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUint;
use std::mem::transmute;
use std::mem::uninitialized;
use std::mem::zeroed;
use std::intrinsics::copy_memory;
use std::rt::heap::deallocate;
use std::mem::size_of;
use std::mem::align_of;

pub struct Queue<T> {
    ptr:        AtomicPtr<Item<T>>,
    lst:        AtomicPtr<Item<T>>,
    rinside:    AtomicUint,
    len:        AtomicUint,
    sent:       AtomicUint,
}

struct Item<T> {
    next:       AtomicPtr<Item<T>>,
    prev:       AtomicPtr<Item<T>>,
    claimed:    AtomicBool,
    needrel:    AtomicBool,
    payload:    T,
}

impl<T> Queue<T> {
    pub fn new() -> Queue<T> {
        Queue {
            ptr:        AtomicPtr::new(0 as *mut Item<T>),
            lst:        AtomicPtr::new(0 as *mut Item<T>),
            rinside:    AtomicUint::new(0),
            len:        AtomicUint::new(0),
            sent:       AtomicUint::new(0),
        }
    }

    pub fn put(&mut self, t: T) {
        let item: *mut Item<T> = unsafe { transmute(box Item {
            next:       AtomicPtr::new(0 as *mut Item<T>),
            prev:       AtomicPtr::new(0 as *mut Item<T>),
            claimed:    AtomicBool::new(true),
            needrel:    AtomicBool::new(false),
            payload:    t,
        }) };

        //println!("put enter");

        let sndx = self.sent.fetch_add(1, Ordering::SeqCst);

        loop {
            let nxt = self.ptr.load(Ordering::SeqCst);
            unsafe { (*item).next.store(nxt, Ordering::SeqCst) };
            // protection from other sender(s)
            if self.ptr.compare_and_swap(nxt, item, Ordering::SeqCst) == nxt {
                if nxt as uint != 0 {
                    unsafe { (*nxt).prev.store(item, Ordering::SeqCst); }
                }
                // release protection from receiver(s) released
                unsafe { (*item).claimed.store(false, Ordering::SeqCst); }
                self.len.fetch_add(1, Ordering::SeqCst);
                //self.lst.compare_and_swap(0 as *mut Item<T>, item, Ordering::SeqCst);
                //println!("put item:{:p} stored and unclaimed nxt:{:p}", item, nxt);
                break;
            }
            //println!("put retry");
        }

        //println!("put exit");
    }

    pub fn len(&self) -> uint {
        self.len.load(Ordering::Relaxed)
    }

    pub fn dbg(&mut self) {
        unsafe {
        let mut cur: *mut Item<T> = self.ptr.load(Ordering::SeqCst);
        println!("queue:dbg:");
        println!("   ptr:{:p}", self.ptr.load(Ordering::SeqCst));
        println!("   lst:{:p}", self.lst.load(Ordering::SeqCst));
        println!("   rinside:{}", self.rinside.load(Ordering::SeqCst));
        println!("   len:{}", self.len.load(Ordering::SeqCst));
        println!("   sent:{}", self.sent.load(Ordering::SeqCst));
        while cur != 0 as *mut Item<T> {
            println!("  item:{:p}", cur);
            println!("     prev:{:p} next:{:p}", (*cur).prev.load(Ordering::SeqCst), (*cur).next.load(Ordering::SeqCst));
            println!("     claimed:{} needrel:{}", (*cur).claimed.load(Ordering::SeqCst), (*cur).needrel.load(Ordering::SeqCst));

            cur = (*cur).next.load(Ordering::SeqCst);
        }
        }
    }

    /// Frees anything after `after` but not `after` itself.
    fn taildealloc(&mut self, after: *mut Item<T>) {
        unsafe {
            let mut cur = (*after).next.load(Ordering::SeqCst);
            while cur != 0 as *mut Item<T> {
                // Deallocate the memory consumed by this entry.
                let ncur = (*cur).next.load(Ordering::SeqCst);
                //deallocate(cur as *mut u8, size_of::<Item<T>>(), align_of::<Item<T>>());
                cur = ncur;
            }
            (*after).next.store(0 as *mut Item<T>, Ordering::SeqCst);
        }
    }

    pub fn get(&mut self) -> Option<T> {
        unsafe {
            //println!("get");
            let mut first = true;
            let rinside = self.rinside.fetch_add(1, Ordering::SeqCst);
            let mut cur: *mut Item<T> = self.lst.load(Ordering::SeqCst);
            if cur == 0 as *mut Item<T> {
                let mut ptr: *mut Item<T> = self.ptr.load(Ordering::SeqCst);
                if ptr != 0 as *mut Item<T> {
                    // Find the tail.
                    while (*ptr).next.load(Ordering::SeqCst) != 0 as *mut Item<T> {
                        ptr = (*ptr).next.load(Ordering::SeqCst);
                    }
                    // We could have a fight so only the first will succeed, and on top of
                    // that if there is a fight both should have the same tail. New items
                    // are only added to the beginning not the end.
                    //
                    // _This could be a normal store instead of cmp_and_swap? ..._
                    self.lst.compare_and_swap(0 as *mut Item<T>, ptr, Ordering::SeqCst);
                    cur = ptr;
                    // Now, continue onward. If we were not first we will still continue
                    // along and fight later below if it happens.
                } else {
                    //println!("none"); self.dbg();
                    self.rinside.fetch_sub(1, Ordering::SeqCst);
                    return Option::None;
                }
            }        
            // If the lst says it needs release, then we can safely assume that
            // everything after the lst can be released, therefore let us release
            // them. This may be called a lot if only one item is left and it is
            // set as the `lst`.
            //if  self.rinside.load(Ordering::SeqCst) == 1 && 
            //    (*cur).needrel.load(Ordering::SeqCst) &&
            //    self.len.load(Ordering::SeqCst) > 10
            {
                //self.taildealloc(cur);
            }

            // If the lst needs a release
            if (*cur).needrel.load(Ordering::SeqCst) {
                // We want to move the last pointer to the previous, but only if
                // the previous is not zero.
                //
                // _Could it be zero? Would that be okay?_
                let nlst = (*cur).prev.load(Ordering::SeqCst);
                if nlst != 0 as *mut Item<T> {
                    self.lst.compare_and_swap(cur, nlst, Ordering::SeqCst);
                }
            }
            // Follow the list backwards trying to claim an item.
            while cur != 0 as *mut Item<T> {
                //            
                //println!("recv cur:{:p}", cur);
                if !(*cur).claimed.compare_and_swap(false, true, Ordering::SeqCst) {
                    self.len.fetch_sub(1, Ordering::Relaxed);
                    // Are we dealing with the last one?
                    if first {
                        // We want to move the last pointer to the previous, but only
                        // if the previous is not zero.
                        // 
                        // _Could it be zero? Would that be okay?_
                        let nlst = (*cur).prev.load(Ordering::SeqCst);
                        if nlst != 0 as *mut Item<T> {
                            self.lst.compare_and_swap(cur, nlst, Ordering::SeqCst);
                        }
                    } else {
                        //println!("recv limbo no go");
                    }
                    // Move payload out, but unsafely since we leave nothing in it's place.
                    let payload: T = uninitialized();
                    copy_memory(transmute(&payload), &mut ((*cur).payload), 1);
                    // Set that we are done using it.
                    (*cur).needrel.store(true, Ordering::SeqCst);
                    self.rinside.fetch_sub(1, Ordering::SeqCst);
                    //println!("recv one"); self.dbg();
                    return Option::Some(payload);
                }
                //println!("recv iter");
                // 
                first = false;
                cur = (*cur).prev.load(Ordering::SeqCst);
            }

            //println!("none unclaimed"); self.dbg();
            self.rinside.fetch_sub(1, Ordering::SeqCst);
            Option::None
        }
    }
}