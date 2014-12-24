use std::sync::Arc;
use std::sync::Mutex;
use std::intrinsics::transmute;

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

pub struct _TcpListener {
    net:    Net,

}
pub struct _TcpConnector {
    net:    Net,
}

pub type TcpListener = Arc<Mutex<_TcpListener>>;
pub type TcpConnector = Arc<Mutex<_TcpConnector>>;

impl _TcpListener {
    pub fn thread(s: TcpListener) {
    }

    pub fn new(net: &Net, host: String, port: u16) -> TcpListener {
        let n = Arc::new(Mutex::new(_TcpListener { net: net.clone() }));
        let nclone = n.clone();
        spawn(move || { _TcpListener::thread(nclone) });

        n
    }

}

impl _TcpConnector {
    pub fn thread(s: TcpConnector) {
    }

    pub fn new(net: &Net, host: String, port: u16) -> TcpConnector {
        let n = Arc::new(Mutex::new(_TcpConnector { net: net.clone() }));
        let nclone = n.clone();
        spawn(move || { _TcpConnector::thread(nclone)});

        n
    }
}
