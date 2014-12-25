use std::mem::size_of;
use std::intrinsics::TypeId;

use rawmessage::RawMessage;
use syncmessage::SyncMessage;

pub fn workaround_to_static_bug() {
    panic!("sync message was not correct type");
}

/// A structure for generic represention of a message.
///
/// This structure contains the actual message data. It exposes
/// the source and destination fields, but the actual message payload
/// must be accessed using the `payload` field.
pub struct Message {
    pub srcsid:         u64,             // source server id
    pub srceid:         u64,             // source endpoint id
    pub dstsid:         u64,             // destination server id
    pub dsteid:         u64,             // destination endpoint id
    pub payload:        MessagePayload,  // actual payload
}

/// A message that can be cloned but not copied, and can be shared with other threads.
///
/// This message _can_ be cloned and _can_ be recieved by multiple endpoints, but only
/// on the local net. This message can not cross process boundaries and is sharable only
/// with threads under the same process as the sender.
pub struct CloneMessage {
    pub hash:           u64,
    pub payload:        RawMessage,
}

/// Helps `Message` become generic allowing it to represent multiple types of messages.
pub enum MessagePayload {
    Raw(RawMessage),
    Sync(SyncMessage),
    Clone(CloneMessage),
}

impl Clone for Message {
    /// Will properly clone the message and respect the actual message type. This can fail
    /// with a panic if the actual message type does not support `clone()` therefore it is
    /// your responsibility to verify or ensure the message supports `clone()`.
    fn clone(&self) -> Message {
        match self.payload {
            MessagePayload::Raw(ref msg) => {
                Message {
                    srcsid: self.srcsid, srceid: self.srceid,
                    dstsid: self.dstsid, dsteid: self.dsteid,
                    payload: MessagePayload::Raw((*msg).clone())
                }
            },
            MessagePayload::Clone(ref msg) => {
                Message {
                    srcsid: self.srcsid, srceid: self.srceid,
                    dstsid: self.dstsid, dsteid: self.dsteid,
                    payload: MessagePayload::Clone((*msg).clone())
                }                
            }
            MessagePayload::Sync(ref msg) => {
                panic!("Tried to clone a SyncMessage which is unique!");
            }
        }


    }
}

impl Message {
    /// Returns the total capacity of the message. Also known as the total size of the 
    /// message buffer which will contain raw byte data.
    pub fn cap(&self) -> uint {
        match self.payload {
            MessagePayload::Raw(ref msg) => msg.cap(),
            MessagePayload::Sync(ref msg) => msg.payload.cap(),
            MessagePayload::Clone(ref msg) => msg.payload.cap(),
        }
    }

    /// Duplicates the message making a new buffer and returning that message. _At the
    /// moment only raw messages can be duplicated._
    pub fn dup(&self) -> Message {
        match self.payload {
            MessagePayload::Raw(ref msg) => {
                Message {
                    dstsid: self.dstsid, dsteid: self.dsteid,
                    srcsid: self.srcsid, srceid: self.srceid,
                    payload: MessagePayload::Raw(msg.clone())
                }
            },
            _ => {
                panic!("message type can not be duplicated!");
            }
        }
    }

    pub fn get_raw(self) -> RawMessage {
        match self.payload {
            MessagePayload::Raw(msg) => {
                msg
            },
            _ => {
                panic!("message was not type raw! [consider checking type]")
            }
        }
    }

    pub fn get_rawmutref(&mut self) -> &mut RawMessage {
        match self.payload {
            MessagePayload::Raw(ref mut msg) => {
                msg
            },
            _ => {
                panic!("message was not type raw! [consider checking type]")
            }
        }
    }

    pub fn get_clonemutref(&mut self) -> &mut CloneMessage {
        match self.payload {
            MessagePayload::Clone(ref mut msg) => {
                msg
            },
            _ => {
                panic!("message was not type clone! [consider checking type]")
            }
        }
    }

    pub fn get_syncmutref(&mut self) -> &mut SyncMessage {
        match self.payload {
            MessagePayload::Sync(ref mut msg) => {
                msg
            },
            _ => {
                panic!("message was not type sync! [consider checking type]")
            }
        }
    }

    pub fn get_rawref(&self) -> &RawMessage {
        match self.payload {
            MessagePayload::Raw(ref msg) => {
                msg
            },
            _ => {
                panic!("message was not type raw! [consider checking type]")
            }
        }
    }

