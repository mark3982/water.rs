extern crate water;

use std::thread::Thread;
use water::compat::channel;

#[test]
#[should_fail]
fn checkcompat_fails() {
    // Make sure this produces a panic.
    let (tx, rx) = channel::<u32>();
    drop(rx);
    tx.send(3);

}

#[test]
fn checkcompat() {
    let (tx, rx) = channel::<u32>();

    // Can send and receive multiple messages.
    tx.send(3);
    assert!(rx.recv() == 3);
    tx.send(0x12345678);
    assert!(rx.recv() == 0x12345678);
    tx.send(100);
    assert!(rx.recv() == 100);

    // We get an error when no messages.
    assert!(rx.try_recv().is_err());

    // Receive errors when no peers listening.
    drop(tx);
    assert!(rx.recv_opt().is_err());
    drop(rx);

    // Make sure this produces an error.
    let (tx, rx) = channel::<u32>();
    drop(rx);
    assert!(tx.send_opt(3).is_err());
}