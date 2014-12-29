use std::mem::size_of;
use std::intrinsics::TypeId;

use rawmessage::RawMessage;
use syncmessage::SyncMessage;
use clonemessage::CloneMessage;

pub fn workaround_to_static_bug() {
    panic!("sync message was not correct type");
}

/// A structure for generic represention of a message.
///
/// This structure contains the actual message data. It exposes
/// the source and destination fields, but the actual message payload
/// must be accessed using the `payload` field.
///
/// A message can, currently, be of three types. It can by a raw, sync,
/// or clone. The sync and clone internaly are actually raw messages but
/// they provide support for building the raw message containing the type
/// instance and provide additional safety than using the raw type alone.
///
/// To be perfectly correct when you have a message you should first check
/// the type. If it is a raw message you will need to extract the raw message
/// instance and work with it's byte stream.
///
/// If the type is clone or sync you should use `is_type` and `unwraptype`. The
/// `unwraptype` will panic if the expected type is not the type contained in the
/// message. This panic is the only sane way to handle this situation. I may implement
/// a Result enum later if this is desired. If you use `is_type` you can check for
/// the type of the message and prevent any panic from `unwraptype`.
/// ```
///     let result = endpoint.recvorblock( Timespec { sec: 5i64, nsec: 0i32 } );
///     if result.is_err() { try_again_or_quit; }
///     let message = result.ok();
///     if !message.is_sync() && !message.is_clone() { maybe_check_for_raw; }
///     if message.is_type::<Apple>() {
///         let apple: Apple = message.unwraptype();
///     }
///     if message.is_type::<Grape>() {
///         let grape: Grape = message.unwraptype();
///     }
/// ```
pub struct Message {
    pub srcsid:         u64,             // source server id
    pub srceid:         u64,             // source endpoint id
    pub dstsid:         u64,             // destination server id
    pub dsteid:         u64,             // destination endpoint id
    pub canloop:        bool,            // can loop back into sender?
    pub payload:        MessagePayload,  // actual payload
}

/// Helps `Message` become generic allowing it to represent multiple types of messages.
pub enum MessagePayload {
    Raw(RawMessage),
    Sync(SyncMessage),
    Clone(CloneMessage),
}

unsafe impl Send for Message {}
unsafe impl Send for CloneMessage {}

impl Clone for Message {
    /// Will properly clone the message and respect the actual message type. This can fail
    /// with a panic if the actual message type does not support `clone()` therefore it is
    /// your responsibility to verify or ensure the message supports `clone()`.
    fn clone(&self) -> Message {
        match self.payload {
            MessagePayload::Raw(ref msg) => {
                Message {
                    canloop: false,
                    srcsid: self.srcsid, srceid: self.srceid,
                    dstsid: self.dstsid, dsteid: self.dsteid,
                    payload: MessagePayload::Raw((*msg).clone())
                }
            },
            MessagePayload::Clone(ref msg) => {
                Message {
                    canloop: false,
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
                    canloop: self.canloop,
                    dstsid: self.dstsid, dsteid: self.dsteid,
                    srcsid: self.srcsid, srceid: self.srceid,
                    payload: MessagePayload::Raw(msg.dup())
                }
            },
            _ => {
                panic!("message type can not be duplicated!");
            }
        }
    }

