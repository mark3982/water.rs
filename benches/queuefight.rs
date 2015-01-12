#![feature(unsafe_destructor)]
//
// Our goal is to take the queue used by native channels and pit
// it against our queue to see what differences exist and determine
// the performance to use for regression testing and continual 
// improvement.
//
extern crate test;
extern crate water;

use water::Queue;
use std::sync::Arc;
use std::thread::Thread;
use std::thread::JoinGuard;
use test::Bencher;

use std::mem;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicPtr, Ordering};

const COUNT: uint = 1000;

#[bench]
fn queuefight_water1x1(b: &mut Bencher) { b.iter(|| queuefight_water_run(1, 1, COUNT)); }
#[bench]
fn queuefight_water2x1(b: &mut Bencher) { b.iter(|| queuefight_water_run(2, 1, COUNT)); }
#[bench]
fn queuefight_water3x1(b: &mut Bencher) { b.iter(|| queuefight_water_run(3, 1, COUNT)); }
#[bench]
fn queuefight_water4x1(b: &mut Bencher) { b.iter(|| queuefight_water_run(4, 1, COUNT)); }
#[bench]
fn queuefight_water10x1(b: &mut Bencher) { b.iter(|| queuefight_water_run(10, 1, COUNT)); }
#[bench]
fn queuefight_water10x10(b: &mut Bencher) { b.iter(|| queuefight_water_run(10, 10, COUNT)); }
#[bench]
fn queuefight_water1x2(b: &mut Bencher) { b.iter(|| queuefight_water_run(1, 2, COUNT)); }
#[bench]
fn queuefight_water2x2(b: &mut Bencher) { b.iter(|| queuefight_water_run(2, 2, COUNT)); }
#[bench]
fn queuefight_water3x2(b: &mut Bencher) { b.iter(|| queuefight_water_run(3, 2, COUNT)); }
#[bench]
fn queuefight_water4x2(b: &mut Bencher) { b.iter(|| queuefight_water_run(4, 2, COUNT)); }

#[bench]
fn queuefight_native1x1(b: &mut Bencher) { b.iter(|| queuefight_native_run(1, 1, COUNT)); }
#[bench]
fn queuefight_native2x1(b: &mut Bencher) { b.iter(|| queuefight_native_run(2, 1, COUNT)); }
#[bench]
fn queuefight_native3x1(b: &mut Bencher) { b.iter(|| queuefight_native_run(3, 1, COUNT)); }
#[bench]
fn queuefight_native4x1(b: &mut Bencher) { b.iter(|| queuefight_native_run(4, 1, COUNT)); }
#[bench]
fn queuefight_native10x1(b: &mut Bencher) { b.iter(|| queuefight_native_run(10, 1, COUNT)); }

fn queuefight_water_run(txcnt: uint, rxcnt: uint, i: uint) {
    let q: Arc<Queue<u64>> = Arc::new(Queue::new());
    let mut threads: Vec<JoinGuard<()>> = Vec::new();

    fn tx_thread(id: u64, q: Arc<Queue<u64>>, txcnt: uint, rxcnt: uint, i: uint) {
        for num in range(0u, i) {
            q.put(num as u64 | (id << 32));
        }
    }

    fn rx_thread(q: Arc<Queue<u64>>, txcnt: uint, rxcnt: uint, i: uint) {
        let need = (i * txcnt) / rxcnt;
        let mut got: uint = 0;
        while got < need {
            let r = q.get();
            if r.is_none() {
                continue;
            }
            got += 1;
        }
    }

    for id in range(0u, txcnt) {
        let qc = q.clone();
        threads.push(Thread::scoped(move || tx_thread(id as u64, qc, txcnt, rxcnt, i)));
    }

    for _ in range(0u, rxcnt) {
        let qc = q.clone();
        threads.push(Thread::scoped(move || rx_thread(qc, txcnt, rxcnt, i)));
    }

    drop(threads);
}