    pub fn get_cloneref(&self) -> &CloneMessage {
        match self.payload {
            MessagePayload::Clone(ref msg) => {
                msg
            },
            _ => {
                panic!("message was not type clone! [consider checking type]")
            }
        }
    }

    pub fn get_syncref(&self) -> &SyncMessage {
        match self.payload {
            MessagePayload::Sync(ref msg) => {
                msg
            },
            _ => {
                panic!("message was not type sync! [consider checking type]")
            }
        }
    }

    pub fn get_clone(self) -> CloneMessage {
        match self.payload {
            MessagePayload::Clone(msg) => {
                msg
            },
            _ => {
                panic!("message was not type clone! [consider checking type]")
            }
        }
    }

    pub fn get_sync(self) -> SyncMessage {
        match self.payload {
            MessagePayload::Sync(msg) => {
                msg
            },
            _ => {
                panic!("message was not type sync! [consider checking type]")
            }
        }
    }

    pub fn is_clone(&self) -> bool {
        match self.payload {
            MessagePayload::Sync(_) => false,
            MessagePayload::Raw(_) => false,
            MessagePayload::Clone(_) => true,
        }
    }

    pub fn is_raw(&self) -> bool {
        match self.payload {
            MessagePayload::Sync(_) => false,
            MessagePayload::Raw(_) => true,
            MessagePayload::Clone(_) => false,
        }
    }

    pub fn is_sync(&self) -> bool {
        match self.payload {
            MessagePayload::Sync(_) => true,
            MessagePayload::Raw(_) => false,
            MessagePayload::Clone(_) => false,
        }
    }

    pub fn new_fromraw(rmsg: RawMessage) -> Message {
        Message {
            srcsid: 0, srceid: 0,
            dstsid: 0, dsteid: 0,
            payload: MessagePayload::Raw(rmsg),
        }
    }

    pub fn new_raw(cap: uint) -> Message {
        Message {
            srcsid: 0, srceid: 0,
            dstsid: 0, dsteid: 0,
            payload: MessagePayload::Raw(RawMessage::new(cap)),
        }
    }

    pub fn new_clone<T: Send + Clone + 'static>(t: T) -> Message {
        // Create a message payload of type Sync.
        let payload = MessagePayload::Clone(CloneMessage::new(t));

        Message {
            srcsid: 0, srceid: 0, dstsid: 0, dsteid: 0,
            payload: payload,
        }
    }

    pub fn new_sync<T: Send + 'static>(t: T) -> Message {
        // Create a message payload of type Sync.
        let payload = MessagePayload::Sync(SyncMessage::new(t));

        Message {
            srcsid: 0, srceid: 0, dstsid: 0, dsteid: 0,
            payload: payload,
        }
    }

    pub fn is_type<T: Send + 'static>(&self) -> bool {
        if self.is_sync() && self.get_syncref().is_type::<T>() {
            return true;
        }

        self.is_clone();

        panic!("BUG BUG BUG");
        //if self.is_clone() && self.get_clone().is_type::<T>() {
        //    return true;
        //}

        false
    }
}

impl Clone for CloneMessage {
    fn clone(&self) -> CloneMessage {
        CloneMessage {
            hash:       self.hash,
            payload:    self.payload.clone(),
        }
    }
}

impl CloneMessage {
    pub fn new<T: Send + Clone + 'static>(t: T) -> CloneMessage {
        let tyid = TypeId::of::<T>();
        let hash = tyid.hash();

        // Write the structure into a raw message, and
        // consume it in the process making it unsable.
        let mut rmsg = RawMessage::new(size_of::<T>());
        rmsg.writestruct(0, t);

        CloneMessage {
            hash:       hash,
            payload:    rmsg
        }        
    }

    pub fn is_type<T: Send + 'static>(&self) -> bool {
        let tyid = TypeId::of::<T>();
        let hash = tyid.hash();

        if hash != self.hash {
            return false;
        }

        return true;
    }

    pub fn get_payload<T: Send + Clone + 'static>(self) -> T {
        let rawmsg = self.payload;

        let tyid = TypeId::of::<T>();
        let hash = tyid.hash();

        if hash != self.hash {
            panic!("clone message was not correct type");
        }

        let t: T = unsafe { rawmsg.readstructunsafe(0) };
        t
    }

}