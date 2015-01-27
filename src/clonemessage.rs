use RawMessage;

use std::mem::size_of;
use std::any::TypeId;
use std::hash::Hash;

/// A message that can be cloned but not copied, and can be shared with other threads.
///
/// This message _can_ be cloned and _can_ be recieved by multiple endpoints, but only
/// on the local net. This message can not cross process boundaries and is sharable only
/// with threads under the same process as the sender.
pub struct CloneMessage {
    /// The hash represents the type.
    pub tyid:           TypeId,
    /// The payload contains the raw type bytes.
    pub payload:        RawMessage,
}

/// Provides ability to be cloned and satisfies clone contraints.
impl Clone for CloneMessage {
    /// Will produce a clone but will keep the same payload instance.
    fn clone(&self) -> CloneMessage {
        CloneMessage {
            tyid:       self.tyid,
            payload:    self.payload.clone(),
        }
    }
}


impl CloneMessage {
    /// Create a new clone message from a specific type.
    pub fn new<T: Send + Clone + 'static>(t: T) -> CloneMessage {
        let tyid = TypeId::of::<T>();

        // Write the structure into a raw message, and
        // consume it in the process making it unsable.
        let mut rmsg = RawMessage::new(size_of::<T>());
        rmsg.writestruct(0, t);

        CloneMessage {
            tyid:       tyid,
            payload:    rmsg
        }        
    }

    /// Check if the clone message contains the type specified. `is_type::<MyType>()`
    pub fn is_type<T: Send + 'static>(&self) -> bool {
        let tyid = TypeId::of::<T>();

        if tyid != self.tyid {
            return false;
        }

        return true;
    }

    /// Returns an instance of the type by consuming the clone message. The
    /// clone message is consumed to prevent another copy from being produced.
    /// To produce a copy you should call `clone` on the type returned.
    ///
    /// _There is no contraint on `T` to have the traits `Send` and `Clone` because
    /// the hash is checked to ensure it is the same type that was sent._
    pub fn get_payload<T: 'static>(self) -> T {
        let rawmsg = self.payload;

        let tyid = TypeId::of::<T>();

        if tyid != self.tyid {
            panic!("clone message was not correct type");
        }

        let t: T = unsafe { rawmsg.readstructunsafe(0) };
        t
    }

}