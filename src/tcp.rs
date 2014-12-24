use std::sync::Arc;
use std::sync::Mutex;
use std::intrinsics::transmute;

use std::io::{TcpListener, TcpStream};
use std::io::{Acceptor, Listener};

/*
    Trying a new design here were we actually put the
    Arc, Mutex, to usage. My other designs do the locking
    per method call, but once I really thought it I realized
    that having the library caller do something like:

        tcpconn.lock().getcount()

    Is actually not too bad consider this lets the library
    caller decide when to use the lock and when not too. The
    alternative is:

        tcpconn.getcount()

    Even though it looks nicer it still results in the inability
    to group many calls under the same locking call.
*/

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

impl _TcpBridgeListener {
    pub fn thread(s: TcpBridgeListener) {

    }

    pub fn new(net: &Net, host: String, port: u16) -> TcpBridgeListener {
        let n = Arc::new(Mutex::new(_TcpBridgeListener { 
            net:    net.clone(),
            host:   host,
            port:   port,
        }));

        let nclone = n.clone();
        spawn(move || { _TcpBridgeListener::thread(nclone) });

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
