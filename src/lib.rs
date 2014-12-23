#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_must_use)]
#![allow(deprecated)]

extern crate time;

pub use net::Net;
pub use endpoint::Endpoint;
pub use rawmessage::RawMessage;
pub use rawmessage::NoPointers;
pub use message::MessagePayload;
pub use message::Message;
pub use endpoint::IoResult;
pub use endpoint::IoError;
pub use endpoint::IoErrorCode;

mod endpoint;
mod net;
mod rawmessage;
mod timespec;
mod tcp;
mod message;