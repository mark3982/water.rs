use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUint;
use std::sync::Mutex;
use std::mem::transmute;
use std::mem::uninitialized;
use std::mem::zeroed;
use std::intrinsics::copy_memory;
use std::rt::heap::deallocate;
use std::rt::heap::allocate;
use std::mem::size_of;
use std::mem::align_of;
use std::vec::Vec;

struct PointerCache<T> {
    block:      uint,
    mask:       uint,
    pos:     AtomicUint,
}

#[unsafe_destructor]
impl<T> Drop for PointerCache<T> {
    fn drop(&mut self) {
        for pos in range(0u, self.mask + 1) {
            // Will end up clearing/deallocating everything.
            self.push(0 as *mut T);
            // TODO: call destructor on each AtomicPtr<*mut T> item
        }
        unsafe { deallocate(self.block as *mut u8, size_of::<AtomicPtr<*mut T>>() * (self.mask + 1), align_of::<AtomicPtr<*mut T>>()); }
    }
}

impl<T> PointerCache<T> {
    pub fn new<T>(bsize: uint) -> PointerCache<T> {
        let count = !(!0u << bsize) + 1;
        let itc = PointerCache {
            block:  unsafe { transmute(allocate(size_of::<AtomicPtr<T>>() * count, align_of::<AtomicPtr<T>>())) },
            mask:   !(!0u << bsize),
            pos:    AtomicUint::new(0),
        };

        for pos in range(0u, count) { 
            itc.init(pos);
        }

        itc
    }

    fn init(&self, pos: uint) {
        unsafe { *((self.block + size_of::<AtomicPtr<T>>() * pos) as *mut AtomicPtr<T>) = AtomicPtr::new(0 as *mut T); }
    }

    fn get(&self, pos: uint) -> &AtomicPtr<T> {
        unsafe { transmute((self.block + size_of::<AtomicPtr<T>>() * pos) as *mut AtomicPtr<T>) }
    }

    pub fn pop(&self) -> Option<*mut T> {
        let pos = (self.pos.fetch_sub(1, Ordering::Relaxed) - 1) & self.mask;

        let old = self.get(pos).swap(0 as *mut T, Ordering::Relaxed);

        if old != 0 as *mut T {
            //println!("pop {} {:p}", pos, old);
            Option::Some(old)
        } else {
            //println!("pop {} none", pos);
            Option::None
        }
    }

    pub fn push(&self, new: *mut T) {
        let pos = self.pos.fetch_add(1, Ordering::Relaxed) & self.mask;

        let old = self.get(pos).swap(new, Ordering::Relaxed);

        if old != 0 as *mut T {
            //println!("replaced {} {:p} over {:p}", pos, new, old);
            unsafe { deallocate(old as *mut u8, size_of::<T>(), align_of::<T>()); }
        } else {
            //println!("push {} {:p}", pos, new);
        }
    }
}

pub struct Queue<T> {
    ptr:        AtomicPtr<Item<T>>,
    lst:        AtomicPtr<Item<T>>,
    rinside:    AtomicUint,
    len:        AtomicUint,
    sent:       AtomicUint,
    needrel:    AtomicUint,
    doingrel:   AtomicBool,
    recycle:    PointerCache<Item<T>>,
}

struct Item<T> {
    next:       AtomicPtr<Item<T>>,
    prev:       AtomicPtr<Item<T>>,
    claimed:    AtomicBool,
    needrel:    AtomicBool,
    payload:    T,
}

#[unsafe_destructor]
impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        self.forcedealloc();
    }
}

pub struct SafeQueue<T> {
    vec:    Mutex<Vec<T>>,
}

impl<T: Send> SafeQueue<T> {
    pub fn new() -> SafeQueue<T> {
        SafeQueue {
            vec:        Mutex::new(Vec::new()),
        }
    }

    pub fn put(&self, t: T) {
        self.vec.lock().unwrap().push(t)
    }

    pub fn get(&self) -> Option<T> {
        let mut lock = self.vec.lock().unwrap();
        if lock.len() < 1 {
            Option::None
        } else {
            Option::Some(lock.remove(0))
        }
    }

    pub fn len(&self) -> uint {
        self.vec.lock().unwrap().len()
    }
}

impl<T> Queue<T> {
    pub fn new() -> Queue<T> {
        Queue {
            ptr:        AtomicPtr::new(0 as *mut Item<T>),
            lst:        AtomicPtr::new(0 as *mut Item<T>),
            rinside:    AtomicUint::new(0),
            len:        AtomicUint::new(0),
            sent:       AtomicUint::new(0),
            needrel:    AtomicUint::new(0),
            doingrel:   AtomicBool::new(false),
            recycle:    PointerCache::<Item<T>>::new(10),
        }
    }

