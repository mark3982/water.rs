#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_must_use)]
#![allow(deprecated)]

// Complains of unused attribute. No idea. -- kmcg3413@gmail.com
//#![crate_id = "water"]
//#![crate_type = "lib"]

//! Provides synchronous and asynchronous messages passing intra-process, and inter-process using
//! bridges. The messages can be of raw, clone, or sync type. Envisioned as a replacement for
//! channels. The raw messages can be sent to multiple endpoints and cross remote bridges. The sync
//! type hold a type instance and can only be sent intra-process and only one endpoint can recieve
//! the message. The clone messages are intra-process only but many endpoints can recieve.
//!
//! To see example usage checkout:
//!               https://github.com/kmcguire3413/water.rs/tree/master/tests
//!      In the tests directory you will find various test not only testing the functionality but
//!      also demonstrating it. Also you can use this documentation as a reference.
//!
//! _This library is still in a developmental state (alpha) and is subject to large breaking changes._
//! Once all the design issues are worked out it will stabilize and become beta or release. Consider
//! all API to be experimental and unstable.
//!
//! Also be sure to check out the README which is visible at https://github.com/kmcguire3413/water.rs - 
//! it will detail more information about the library.

extern crate time;

pub use time::Timespec;
pub use net::Net;
pub use endpoint::Endpoint;
pub use rawmessage::RawMessage;
pub use rawmessage::NoPointers;
pub use message::MessagePayload;
pub use message::Message;
pub use endpoint::IoResult;
pub use endpoint::IoError;
pub use endpoint::IoErrorCode;
pub use syncmessage::SyncMessage;
pub use clonemessage::CloneMessage;
pub use tcp::TcpBridgeConnector;
pub use tcp::TcpBridgeListener;
//pub use allocmutex::AllocMutex;

pub mod syncmessage;
pub mod endpoint;
pub mod net;
pub mod rawmessage;
pub mod timespec;
pub mod tcp;
pub mod message;
pub mod clonemessage;
//pub mod allocmutex;