use std::mem::size_of;
use std::intrinsics::TypeId;
use std::sync::Arc;
use std::sync::Mutex;

use rawmessage::RawMessage;

/// A message that can not be cloned or copied, and can be shared with other threads.
///
/// This message can not be cloned or copied and can only be recieved
/// by a single endpoint on the local net. If you try to send it to
/// other nets it will fail, possibily with a panic. Therefore it should
/// be known that this message can not cross process boundaries.
pub struct SyncMessage {
    pub hash:           u64,
    pub valid:          Arc<Mutex<bool>>,
    pub payload:        RawMessage,
}

unsafe impl Send for SyncMessage { }

impl SyncMessage {
    /// Return the type contained in the sync message by consuming the
    /// sync message so that it can no longer be used. 
    ///
    /// _There is no contraint on `T` to be Send because the type is 
    /// checked using the hash._
    pub fn get_payload<T: 'static>(self) -> T {
        let rawmsg = self.payload;

        let tyid = TypeId::of::<T>();
        let hash = tyid.hash();

        if hash != self.hash {
            panic!("sync message was not correct type");
        }

        let t: T = unsafe { rawmsg.readstructunsafe(0) };
        t
    }

    /// Will produce a clone but will keep the same payload instance. This is 
    /// used to support sending a sync message to more than one endpoint. However
    /// the endpoints respect that only one sync message should exist so they
    /// use `takeasvalid` to ensure only one takes the message and any others
    /// discard it.
    ///
    /// I use a `key` to protect you from accidentally using this internal method
    /// in your normal code. You will have to search the source or call the method
    /// with an invalid key to get the proper value. This was caused because I was
    /// unable to make a method only visible to my crate across modules. To keep
    /// from having to use traits which would complicate the code (I believed at
    /// this time at least) I used this method.
    #[inline]
    pub fn internal_clone(&self, key: usize) -> SyncMessage {
        if key != 0x879 {
            panic!("You used an internal function. They key is 0x879.")
        }        
        SyncMessage {
            hash:       self.hash,
            valid:      self.valid.clone(),
            payload:    self.payload.clone(),
        }
    }

    /// _(internal)_ This attempts to be the first to take the message
    /// and will return a boolean if successful. This supports the ability
    /// to send a sync message to more than one endpoint and the first one
    /// to take it can return it. Any others that fail to take it as valid
    /// will simply drop the message and try to read the next.
    pub fn takeasvalid(&self) -> bool {
        let mut lock = self.valid.lock().unwrap();
        if *lock {
            // Prevent anyone else from taking this message.
            *lock = false;
            // Let the caller know we succeeded.
            drop(lock);
            true
        } else {
            drop(lock);
            // Let the caller know we failed.
            false
        }
    }

    /// Check if the type is contained. `is_type::<MyType>()`
    pub fn is_type<T: Send + 'static>(&self) -> bool {
        let tyid = TypeId::of::<T>();
        let hash = tyid.hash();

        if hash != self.hash {
            return false;
        }

        return true;
    }

    /// Create a new sync message by consuming the type passed making
    /// that type unique where it can not be cloned or duplicated.
    pub fn new<T: Send + 'static>(t: T) -> SyncMessage {
        let tyid = TypeId::of::<T>();
        let hash = tyid.hash();

        // Write the structure into a raw message, and
        // consume it in the process making it unsable.
        let mut rmsg = RawMessage::new(size_of::<T>());
        rmsg.writestruct(0, t);

        SyncMessage {
            hash:       hash,
            valid:      Arc::new(Mutex::new(true)),
            payload:    rmsg
        }
    }
}
