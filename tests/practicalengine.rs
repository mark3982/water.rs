#![allow(unstable)]

extern crate time;
extern crate water;

use water::Net;
use water::Endpoint;
use water::Message;
use water::ID;
use water::Duration;
use time::Timespec;

use std::thread::JoinGuard;
use std::thread::Thread;
use std::vec::Vec;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

/*
    This tests and demonstrates:
        * segmented networks
        * selecting on multiple endpoints
        * splitting work onto multiple
            * dynamically
            * using sync messages

    We accept a `Request` packet which is accepted by one manager. We can have one or more
    managers and do not need to implement logic to select between them. The first avalible
    one will receive the message and only one. It will then produce work depending on the
    number of threads. If a thread becomes stuck it will cause the request/job to become
    stuck but some redundancy is provided using sync messages in that the other threads could
    continue to function. As each thread/worker finishes the result is sent back to the
    manager who has been tracking the jobs. When all results arrived a final packet is completed
    and this is sent to the requestor.

    The important parts are the potential scalability and the simplicity of implementation. We
    did not have to implement logic to handle multiple channels, ensure only one worker or manager
    received a packet, or serialize any of our data.
*/

struct Request {
    id:         u64,
    data:       Vec<u8>,
}

struct Work {
    id:         u64,
    data:       Arc<Vec<u8>>,
    offset:     usize,
    length:     usize,
    tail:       u64,
}

struct WorkReply {
    id:         u64,
    data:       Vec<u8>,
    offset:     usize,
    length:     usize,
}

struct WorkPending {
    id:         u64,
    epid:       ID,
    epsid:      ID,
    need:       usize,
    totalsize:  usize,
    work:       Vec<WorkReply>,
}

struct Reply {
    id:         u64,
    data:       Vec<u8>,
}

struct Terminate;
impl Clone for Terminate { fn clone(&self) -> Terminate { Terminate } }

const WORKERGRPID: ID = 0x3000;
const MANAGEREID: ID = 0x1000;

/// Accepts work packets. Processes the packet. Sends back reply.
fn thread_pipelineworker(ep: Endpoint) {
    loop {
        println!("[worker] waiting for messages");
        let result = ep.recvorblockforever();

        if result.is_err() {
            return;
        }

        let msg = result.unwrap();

        if msg.is_type::<Terminate>() {
            return;
        }

        // Figure out the type of message.
        if msg.is_type::<Work>() {
            let work: Work = msg.get_sync().get_payload();
            println!("[worker] working on id {:x} and tail {:x} with data of length {:x}/{:x}", work.id, work.tail, work.length, work.data.len());
            // Do the work.
            let mut out: Vec<u8> = Vec::with_capacity(work.length);
            for i in range(0us, work.length) {
                out.push(((work.data[i + work.offset] * 2) >> 1 + 5) * 2);
            }
            let mut wreply = Message::new_sync(WorkReply {
                id:     work.id,
                data:   out,
                offset: work.offset,
                length: work.length,                
            });
            wreply.dsteid = MANAGEREID;
            wreply.dstsid = 1;
            // The `srceid` and `srcsid` will be filled in automatically.
            ep.send(wreply);
        }
    }
}

