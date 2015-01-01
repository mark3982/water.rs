extern crate water;
extern crate time;

use water::Net;
use time::Timespec;
use std::thread::Thread;

struct Foo;

#[test]
fn pingpong_test() {
    pingpong_water(1, 10);
}

fn main() {
    pingpong_water(4, 10000);
}

fn pingpong_native(m: uint, n: uint) {
    // Create pairs of tasks that pingpong back and forth.
    fn run_pair(n: uint) {
        // Create a stream A->B
        let (atx, arx) = channel::<()>();
        // Create a stream B->A
        let (btx, brx) = channel::<()>();

        spawn(move|| {
            let (tx, rx) = (atx, brx);
            for _ in range(0, n) {
                tx.send(());
                rx.recv();
            }
        });

        spawn(move|| {
            let (tx, rx) = (btx, arx);
            for _ in range(0, n) {
                rx.recv();
                tx.send(());
            }
        });
    }

    for _ in range(0, m) {
        run_pair(n)
    }
}

fn pingpong_water(m: uint, n: uint) {

    fn run_pair(n: uint) {
        let mut net = Net::new(100);

        let epa = net.new_endpoint();
        let epb = net.new_endpoint();

        let ta = Thread::spawn(move || {
            for _ in range(0, n) {
                epa.sendsynctype(());
                epa.recvorblock( Timespec { sec: 9i64, nsec: 0i32 } ).ok();
            }
        });

        let tb = Thread::spawn(move || {
            for _ in range(0, n) {
                epb.recvorblock( Timespec { sec: 9i64, nsec: 0i32 } ).ok();
                epb.sendsynctype(());
            }
        });

        drop(ta);
        drop(tb);
    }

    for _ in range(0u, m) {
        run_pair(n);
    }
}