    pub fn put(&self, t: T) {
        unsafe {
            let item: *mut Item<T> = match self.recycle.pop() {
                Option::Some(item) => {
                    (*item).claimed.store(false, Ordering::Relaxed);
                    (*item).needrel.store(false, Ordering::Relaxed);
                    (*item).next.store(0 as *mut Item<T>, Ordering::Relaxed);
                    (*item).prev.store(0 as *mut Item<T>, Ordering::Relaxed);
                    copy_memory(&mut ((*item).payload), transmute(&t), 1);
                    item
                },
                Option::None => {
                    transmute(box Item {
                        next:       AtomicPtr::new(0 as *mut Item<T>),
                        prev:       AtomicPtr::new(0 as *mut Item<T>),
                        claimed:    AtomicBool::new(false),
                        needrel:    AtomicBool::new(false),
                        payload:    t,
                    })
                }
            };

            let sndx = self.sent.fetch_add(1, Ordering::SeqCst);

            let oldhead = self.ptr.swap(item, Ordering::SeqCst);

            (*item).next.store(oldhead, Ordering::SeqCst);

            if oldhead != 0 as *mut Item<T> {
                (*oldhead).prev.store(item, Ordering::SeqCst);
            } else {
                self.lst.store(item, Ordering::SeqCst);
            }

            self.len.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn len(&self) -> uint {
        self.len.load(Ordering::Relaxed)
    }

    pub fn dbg(&self) {
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
    fn taildealloc(&self, after: *mut Item<T>) {
        unsafe {
            let mut cur = (*after).next.load(Ordering::SeqCst);
            while cur != 0 as *mut Item<T> {
                // Deallocate the memory consumed by this entry.
                let ncur = (*cur).next.load(Ordering::SeqCst);
                self.recycle.push(cur);
                //deallocate(cur as *mut u8, size_of::<Item<T>>(), align_of::<Item<T>>());
                cur = ncur;
            }
            (*after).next.store(0 as *mut Item<T>, Ordering::SeqCst);
        }
    }

    fn forcedealloc(&self) {
        let after = self.ptr.load(Ordering::SeqCst);
        let mut cnt: uint = 0;
        self.ptr.store(0 as *mut Item<T>, Ordering::SeqCst);
        self.lst.store(0 as *mut Item<T>, Ordering::SeqCst);
        unsafe {
            let mut cur = (*after).next.load(Ordering::SeqCst);
            while cur != 0 as *mut Item<T> {
                // Deallocate the memory consumed by this entry.
                let ncur = (*cur).next.load(Ordering::SeqCst);
                //deallocate(cur as *mut u8, size_of::<Item<T>>(), align_of::<Item<T>>());
                self.recycle.push(cur);
                cur = ncur;
                cnt += 1;
            }
            (*after).next.store(0 as *mut Item<T>, Ordering::SeqCst);
            self.needrel.fetch_sub(cnt, Ordering::SeqCst);
        }
    }

    pub fn get(&self) -> Option<T> {
        unsafe {
            //println!("get");
            let mut first = true;
            let rinside = self.rinside.fetch_add(1, Ordering::SeqCst);
            let mut cur: *mut Item<T> = self.lst.load(Ordering::SeqCst);
            if cur == 0 as *mut Item<T> {
                return Option::None;
            }

            
            if  self.rinside.load(Ordering::SeqCst) == 1 && 
                //self.needrel.load(Ordering::Relaxed) > 200 &&
                (*cur).needrel.load(Ordering::SeqCst)
            { 
                if !self.doingrel.compare_and_swap(false, true, Ordering::SeqCst) {
                    self.taildealloc(cur);
                    self.doingrel.store(false, Ordering::SeqCst);
                }
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
                cur = nlst;
            }

            // Follow the list backwards trying to claim an item.
            while cur != 0 as *mut Item<T> {
                if !(*cur).claimed.compare_and_swap(false, true, Ordering::SeqCst) {
                    self.len.fetch_sub(1, Ordering::Relaxed);
                    if first {
                        // We want to move the last pointer to the previous, but only
                        // if the previous is not zero.
                        // 
                        // _Could it be zero? Would that be okay?_
                        let nlst = (*cur).prev.load(Ordering::SeqCst);
                        if nlst != 0 as *mut Item<T> {
                            self.lst.compare_and_swap(cur, nlst, Ordering::SeqCst);
                        } 
                    }
                    let payload: T = uninitialized();
                    copy_memory(transmute(&payload), &mut ((*cur).payload), 1);
                    (*cur).needrel.store(true, Ordering::SeqCst);
                    self.needrel.fetch_add(1, Ordering::SeqCst);
                    self.rinside.fetch_sub(1, Ordering::SeqCst);
                    //println!("recv one");
                    return Option::Some(payload);
                }
                first = false;
                cur = (*cur).prev.load(Ordering::SeqCst);
            }

            //println!("none unclaimed"); self.dbg();
            self.rinside.fetch_sub(1, Ordering::SeqCst);
            Option::None
        }
    }
}