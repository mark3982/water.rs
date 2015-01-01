extern crate test;
extern crate water;

use std::mem::transmute_copy;

use test::Bencher;

use water::get_time;
use water::Timespec;
use water::timespec::add;
use water::timespec::sub;
use water::timespec::NSINSEC;

use water::Net;
use std::thread::Thread;
use std::time::duration::Duration;

const BIGSIZE: uint = 1024;

pub struct BencherHack {
    iterations: u64,
    dur:        Duration,
    bytes:      u64,
}

#[bench]
fn pingpong_water(b: &mut Bencher) {
    let h: &mut BencherHack = unsafe { transmute_copy(&b) };

    let start = get_time();
    pingpong_water_run(1, 1000);
    let end = get_time();
    let dur = sub(end, start);

    h.iterations = 1000;
    h.dur = Duration::nanoseconds(dur.sec * NSINSEC + dur.nsec as i64);
    h.bytes = 0;
}

#[bench]
fn pingpong_native(b: &mut Bencher) {
    let h: &mut BencherHack = unsafe { transmute_copy(&b) };

    let start = get_time();
    pingpong_native_run(1, 1000);
    let end = get_time();
    let dur = sub(end, start);

    h.iterations = 1000;
    h.dur = Duration::nanoseconds(dur.sec * NSINSEC + dur.nsec as i64);
    h.bytes = 0;
}

enum NativeFoo {
    Apple,
    Grape([u8, ..BIGSIZE]),
}

fn pingpong_native_run(m: uint, n: uint) {
    // Create pairs of tasks that pingpong back and forth.
    fn run_pair(n: uint) {
        // Create a stream A->B
        let (atx, arx) = channel::<NativeFoo>();
        // Create a stream B->A
        let (btx, brx) = channel::<NativeFoo>();

        let ta = Thread::spawn(move|| {
            let (tx, rx) = (atx, brx);
            for _ in range(0, n) {
                tx.send(NativeFoo::Apple);
                rx.recv();
            }
        });

        let tb = Thread::spawn(move|| {
            let (tx, rx) = (btx, arx);
            for _ in range(0, n) {
                rx.recv();
                tx.send(NativeFoo::Grape([0u8, ..BIGSIZE]));
            }
        });

        drop(ta);
        drop(tb);
    }

    for _ in range(0, m) {
        run_pair(n)
    }
}

struct FooApple;
struct FooGrape {
    field:  [u8, ..BIGSIZE],
}

fn pingpong_water_run(m: uint, n: uint) {
    fn run_pair(n: uint) {
        let mut net = Net::new(100);
        let epa = net.new_endpoint();
        let epb = net.new_endpoint();

        let ta = Thread::spawn(move || {
            for _ in range(0, n) {
                epa.sendsynctype(FooApple);
                epa.recvorblockforever().ok();
            }
        });

        let tb = Thread::spawn(move || {
            for _ in range(0, n) {
                epb.recvorblockforever().ok();
                epb.sendsynctype(FooGrape { field: [0u8, ..BIGSIZE] });
            }
        });

        drop(ta);
        drop(tb);
    }

    for _ in range(0u, m) {
        run_pair(n);
    }
}