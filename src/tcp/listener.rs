use std::sync::Arc;
use std::sync::Mutex;
use std::intrinsics::transmute;

use std::io::IoError;
use std::result::Result;
use std::vec::Vec;
use std::io::{TcpListener, TcpStream, Listener, Acceptor};
use std::io::net::tcp::TcpAcceptor;
use std::thread::Thread;

use time::Timespec;

use net::ID;
use net::UNUSED_ID;
use endpoint::IoResult;
use endpoint::Endpoint;
use message::Message;
use rawmessage::RawMessage;
use net::Net;
use tcp::thread_rx;
use tcp::thread_tx;
use tcp::TerminateMessage;
use tcp::Which;

pub struct Internal {
    net:                Net,
    addr:               String,
    terminate:          bool,
    clientcount:        u64,
    negcount:           u64,
    acceptor:           Option<TcpAcceptor>,
}

/// This is a listener which handles accepting connections on the TCP
/// protocol from a TcpBridgeConnector. It automatically works with the
/// connector to transport packets both directions providing a bridge
/// between two nets.
pub struct TcpBridgeListener {
    i:                  Arc<Mutex<Internal>>,
}

impl Clone for TcpBridgeListener {
    fn clone(&self) -> TcpBridgeListener {
        TcpBridgeListener {
            i:      self.i.clone(),
        }
    }
}

impl TcpBridgeListener {
    /// Terminates the acceptor logic which causes all future connections
    /// to be rejected. _It should also cleanup RX and TX threads for
    /// existing connections causing them to be dropped, but I think that
    /// is pending implementation._
    pub fn terminate(&mut self) {
        self.i.lock().unwrap().terminate = true;
        if self.i.lock().unwrap().acceptor.is_some() {
            self.i.lock().unwrap().acceptor.as_mut().unwrap().close_accept();
        }
        // We have to exit because the RX, TX, and
        // accept threads might try to take lock,
        // and we will deadlock there.
    }

    /// _(internal)_ This will set the acceptor.
    pub fn setacceptor(&mut self, op: Option<TcpAcceptor>) {
        self.i.lock().unwrap().acceptor = op;
    }

    /// This will determine if this listener has been slated for termination.
    pub fn getterminate(&self) -> bool {
        self.i.lock().unwrap().terminate
    }

    /// Get the address used to listen on. It is a String with the format
    /// "<host/ip>:<port>".
    pub fn getaddr(&self) -> String {
        self.i.lock().unwrap().addr.clone()
    }

    /// _(internal)_ Increment the client count.
    pub fn clientcountinc(&mut self) {
        let mut i = self.i.lock().unwrap();
        i.clientcount += 1;
    }

    /// Get count of TCP clients/connections. Just because a connection is active does
    /// not mean that it has been negotiated therefore you should likely check `getnegcount`
    /// instead. This is just left in for testing primarily.
    pub fn getclientcount(&self) -> u64 {
        self.i.lock().unwrap().clientcount
    }

    pub fn negcountinc(&mut self) {
        let mut i = self.i.lock().unwrap();
        i.negcount += 1;
    }

    /// Get negotiated count. This represents the number of current successfully
    /// negotiated links. It may be lower than `getclientcount()`.
    pub fn getnegcount(&self) -> u64 {
        self.i.lock().unwrap().negcount
    }

    pub fn thread_accept(mut bridge: TcpBridgeListener) {

        let listener = TcpListener::bind(bridge.getaddr().as_slice());
        let mut acceptor = listener.listen().unwrap();

        bridge.setacceptor(Option::Some(acceptor.clone()));
        if bridge.getterminate() {
            return;
        }

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
                    {
                        let net = &mut bridge.i.lock().unwrap().net;
                        let mut ep = Endpoint::new(!0u64, net.get_neweid(), net.clone());
                        // Get unique group ID for control messages.
                        ep.setgid(net.get_neweid());
                        net.add_endpoint(ep.clone());
                        let _stream = stream.clone();
                        let _ep = ep.clone();
                        let _bridge = bridge.clone();
                        Thread::spawn(move || { thread_rx(Which::Listener(_bridge), _ep, _stream) });
                        let _sid = net.getserveraddr();
                        let _bridge = bridge.clone();
                        Thread::spawn(move || { thread_tx(Which::Listener(_bridge), ep, stream, _sid) });
                    }
                    // TODO: make client count decrement on connection lost
                    bridge.clientcountinc();
                }
            }
        }
    }

    pub fn new(net: &Net, addr: String) -> TcpBridgeListener {
        let b = TcpBridgeListener {
            i: Arc::new(Mutex::new(Internal {
                acceptor:       Option::None,
                net:            net.clone(),
                addr:           addr,
                terminate:      false,
                clientcount:    0,
                negcount:       0,
            })),
        };

        let bclone = b.clone();
        Thread::spawn(move || { TcpBridgeListener::thread_accept(bclone) });

        b
    }

}
