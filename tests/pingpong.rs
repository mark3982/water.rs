extern crate water;
extern crate time;

use water::Net;
use time::Timespec;
use std::thread::Thread;

struct Foo;

#[test]
fn pingpong() {
    pingpong_bench(4, 10000);
}

/*
    rust channels
        real 19.823
        user 4.856
        sys 34.598
    water
        real 9.397
        user 0.220
        sys 7.492
*/

fn main() {
    pingpong_bench(4, 10000);
}

fn pingpong_bench(m: uint, n: uint) {

    fn run_pair(n: uint) {
        let mut net = Net::new(100);

        let epa = net.new_endpoint();
        let epb = net.new_endpoint();

        let ta = Thread::spawn(move || {
            for _ in range(0, n) {
                epa.sendsynctype(Foo);
                epa.recvorblock( Timespec { sec: 9i64, nsec: 0i32 } ).ok();
            }
        });

        let tb = Thread::spawn(move || {
            for _ in range(0, n) {
                epb.recvorblock( Timespec { sec: 9i64, nsec: 0i32 } ).ok();
                epb.sendsynctype(Foo);
            }
        });

        drop(ta);
        drop(tb);
    }

    for _ in range(0u, m) {
        run_pair(n);
    }
}