use std::sync::Arc;
use std::sync::Mutex;
use std::intrinsics::transmute;

use std::io::IoError;
use std::result::Result;
use std::vec::Vec;
use std::io::{TcpListener, TcpStream};
use std::io::{Acceptor, Listener};

/*
    Trying a new design here were we actually put the
    Arc, Mutex, to usage. My other designs do the locking
    per method call, but once I really thought it I realized
    that having the library caller do something like:

        tcpconn.lock().method()

    Is actually not too bad consider this lets the library
    caller decide when to use the lock and when not too. The
    alternative is:

        tcpconn.getcount()

    Even though it looks nicer it still results in the inability
    to group many calls under the same locking call. The only
    other problem is that all methods become locking but hopefully
    that will be okay here.
*/

use endpoint::IoResult;
use endpoint::Endpoint;
use message::Message;
use rawmessage::RawMessage;
use net::Net;

pub struct _TcpBridgeListener {
    net:    Net,
    host:   String,
    port:   u16,

}
pub struct _TcpBridgeConnector {
    net:    Net,
}

pub type TcpBridgeListener = Arc<Mutex<_TcpBridgeListener>>;
pub type TcpBridgeConnector = Arc<Mutex<_TcpBridgeConnector>>;

fn getok<T, E>(result: Result<T, E>) -> T {
    match result {
        Ok(r) => r,
        Err(e) => panic!("I/O error"),
    }
}

#[repr(C)]
struct StreamMessageHeader {
    xtype:      u8,   // raw, sync, clone, ...
    srcsid:     u64,
    srceid:     u64,
    dstsid:     u64,
    dsteid:     u64,
}

fn thread_rx(mut bridge: TcpBridgeListener, mut ep: Endpoint, mut stream: TcpStream) {
    // The first message will be the remote net ID.
    let rsid: u64 = getok(stream.read_be_u64());

    // This is used to catch messages directed to go only onto the
    // remote net, or for broadcast messages.
    ep.sid = rsid;

    loop {
        // Read a single message from the stream.
        let mut msgsize: u64 = getok(stream.read_be_u64());

        let msg_type: u8 = getok(stream.read_u8());
        let msg_srcsid: u64 = getok(stream.read_be_u64());
        let msg_srceid: u64 = getok(stream.read_be_u64());
        let msg_dstsid: u64 = getok(stream.read_be_u64());
        let msg_dsteid: u64 = getok(stream.read_be_u64());
        msgsize -= 1 + 8 * 4;

        // Read the actual raw message part of the message.
        let mut vbuf: Vec<u8> = Vec::with_capacity(msgsize as uint);
        stream.read_at_least(msgsize as uint, vbuf.as_mut_slice());

        // We currently only support raw messages at the moment, since
        // there is no way possible at this time to support sync or clone
        // type messages.
        if msg_type != 1u8 {
            // Just ignore the message. But, for debugging lets panic!
            panic!("got message type {} instead of 1", msg_type);
            continue;
        }

        // Create a raw message.
        let mut rmsg = RawMessage::new(msgsize as uint);
        rmsg.write_from_slice(0, vbuf.as_slice());

        // Create the actual message, and transfer the source and
        // destination fields over.
        let mut msg = Message::new_fromraw(rmsg);
        msg.dstsid = msg_dstsid;
        msg.dsteid = msg_dsteid;
        msg.srcsid = msg_srcsid;
        msg.srceid = msg_srceid;

        // We need to place the message onto the net so that that it can
        // be routed to its one or more destinations.
        ep.send(&msg);
    }
}

fn thread_tx(mut bridge: TcpBridgeListener, mut ep: Endpoint, mut stream: TcpStream) {
    stream.write_be_u64(bridge.lock().net.sid);

    loop {
        let result = ep.recvorblock(Timespec { sec: 900i64, nsec: 0i64 })
        if result.is_err() {
            continue;
        }

        let msg = result.ok();
    }
}


impl _TcpBridgeListener {
    pub fn thread_accept(mut bridge: TcpBridgeListener) {
        let addr = format!("{}:{}", bridge.lock().host, bridge.lock().port);
        let listener = TcpListener::bind(addr.as_slice());
        let mut acceptor = listener.listen();

        for stream in acceptor.incoming() {
            match stream {
                Err(e) => continue,
                Ok(stream) => {
                    // Since Rust plays so nicely with concurrency it is
                    // really easy to just spawn two threads and let one
                    // do TX and the other RX. This is not the best 
                    // performance, but if the needed arises we can always
                    // come back and do it faster (optimize).

                    // The same endpoint is shared between RX and TX.
                    let ep = Endpoint::new(!0u64, bridge.lock().net.get_neweid(), bridge.lock().net.clone());
                    bridge.lock().net.add_endpoint(ep.clone());
                    let _bridge = bridge.clone();
                    let _stream = stream.clone();
                    let _ep = ep.clone();
                    spawn(move || { thread_rx(_bridge, _ep, _stream) });
                    let _bridge = bridge.clone();
                    spawn(move || { thread_rx(_bridge, ep, stream) });
                }
            }
        }
    }

    pub fn new(net: &Net, host: String, port: u16) -> TcpBridgeListener {
        let n = Arc::new(Mutex::new(_TcpBridgeListener { 
            net:    net.clone(),
            host:   host,
            port:   port,
        }));

        let nclone = n.clone();
        spawn(move || { _TcpBridgeListener::thread_accept(nclone) });

        n
    }

}

impl _TcpBridgeConnector {
    pub fn thread(s: TcpBridgeConnector) {
    }

    pub fn new(net: &Net, host: String, port: u16) -> TcpBridgeConnector {
        let n = Arc::new(Mutex::new(_TcpBridgeConnector { net: net.clone() }));
        let nclone = n.clone();
        spawn(move || { _TcpBridgeConnector::thread(nclone)});

        n
    }
}