/// Accepts requests. Splits the data. Sends it out to processors. It also
/// forwards the data for the second processing pass to the processors.
///
fn thread_pipelinemanager(mep: Endpoint, corecnt: usize) {
    let mut endpoints: Vec<Endpoint> = Vec::new();
    let mut workerthreads: Vec<JoinGuard<()>> = Vec::new();
    let mut pending: HashMap<u64, WorkPending> = HashMap::new();

    // Create net for this pipeline.
    let net = Net::new(400);
    let ep = net.new_endpoint_withid(MANAGEREID);

    endpoints.push(mep.clone());
    endpoints.push(ep.clone());

    println!("[man] creating worker threads");
    // Spawn worker threads for the number of specified cores.
    for _ in range(0us, corecnt) {
        let wep = net.new_endpoint();
        wep.setgid(WORKERGRPID);
        workerthreads.push(Thread::scoped(move || thread_pipelineworker(wep)));
    }

    // Create processor threads for this pipeline.
    loop {
        // Check for requests and results from data processors.
        println!("[man] waiting for messages");
        let result = water::recvorblock(&endpoints, Duration::seconds(10));

        if result.is_err() {
            continue;
        }

        let msg = result.unwrap();

        if msg.is_type::<Terminate>() {
            // Tell all workers to terminate.
            ep.sendclonetype(Terminate);
            return;
        }

        // Figure out the type of message.
        if msg.is_type::<Request>() {
            println!("[man] got request");
            let msg_srcsid = msg.srcsid;
            let msg_srceid = msg.srceid;
            let request: Request = msg.get_sync().get_payload();
            // Break into DataChunk(s) and hand out to the processor threads.
            let data = request.data.clone();
            let chklen = data.len() / corecnt;
            let slack = data.len() % corecnt;
            let datalen = data.len();
            let adata = Arc::new(data);

            pending.insert(request.id, WorkPending {
                epsid:      msg_srcsid,
                epid:       msg_srceid,
                id:         request.id,
                need:       corecnt,
                work:       Vec::new(),
                totalsize:  datalen,
            });

            for chkndx in range(0us, (corecnt - 1) as usize) {
                let work = Work {
                    id:     request.id,
                    data:   adata.clone(),
                    offset: chkndx * chklen,
                    length: chklen,
                    tail:   0x1234,
                };
                println!("[man] sent work packet of length {:x}", work.data.len());
                // Only the first worker to receive this gets it. This is
                // because we are sending it as sync instead of clone.
                ep.sendsynctype(work);
            }

            // Throw slack onto last chunk.
            let work = Work {
                id:     request.id,
                data:   adata,
                offset: (corecnt - 1) * chklen,
                length: chklen + slack,
                tail:   0x1234,
            };
            println!("[man] sent slack work packet {:x}", work.data.len());
            // Only the first worker to receive this gets it. This is
            // because we are sending it as sync instead of clone.
            ep.sendsynctype(work);
            continue;
        }

        if msg.is_type::<WorkReply>() {
            // Combine all results.
            let reply: WorkReply = msg.get_sync().get_payload();
            println!("[man] got work reply with id:{:x}", reply.id);
            let result = pending.get_mut(&reply.id);
            if result.is_some() {
                println!("[man] looking at work reply");
                let pending = result.unwrap();
                // Place into pending.
                pending.work.push(reply);
                // Are we done?
                if pending.work.len() >= pending.need {
                    println!("[man] processing all work replies for job");
                    // Combine output and send to original requestor.
                    let mut f: Vec<u8> = Vec::with_capacity(pending.totalsize);
                    // Only *safe* way to initialize at the moment.
                    for i in range(0us, pending.totalsize) {
                        f.push(0u8);
                    }
                    for reply in pending.work.iter() {
                        let mut i = 0us;
                        for value in reply.data.iter() {
                            f[reply.offset + i] = *value;
                            i += 1;
                        }
                    }
                    //
                    let reply = Reply {
                        id:         pending.id,
                        data:       f,
                    };
                    println!("[man] sending work finished message");
                    let mut msg = Message::new_sync(reply);
                    msg.dsteid = pending.epid;
                    msg.dstsid = pending.epsid;
                    mep.send(msg);
                }
            }
        }
    }
}

/// Perform a benchmark/test run.
fn go(reqcnt: usize, mancnt: usize, coreperman: usize, datcnt: usize) {
    let net = Net::new(100);
    let mut threads: Vec<JoinGuard<()>> = Vec::new();

    for manid in range(0us, mancnt) {
        let manep = net.new_endpoint_withid((1000 + manid) as ID);
        let t = Thread::scoped(move || thread_pipelinemanager(manep, coreperman));
        threads.push(t);
    }

    while net.getepcount() < mancnt {
        Thread::yield_now();
    }

    let ep = net.new_endpoint();

    for _ in range(0us, reqcnt) {
        let mut data: Vec<u8> = Vec::new();
        for _ in range(0us, datcnt) {
            data.push(1u8);
        }
        
        ep.sendsynctype(Request {
            id:     0x100u64,
            data:   data,
        });
    }

    for _ in range(0us, reqcnt) {
        let msg = ep.recvorblockforever().unwrap();
        println!("[main] got result");
    }

    println!("[main] telling manager to terminate");
    ep.sendclonetype(Terminate);

}

#[test]
fn practicalengine() {
    // Create a thread so we can debug using stdout under `cargo test`.
    std::thread::Thread::scoped(move || {
        go(10, 2, 2, 1024);
    });
}