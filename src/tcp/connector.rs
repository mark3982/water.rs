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

struct Internal {
    net:            Net,
    addr:           String,
    ep:             Option<Endpoint>,
    terminate:      bool,
    gid:            ID,
    pub connected:  bool,
}

pub struct TcpBridgeConnector {
    i:              Arc<Mutex<Internal>>,
}

impl Clone for TcpBridgeConnector {
    fn clone(&self) -> TcpBridgeConnector {
        TcpBridgeConnector {
            i:      self.i.clone(),
        }
    }
}

impl TcpBridgeConnector {
    pub fn terminate(&mut self) {
        // Set termination flag to catch connection loop.
        let mut i = self.i.lock();
        i.terminate = true;
        if i.ep.is_none() {
            // Early exit. It seems the main thread has not had
            // time to set these values so it still has not spawned
            // and RX or TX, hopefully, so lets just tell it to exit.
            return;
        }
        // Send a message to catch active TX thread which
        // will make the RX thread terminate.
        i.ep.as_mut().unwrap().give(&Message::new_clone(TerminateMessage));

        // We can wait for it to terminate inside this method
        // because it a lock was acquired and the thread may
        // need to check `terminate` so it will try to lock it.
        // This is a side-effect of using Mutex<T> instead of
        // per method locking like with Net, Endpoint, and friends.
        // -- kmcg3413@gmail.com
    }

    pub fn connected(&self) -> bool {
        self.i.lock().connected
    }

    pub fn setconnected(&mut self, connected: bool) {
        self.i.lock().connected = connected;
    }

    pub fn thread(bridge: TcpBridgeConnector) {
        // This thread will be short-lived but to prevent us from
        // blocking the calling thread. It should be easier to add
        // in blocking if that is desired.
        loop {
            let result = TcpStream::connect(bridge.i.lock().addr.as_slice());

            if result.is_err() {
                // Just keep trying, unless instructed to terminate.
                if bridge.i.lock().terminate {
                    return;
                }
                continue;
            }

            let stream = result.unwrap();

            if bridge.i.lock().terminate {
                return;
            }

            // The same endpoint is shared between RX and TX.
            let mut lock = bridge.i.lock();
            let mut ep = Endpoint::new(!0u64, lock.net.get_neweid(), lock.net.clone());
            drop(lock);
            // Get unique group ID for control messages.
            ep.setgid(bridge.i.lock().net.get_neweid()); 
            bridge.i.lock().net.add_endpoint(ep.clone());

            if bridge.i.lock().terminate {
                return;
            }

            // Spawn RX and TX
            let _ep = ep.clone();
            let _stream = stream.clone();
            let _bridge = bridge.clone();
            let rxthread = Thread::spawn(move || { thread_rx(Which::Connector(_bridge), _ep, _stream); });
            let _ep = ep.clone();
            let _sid = bridge.i.lock().net.getserveraddr();
            let _bridge = bridge.clone();
            let txthread = Thread::spawn(move || { thread_tx(Which::Connector(_bridge), _ep, stream, _sid); });

            // Set endpoint into bridge.
            bridge.i.lock().ep = Option::Some(ep);

            // Wait for RX and TX to terminate.. then try connection
            // again until we are requested to terminate.
            rxthread.join();
            txthread.join();

            bridge.i.lock().connected = false;

            // The TX may have terminated the RX and we will make it
            // here. We need to check the terminate flag to see if 
            // we also need to exit.
            if bridge.i.lock().terminate {
                return;
            }
        }
    }

    pub fn new(net: &Net, addr: String) -> TcpBridgeConnector {
        let n = TcpBridgeConnector { i: Arc::new(Mutex::new(Internal {
            net:        net.clone(),
            terminate:  false,
            ep:         Option::None,
            addr:       addr,
            gid:        UNUSED_ID,
            connected:  false,
        }))};

        let nclone = n.clone();
        let mainthread = Thread::spawn(move || { TcpBridgeConnector::thread(nclone)});
        mainthread.detach();

        n
    }
}