fn queuefight_native_run(txcnt: uint, rxcnt: uint, i: uint) {
    let q: Arc<NativeQueue<u64>> = Arc::new(NativeQueue::new());
    let mut threads: Vec<JoinGuard<()>> = Vec::new();

    fn tx_thread(id: u64, q: Arc<NativeQueue<u64>>, txcnt: uint, rxcnt: uint, i: uint) {
        for num in range(0u, i) {
            q.push(num as u64 | (id << 32));
        }
    }

    fn rx_thread(q: Arc<NativeQueue<u64>>, txcnt: uint, rxcnt: uint, i: uint) {
        let need = (i * txcnt) / rxcnt;
        let mut got: uint = 0;
        while got < need {
            match q.pop() {
                PopResult::Data(v) => got += 1,
                _ => break,
            }
        }
    }

    for id in range(0u, txcnt) {
        let qc = q.clone();
        threads.push(Thread::scoped(move || tx_thread(id as u64, qc, txcnt, rxcnt, i)));
    }

    for _ in range(0u, rxcnt) {
        let qc = q.clone();
        threads.push(Thread::scoped(move || rx_thread(qc, txcnt, rxcnt, i)));
    }
}

/*
    ---------- COPIED RUST NATIVE STDLIB MPSC QUEUE IMPLEMENTATION ------------
*/

/// A result of the `pop` function.
pub enum PopResult<T> {
    /// Some data has been popped
    Data(T),
    /// The queue is empty
    Empty,
    /// The queue is in an inconsistent state. Popping data should succeed, but
    /// some pushers have yet to make enough progress in order allow a pop to
    /// succeed. It is recommended that a pop() occur "in the near future" in
    /// order to see if the sender has made progress or not
    Inconsistent,
}

struct Node<T> {
    next: AtomicPtr<Node<T>>,
    value: Option<T>,
}

/// The multi-producer single-consumer structure. This is not cloneable, but it
/// may be safely shared so long as it is guaranteed that there is only one
/// popper at a time (many pushers are allowed).
pub struct NativeQueue<T> {
    head: AtomicPtr<Node<T>>,
    tail: UnsafeCell<*mut Node<T>>,
}

unsafe impl<T:Send> Send for NativeQueue<T> { }
unsafe impl<T:Send> Sync for NativeQueue<T> { }

impl<T> Node<T> {
    unsafe fn new(v: Option<T>) -> *mut Node<T> {
        mem::transmute(Box::new(Node {
            next: AtomicPtr::new(0 as *mut Node<T>),
            value: v,
        }))
    }
}

impl<T: Send> NativeQueue<T> {
    /// Creates a new queue that is safe to share among multiple producers and
    /// one consumer.
    pub fn new() -> NativeQueue<T> {
        let stub = unsafe { Node::new(None) };
        NativeQueue {
            head: AtomicPtr::new(stub),
            tail: UnsafeCell::new(stub),
        }
    }

    /// Pushes a new value onto this queue.
    pub fn push(&self, t: T) {
        unsafe {
            let n = Node::new(Some(t));
            let prev = self.head.swap(n, Ordering::AcqRel);
            (*prev).next.store(n, Ordering::Release);
        }
    }

    /// Pops some data from this queue.
    ///
    /// Note that the current implementation means that this function cannot
    /// return `Option<T>`. It is possible for this queue to be in an
    /// inconsistent state where many pushes have succeeded and completely
    /// finished, but pops cannot return `Some(t)`. This inconsistent state
    /// happens when a pusher is pre-empted at an inopportune moment.
    ///
    /// This inconsistent state means that this queue does indeed have data, but
    /// it does not currently have access to it at this time.
    pub fn pop(&self) -> PopResult<T> {
        unsafe {
            let tail = *self.tail.get();
            let next = (*tail).next.load(Ordering::Acquire);

            if !next.is_null() {
                *self.tail.get() = next;
                assert!((*tail).value.is_none());
                assert!((*next).value.is_some());
                let ret = (*next).value.take().unwrap();
                let _: Box<Node<T>> = mem::transmute(tail);
                return PopResult::Data(ret);
            }

            if self.head.load(Ordering::Acquire) == tail {PopResult::Empty} else {PopResult::Inconsistent}
        }
    }
}

#[unsafe_destructor]
impl<T: Send> Drop for NativeQueue<T> {
    fn drop(&mut self) {
        unsafe {
            let mut cur = *self.tail.get();
            while !cur.is_null() {
                let next = (*cur).next.load(Ordering::Relaxed);
                let _: Box<Node<T>> = mem::transmute(cur);
                cur = next;
            }
        }
    }
}