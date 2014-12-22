use std::sync::Arc;
use std::sync::Mutex;
use std::intrinsics::transmute;

use net::Net;
use net::NetProtocolAddress;

pub type TCPLISTENER = Arc<RacyCell<TCPListener>>;
pub type TCPCONNECTOR = Arc<RacyCell<TCPConnector>>;

pub struct RacyCell<T> {
    t:      T
}

impl<T> Deref<T> for RacyCell<T> {
    fn deref<'a>(&'a self) -> &'a T {
        &self.t
    }
}

// Trying a new design here. It is dangerous to have to maintain
// a seperate inner structure just to gain clone functionality.
// Since Arc provides it we can use it, but Arc restricts the deref
// to immutable. So this is my workaround to being able to use Arc.
impl<T> RacyCell<T> {
    pub fn new(t: T) -> RacyCell<T> {
        RacyCell {t: t}
    }

    // This breaks mutability/immutability because it lets you take
    // anything that is immutable and produce an mutable reference.
    fn mutref(&self) -> &mut T {
        let p: &mut RacyCell<T>;

        p = unsafe { transmute(self) };

        &mut p.t
    }
}

pub struct TCPListener {
    net:    Net,
}
pub struct TCPConnector {
    net:    Net,
}

impl TCPListener {
    pub fn thread(s: TCPLISTENER) {

    }

    pub fn new(net: &Net, host: String, port: u16) -> TCPLISTENER {
        let n = Arc::new(RacyCell::new(TCPListener { net: net.clone() }));
        let nclone = n.clone();
        spawn(move || { TCPListener::thread(nclone) });

        n
    }

}

impl TCPConnector {
    pub fn thread(s: TCPCONNECTOR) {

    }

    pub fn new(net: &Net, host: String, port: u16) -> TCPCONNECTOR {
        let n = Arc::new(RacyCell::new(TCPConnector { net: net.clone() }));
        let nclone = n.clone();
        spawn(move || { TCPConnector::thread(nclone)});

        n
    }
}
