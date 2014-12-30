extern crate time;
extern crate water;

use water::Net;
use water::Endpoint;
use water::Message;
use time::Timespec;

#[test]
fn tcpio() {
    //loop {
    std::thread::Thread::spawn(move || { tcpio_do(); });
    //}
}

fn tcpio_do() {
    // Create two nets then link then with TCP.
    let mut net1: Net = Net::new(234);
    let ep1 = net1.new_endpoint();
    let mut net2: Net = Net::new(875);
    let ep2 = net2.new_endpoint();

    // This will be asynchronous. So let us wait
    // until it actually completes.
    let mut listener = net1.tcplisten(String::from_str("localhost:34200"));
    let mut connector = net2.tcpconnect(String::from_str("localhost:34200"));

    // Once this happens we can be sure that messages will be routed onto
    // the other side. It means that the connector was connected and it 
    // completed negotiated of the link.
    println!("waiting for connected");
    while !connector.connected() { }
    // This happens once a connection is established, but the link may
    // still not be negotiated. 
    println!("waiting for client count > 0");
    while listener.getclientcount() < 1 { }
    // Once the link is negotiated (listener side), we can continue. The
    // `negcount` stands for `negotiation count`. It can differ from the
    // `clientcount`.
    println!("waiting for negotiation to complete");
    while listener.getnegcount() < 1 { }

    // Send a message to the remote net over TCP.
    println!("sending message");
    let mut msg = Message::new_raw(32);
    msg.dstsid = 875;
    msg.dsteid = 0;
    {
        let rawmsg = msg.get_rawmutref();
        println!("raw buf pointer {}", rawmsg.id());
        let slice = rawmsg.as_mutslice();
        slice[0] = 0x12;
        slice[1] = 0x34;
        slice[2] = 0x56;
        slice[3] = 0x78;
    }
    ep1.send(&mut msg);

    // The message that we sent should arrive or will have already
    // arrived by the time we make this call. It will block for
    // 900 seconds and return an error if no messages was received.
    println!("waiting for message that was sent");
    let result = ep2.recvorblock(Timespec { sec: 900i64, nsec: 0i32 });

    // We need to get the message as a raw type. This will fail if
    // the message is clone or sync type. Then we get a byte slice
    // and check that the contents are correct.
    let rawmsg = result.ok().get_raw();
    let slice = rawmsg.as_slice();

    println!("id:{} {}:{}:{}:{}", rawmsg.id(), slice[0], slice[1], slice[2], slice[3]);

    assert!(slice[0] == 0x12);
    assert!(slice[1] == 0x34);
    assert!(slice[2] == 0x56);
    assert!(slice[3] == 0x78);

    // We can terminate the listener and connector, which will cleanup
    // any resources used by them including any threads.
    println!("terminating listener and connector");
    listener.terminate();
    connector.terminate();
}