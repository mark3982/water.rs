use std::sync::Arc;
use std::sync::Mutex;
use std::intrinsics::transmute;

use std::io::IoError;
use std::result::Result;
use std::vec::Vec;
use std::io::{TcpListener, TcpStream};
use std::io::{Acceptor, Listener};
use std::thread::Thread;

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
use time::Timespec;

use net::ID;
use net::UNUSED_ID;
use endpoint::IoResult;
use endpoint::Endpoint;
use message::Message;
use rawmessage::RawMessage;
use net::Net;

pub struct TerminateMessage;

pub struct _TcpBridgeListener {
    net:        Net,
    host:       String,
    port:       u16,
    terminate:  bool,

}
pub struct _TcpBridgeConnector {
    net:        Net,
    host:       String,
    port:       u16,
    ep:         Option<Endpoint>,
    terminate:  bool,
    gid:        ID,
}

impl _TcpBridgeConnector {
    pub fn terminate(&mut self) {
        // Set termination flag to catch connection loop.
        self.terminate = true;
        if self.ep.is_none() {
            // Early exit. It seems the main thread has not had
            // time to set these values so it still has not spawned
            // and RX or TX, hopefully, so lets just tell it to exit.
            return;
        }
        // Send a message to catch active TX thread which
        // will make the RX thread terminate.
        self.ep.as_mut().unwrap().give(&Message::new_sync(TerminateMessage));

        // We can wait for it to terminate inside this method
        // because it a lock was acquired and the thread may
        // need to check `terminate` so it will try to lock it.
        // This is a side-effect of using Mutex<T> instead of
        // per method locking like with Net, Endpoint, and friends.
        // -- kmcg3413@gmail.com
    }
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

fn thread_rx(mut ep: Endpoint, mut stream: TcpStream) {
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

fn thread_tx(mut ep: Endpoint, mut stream: TcpStream, sid: ID) {
    stream.write_be_u64(sid);

    loop {
        let result = ep.recvorblock(Timespec { sec: 900i64, nsec: 0i32 });

        if result.is_err() {
            continue;
        }

        let msg = result.ok();

        // Check for termination message.
        if msg.is_type::<TerminateMessage>() {
            // This should cause the RX thread to terminate.
            stream.close_read();
            stream.close_write();
            return;
        }

        // We only forward raw messages. We do not support the ability to
        // properly send sync and clone messages (both because they may
        // contain pointers which we can not properly handle). And, the
        // way they would be expected to work even if we could send them
        // would not be able to work.
        if !msg.is_raw() {
            continue;
        }

        let srcsid = msg.srcsid;
        let srceid = msg.srceid;
        let dstsid = msg.dstsid;
        let dsteid = msg.dsteid;

        let mut rmsg = msg.get_raw();

        stream.write_be_u64((1 + 8 * 4 + rmsg.len()) as u64);
        stream.write_u8(1u8);
        stream.write_be_u64(srcsid);
        stream.write_be_u64(srceid);
        stream.write_be_u64(dstsid);
        stream.write_be_u64(dsteid);
        stream.write(rmsg.as_slice());
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
                    let mut ep = Endpoint::new(!0u64, bridge.lock().net.get_neweid(), bridge.lock().net.clone());
                    // Get unique group ID for control messages.
                    ep.gid = bridge.lock().net.get_neweid();
                    bridge.lock().net.add_endpoint(ep.clone());
                    let _stream = stream.clone();
                    let _ep = ep.clone();
                    Thread::spawn(move || { thread_rx(_ep, _stream) }).detach();
                    let _sid = bridge.lock().net.getserveraddr();
                    Thread::spawn(move || { thread_tx(ep, stream, _sid) }).detach();
                }
            }
        }
    }

    pub fn new(net: &Net, host: String, port: u16) -> TcpBridgeListener {
        let n = Arc::new(Mutex::new(_TcpBridgeListener { 
            net:        net.clone(),
            host:       host,
            port:       port,
            terminate:  false,
        }));

        let nclone = n.clone();
        Thread::spawn(move || { _TcpBridgeListener::thread_accept(nclone) }).detach();

        n
    }

}

impl _TcpBridgeConnector {
    pub fn thread(bridge: TcpBridgeConnector) {
        // This thread will be short-lived but to prevent us from
        // blocking the calling thread. It should be easier to add
        // in blocking if that is desired.
        loop {
            let result = TcpStream::connect(format!("{}:{}", bridge.lock().host, bridge.lock().port).as_slice());

            if result.is_err() {
                // Just keep trying, unless instructed to terminate.
                if bridge.lock().terminate {
                    return;
                }
                continue;
            }

            let stream = result.unwrap();

            if bridge.lock().terminate {
                return;
            }

            // The same endpoint is shared between RX and TX.
            let mut ep = Endpoint::new(!0u64, bridge.lock().net.get_neweid(), bridge.lock().net.clone());
            // Get unique group ID for control messages.
            ep.gid = bridge.lock().net.get_neweid();            
            bridge.lock().net.add_endpoint(ep.clone());

            if bridge.lock().terminate {
                return;
            }

            // Spawn RX and TX
            let _ep = ep.clone();
            let _stream = stream.clone();
            let rxthread = Thread::spawn(move || { thread_rx(_ep, _stream); });
            let _ep = ep.clone();
            let _sid = bridge.lock().net.getserveraddr();
            let txthread = Thread::spawn(move || { thread_tx(_ep, stream, _sid); });

            // Set endpoint into bridge.
            bridge.lock().ep = Option::Some(ep);

            // Wait for RX and TX to terminate.. then try connection
            // again until we are requested to terminate.
            rxthread.join();
            txthread.join();

            // The TX may have terminated the RX and we will make it
            // here. We need to check the terminate flag to see if 
            // we also need to exit.
            if bridge.lock().terminate {
                return;
            }
        }
    }

    pub fn new(net: &Net, host: String, port: u16) -> TcpBridgeConnector {
        let n = Arc::new(Mutex::new(_TcpBridgeConnector { 
            net:        net.clone(),
            terminate:  false,
            ep:         Option::None,
            host:       host,
            port:       port,
            gid:        UNUSED_ID,
        }));
        let nclone = n.clone();
        let mainthread = Thread::spawn(move || { _TcpBridgeConnector::thread(nclone)});
        mainthread.detach();

        n
    }
}
