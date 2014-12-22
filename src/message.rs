use std::mem::size_of;
use std::intrinsics::TypeId;

use rawmessage::RawMessage;

pub struct Message {
    pub srcsid:         u64,             // source server id
    pub srceid:         u64,             // source endpoint id
    pub dstsid:         u64,             // destination server id
    pub dsteid:         u64,             // destination endpoint id
    pub payload:        MessagePayload,  // actual payload
}

pub struct SyncMessage {
    pub hash:           u64,
    pub payload:        RawMessage,
}

pub enum MessagePayload {
    Raw(RawMessage),
    Sync(SyncMessage),
}

impl Clone for Message {
    fn clone(&self) -> Message {
        match self.payload {
            MessagePayload::Raw(ref msg) => {
                Message {
                    srcsid: self.srcsid, srceid: self.srceid,
                    dstsid: self.dstsid, dsteid: self.dsteid,
                    payload: MessagePayload::Raw(msg.clone())
                }
            },
            MessagePayload::Sync(ref msg) => {
                panic!("Tried to clone a SyncMessage which is unique!");
            }
        }


    }
}

impl Message {
    pub fn cap(&self) -> uint {
        match self.payload {
            MessagePayload::Raw(ref msg) => {
                msg.cap()
            },
            MessagePayload::Sync(ref msg) => {
                msg.payload.cap()
            }
        }
    }

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

    pub fn get_rawref(&mut self) -> &mut RawMessage {
        match self.payload {
            MessagePayload::Raw(ref mut msg) => {
                msg
            },
            _ => {
                panic!("message was not type raw! [consider checking type]")
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

    pub fn new_raw(cap: uint) -> Message {
        Message {
            srcsid: 0, srceid: 0,
            dstsid: 0, dsteid: 0,
            payload: MessagePayload::Raw(RawMessage::new(cap))
        }
    }

    pub fn new_sync<T: Sync + 'static>(t: T) -> Message {
        // Create a message payload of type Sync.
        let payload = MessagePayload::Sync(SyncMessage::new(t));

        Message {
            srcsid: 0, srceid: 0, dstsid: 0, dsteid: 0,
            payload: payload,
        }
    }
}

impl SyncMessage {
    pub fn new<T: Sync + 'static>(t: T) -> SyncMessage {
        let tyid = TypeId::of::<T>();
        let hash = tyid.hash();

        // Write the structure into a raw message, and
        // consume it in the process making it unsable.
        let mut rmsg = RawMessage::new(size_of::<T>());
        rmsg.writestruct(0, t);

        SyncMessage {
            hash:       hash,
            payload:    rmsg
        }
    }

    pub fn get_payload<T: Sync + 'static>(self) -> T {
        let rawmsg = self.payload;

        let tyid = TypeId::of::<T>();
        let hash = tyid.hash();

        if hash != self.hash {
            //panic!("sync message was not correct type");
            loop { }
        }

        let t: T = unsafe { rawmsg.readstructunsafe(0) };
        t
    }
}
