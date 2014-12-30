#![allow(unused_mut)]

pub use tcp::listener::TcpBridgeListener;
pub use tcp::connector::TcpBridgeConnector;

use std::io::TcpStream;
use endpoint::Endpoint;
use message::Message;
use rawmessage::RawMessage;
use net::ID;
use net::UNUSED_ID;
use time::Timespec;

pub mod listener;
pub mod connector;

pub struct TerminateMessage;

impl Copy for TerminateMessage { }

impl Clone for TerminateMessage { 
    fn clone(&self) -> TerminateMessage { TerminateMessage }
}

fn getok<T, E>(result: Result<T, E>) -> T {
    match result {
        Ok(r) => r,
        Err(e) => panic!("I/O error"),
    }
}

pub enum Which<T, V> {
    Listener(T),
    Connector(V),
}

pub fn thread_rx(mut which: Which<TcpBridgeListener, TcpBridgeConnector>, mut ep: Endpoint, mut stream: TcpStream) {
    // The first message will be the remote net ID.
    let rsid: u64 = getok(stream.read_be_u64());

    match which {
        Which::Listener(ref mut bridge) => bridge.negcountinc(),
        Which::Connector(ref mut bridge) => bridge.setconnected(true),
    }

    // This is used to catch messages directed to go only onto the
    // remote net, or for broadcast messages.
    ep.setsid(rsid);

    loop {
        // Read a single message from the stream.
        //let mut msgsize: u64 = getok(stream.read_be_u64());

        let ioresult = stream.read_be_u64();
        let mut msgsize: u64;
        msgsize = match ioresult {
            Ok(r) => r,
            Err(e) => {
                // Let us shutdown this thread and hopefully our TX
                // thread will follow suit shortly.
                return;
            }
        };

        let msg_type: u8 = getok(stream.read_u8());
        let msg_srcsid: u64 = getok(stream.read_be_u64());
        let msg_srceid: u64 = getok(stream.read_be_u64());
        let msg_dstsid: u64 = getok(stream.read_be_u64());
        let msg_dsteid: u64 = getok(stream.read_be_u64());
        msgsize -= 1 + 8 * 4;

        // Read the actual raw message part of the message.
        let mut vbuf: Vec<u8> = Vec::from_elem(msgsize as uint, 0u8);
        stream.read_at_least(msgsize as uint, vbuf.as_mut_slice());

        // We currently only support raw messages at the moment, since
        // there is no way possible at this time to support sync or clone
        // type messages.
        if msg_type != 1u8 {
            // Just ignore the message. But, for debugging lets panic!
            panic!("got message type {} instead of 1", msg_type);
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
        ep.sendx(&msg);
    }
}

pub fn thread_tx(mut which: Which<TcpBridgeListener, TcpBridgeConnector>, mut ep: Endpoint, mut stream: TcpStream, sid: ID) {
    stream.write_be_u64(sid);

    loop {
        let result = ep.recvorblock(Timespec { sec: 900i64, nsec: 0i32 });

        if result.is_err() {
            continue;
        }

        let msg = result.ok();

        println!("tx_thread got message raw:{}", msg.is_raw());

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

        let rmsg = msg.get_raw();

        stream.write_be_u64((1 + 8 * 4 + rmsg.len()) as u64);
        stream.write_u8(1u8);
        stream.write_be_u64(srcsid);
        stream.write_be_u64(srceid);
        stream.write_be_u64(dstsid);
        stream.write_be_u64(dsteid);
        stream.write(rmsg.as_slice());
    }
}