    /// Used mostly internally. It helps duplicate only for raw messages to
    /// prevent having a buffer shared outside the system. Although having
    /// a buffer shared outside the system could be useful it is not supported
    /// at the moment. You may find implementing a type that provides this and
    /// sending it as a sync or clone type will provide that functionality.
    pub fn dup_ifok(self) -> Message {
        match self.payload {
            MessagePayload::Raw(ref msg) => {
                Message {
                    canloop: self.canloop,
                    dstsid: self.dstsid, dsteid: self.dsteid,
                    srcsid: self.srcsid, srceid: self.srceid,
                    payload: MessagePayload::Raw(msg.dup())
                }
            },
            _ => {
                self
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

    /// Gets a reference to the inner API for a raw message. This will panic
    /// if the message is not a raw type.
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

    /// For a sync or clone message type only this will extract the
    /// instance of the type contained. You must be explicit about
    /// the type contained. You can use `is_type::<T>` to check
    /// the type.
    pub fn typeunwrap<T: 'static>(self) -> T {
        match self.payload {
            MessagePayload::Clone(msg) => msg.get_payload::<T>(),
            MessagePayload::Sync(msg) => msg.get_payload::<T>(),
            _ => {
                panic!("message was not clone or sync type! [consider checking type]")
            }
        }
    }

    /// Get a reference to the clone message API for this message without
    /// consuming this message. This can be useful if you still need to
    /// keep the message around maybe for resending.
    ///
    /// To get the type consider `typeunwrap::<T>()`.
    /// To check the message type consider `is_sync`, `is_raw`, and `is_clone`.
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

    /// Get a reference to the sync message API for this message without
    /// consuming this message. This can be useful if you still need to
    /// keep the message around maybe for resending.
    ///
    /// To get the type consider `typeunwrap::<T>()`.
    /// To check the message type consider `is_sync`, `is_raw`, and `is_clone`.
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

    /// This works like `get_cloneref` except you consume the outer message
    /// API and the clone message is returned. This is useful if you have
    /// no need of the outer message layer and need to do more than just get
    /// the type out.
    ///
    /// If you only need the type consider `typeunwrap::<T>()`.
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

    /// This works like `get_syncref` except you consume the outer message
    /// API and the sync message is returned. This is useful if you have
    /// no need of the outer message layer and need to do more than just get
    /// the type out.
    ///
    /// If you only need the type consider `typeunwrap::<T>()`.
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

    /// Check if this is a clone message.
    pub fn is_clone(&self) -> bool {
        match self.payload {
            MessagePayload::Sync(_) => false,
            MessagePayload::Raw(_) => false,
            MessagePayload::Clone(_) => true,
        }
    }

    /// Check if this is a raw message.
    pub fn is_raw(&self) -> bool {
        match self.payload {
            MessagePayload::Sync(_) => false,
            MessagePayload::Raw(_) => true,
            MessagePayload::Clone(_) => false,
        }
    }

    /// Check if this is a sync message.
    pub fn is_sync(&self) -> bool {
        match self.payload {
            MessagePayload::Sync(_) => true,
            MessagePayload::Raw(_) => false,
            MessagePayload::Clone(_) => false,
        }
    }

    /// Creates a new message from a raw message.
    pub fn new_fromraw(rmsg: RawMessage) -> Message {
        Message {
            canloop: false,
            srcsid: 0, srceid: 0,
            dstsid: 0, dsteid: 0,
            payload: MessagePayload::Raw(rmsg),
        }
    }

    /// Helper function for creating a raw message with capacity specified.
    pub fn new_raw(cap: uint) -> Message {
        Message {
            canloop: false,
            srcsid: 0, srceid: 0,
            dstsid: 0, dsteid: 0,
            payload: MessagePayload::Raw(RawMessage::new(cap)),
        }
    }

    /// Helper function for creating a clone message with a type instance.
    pub fn new_clone<T: Send + Clone + 'static>(t: T) -> Message {
        // Create a message payload of type Sync.
        let payload = MessagePayload::Clone(CloneMessage::new(t));

        Message {
            canloop: false,
            srcsid: 0, srceid: 0, dstsid: 0, dsteid: 0,
            payload: payload,
        }
    }

    /// Helper function for creating a sync message with a type instance.
    pub fn new_sync<T: Send + 'static>(t: T) -> Message {
        // Create a message payload of type Sync.
        let payload = MessagePayload::Sync(SyncMessage::new(t));

        Message {
            canloop: false,
            srcsid: 0, srceid: 0, dstsid: 0, dsteid: 0,
            payload: payload,
        }
    }

    /// This will return `true` if the type is the same as expected,
    /// `false` if it is not. 
    pub fn is_type<T: Send + 'static>(&self) -> bool {
        if self.is_sync() && self.get_syncref().is_type::<T>() {
            return true;
        }

        if self.is_clone() && self.get_cloneref().is_type::<T>() {
            return true;
        }

        false
    }
}