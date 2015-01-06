#![feature(slicing_syntax)]

extern crate test;
extern crate water;

use std::sync::Arc;
use std::mem::transmute_copy;

use water::Queue;
use water::get_time;
use water::Timespec;
use water::timespec::add;
use water::timespec::sub;
use water::timespec::NSINSEC;

use water::Net;
use std::cell::UnsafeCell;
use std::thread::Thread;
use std::time::duration::Duration;

#[test]
fn mpscqueue() {
    let start = get_time();
    Thread::spawn(|| mpscqueue_run(100, 1000) );
    let end = get_time();
    let dur = sub(end, start);
}

fn mpscqueue_run(m: uint, n: uint) {
    fn run_pair(n: uint) {
        let q: Queue<u64> = Queue::new();
        let q1: &mut Queue<u64> = unsafe { transmute_copy(&&q) };
        let q2: &mut Queue<u64> = unsafe { transmute_copy(&&q) };
        let q3: &mut Queue<u64> = unsafe { transmute_copy(&&q) };

        println!("-----");

        let ta = Thread::spawn(move || {
            for y in range(0, n) {
                q1.put(y as u64);
            }
            println!("ta exit {:p}", q1);
        });

        let tb = Thread::spawn(move || {
            for y in range(0, n) {
                q2.put((y as u64) + 1000u64);
            }
            println!("tb exit {:p}", q2);
        });

        let tc = Thread::spawn(move || {
            let mut x = 0u;
            while x < n * 2 {
                let val: u64;
                loop {
                    match q3.get() {
                        Some(val) => {
                            println!("got:{}/{} val:{}", x, n * 2, val);
                            x += 1;
                            break;
                        }
                        None => continue,
                    }
                }
            }
            println!("tc exit {:p}", q3);
        });
        drop(q);
    }

    for _ in range(0u, m) {
        run_pair(n);
    }
}