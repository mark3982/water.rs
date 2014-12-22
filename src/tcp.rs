use std::sync::Arc;
use std::sync::Mutex;

use net::Net;
use net::NetProtocolAddress;

pub type TCPLISTENER = Arc<Mutex<TCPListener>>;
pub type TCPCONNECTOR = Arc<Mutex<TCPConnector>>;

pub struct TCPListener {
    a: uint,
}
pub struct TCPConnector {
    a: uint,
}

impl Sync for TCPListener { }

impl TCPListener {
    pub fn new(net: &Net, host: String, port: u16) -> TCPLISTENER {
        Arc::new(Mutex::new(TCPListener { a: 0 }))
    }
}

impl TCPConnector {
    pub fn new(net: &Net, host: String, port: u16) -> TCPCONNECTOR {
        Arc::new(Mutex::new(TCPConnector { a: 0 }))
    }
}
