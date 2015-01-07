use Endpoint;
use std::sync::mpsc::TryRecvError;
use Timespec;
use Net;

/// This module provides a compatibility layer which, hopefully, provides
/// semantics equal to std::comm::channel. At the moment I do not have
/// support in Water for waiting on multiple endpoints, but once that is
/// complete I _may_ try to provide a select type ability.
///
/// However, to note that using this compatibility layer limits functionality
/// unless you access the `Endpoint` through `getendpoint` methods. So you lose
/// a lot of the power of Water at the gain of simplicity which may be desired.

pub struct SenderProxy<T> {
    ep:         Endpoint,
}

impl<T> Clone for SenderProxy<T> {
    fn clone(&self) -> SenderProxy<T> {
        SenderProxy {
            ep:     self.ep.clone(),
        }
    }
}

/// http://doc.rust-lang.org/nightly/std/comm/struct.Sender.html
impl<T: Send> SenderProxy<T> {
    pub fn getendpoint(&mut self) -> &mut Endpoint {
        &mut self.ep
    }

    pub fn send(&self, t: T) {
        if self.send_opt(t).is_err() {
            panic!("remote end disconnected")
        }
    }

    pub fn send_opt(&self, t: T) -> Result<(), T> {
        if self.ep.getpeercount() < 2 {
            Result::Err(t)
        } else {
            self.ep.sendsynctype(t);
            Result::Ok(())
        }
    }
}

pub struct MessagesIterator<'a, T> {
    proxy:      ReceiverProxy<T>,
}

impl<'a, T: Send> Iterator for MessagesIterator<'a, T> {
    fn next(&mut self) -> Option<T> {
        let result = self.proxy.recv_opt();
        match result {
            Result::Ok(t) => Option::Some(t),
            Result::Err(_) => Option::None,
        }
    }
}

pub struct ReceiverProxy<T> {
    ep:         Endpoint,
}

/// http://doc.rust-lang.org/nightly/std/comm/struct.Receiver.html
impl<T: Send> Clone for ReceiverProxy<T> {
    fn clone(&self) -> ReceiverProxy<T> {
        ReceiverProxy {
            ep:     self.ep.clone(),
        }
    }
}

impl<T: Send> ReceiverProxy<T> {
    pub fn getendpoint(&mut self) -> &mut Endpoint {
        &mut self.ep
    }

    pub fn recv(&self) -> T {
        loop {
            let result = self.ep.recvorblockforever();

            if !result.is_ok() {
                panic!("recv I/O error");
            }

            let msg = result.ok();

            // Just ignore raw messages.
            if msg.is_raw() {
                continue;
            }

            // Do not return anything but the specified type.
            if !msg.is_type::<T>() {
                continue;
            }

            // Since we know is it T it should not fail.
            let instance = msg.typeunwrap::<T>();

            return instance;
        }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        loop {
            let result = self.ep.recv();

            if !result.is_ok() {
                return Result::Err(TryRecvError::Empty);
            }

            let msg = result.ok();

            // Just ignore raw messages.
            if msg.is_raw() {
                continue;
            }

            // Do not return anything but the specified type.
            if !msg.is_type::<T>() {
                continue;
            }

            // Since we know is it T it should not fail.
            let instance = msg.typeunwrap::<T>();

            return Result::Ok(instance);
        }            
    }

    pub fn recv_opt(&self) -> Result<T, ()> {
        loop {
            // Wake up every so often so we can check if anyone is still
            // out there listening. I would really love to be able to flag
            // us to wake up when only one endpoint is left which would be
            // more efficient, but that is an early optimization.
            let result = self.ep.recvorblock( Timespec { sec: 3i64, nsec: 0i32 } );

            println!("checking result");

            if !result.is_ok() {
                // If no one is out there to keep the behavior consistent
                // with native channels we are just going to consider this
                // a disconnect.
                println!("peercount:{}", self.ep.getpeercount());
                if self.ep.getpeercount() < 2 {
                    return Result::Err(());
                }
                // Go back to listening for a message.
                continue;
            }

            println!("got message");

            let msg = result.ok();

            // Just ignore raw messages.
            if msg.is_raw() {
                continue;
            }

            // Do not return anything but the specified type.
            if !msg.is_type::<T>() {
                continue;
            }

            // Since we know is it T it should not fail.
            let instance = msg.typeunwrap::<T>();

            return Result::Ok(instance);
        }            
    }

    pub fn iter<'a>(&'a self) -> MessagesIterator<'a, T> {
        MessagesIterator {
            proxy:     self.clone(),
        }
    }
}

/// http://doc.rust-lang.org/nightly/std/comm/index.html
pub fn channel<T>() -> (SenderProxy<T>, ReceiverProxy<T>) {
    let mut net = Net::new(200);
    let epa = net.new_endpoint();
    let epb = net.new_endpoint();
    (
        SenderProxy {
            ep:     epa,
        },
        ReceiverProxy {
            ep:     epb,
        }
    )
}