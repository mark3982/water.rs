_This library is missing support for remote nets! However, I will talk about
it like it has that support so you understand what to expect from it._

Water is a library that provides a network like communication structure. It allows
you to create just a local net for communication and join your local net if you
desire to another remote net either on the same machine or a remote machine across
a network.

The endpoint is a reciever and sender of messages and resides on a specific net.
Each net has an ID and each endpoint has an ID. This means you can address:

 * specific endpoint on specific network
 * specific endpoint on all networks
 * any endpoint on a specific network
 * any endpoint on any network

Some uses:

 * two processes to communicate on the same machine
 * two processes to communicate on different machines
 * interprocess communication between threads

Some advantages:

 * simple and easy to manage a single endpoint than multiple channels since you can send and recieve to multiple endpoints (threads) from a single endpoint
 * easy to segment loads by creating a separate network

Some disadvantages:

 * more attention must be paid not to overload a network depending on your usage
 * asynchronous send (default) overload can result in excess memory usage and limits can
   cause lost messages

Asynchronous Versus Synchronous
===

The `recv` and `send` calls are asynchronous and will not block but can fail. The failure is actually
a feature. The `send` can have a partial failure where certain endpoints were not able to recieve because
there message queue reached it's limit. A queue reaching its limits can be caused by the endpoint not being
read enough or an overload situation in which it is impossible to read it fast enough which depends entirely
on the situation.

The `recvorblock` and `sendorblock` calls are synchronous and will block until success or timeout. The 
`sendorblock` call can partially fail if a timeout is reached and one or more endpoints may not have
recieved the message because of their queues reaching limits.

Limits
===

A limit is specified to prevent memory system exhaustion which would eventually happen and result in
run away memory usage that could leave the system unstable and unusable. To prevent this all endpoints
have a default limit which may or may not be ideal for your application. You are encouraged to adjust
this limit to your needs with `setlimitpending` where the value represents the maximum number of pending
messages. This maximum number is not related to the memory consumed by the messages. To limit based on
memory used in the incoming queue for each endpoint you should use `setlimitmemory`. The memory limit
is a little misleading as message data is actually shared between endpoints. So a one megabyte message
does not consume two megabyte for two endpoints but rather one megabyte plus roughly thirty-two bytes
for each endpoint who recieves the message.


Asynchronous Send And Synchronous Receive Example
==

An example of synchronous blocking. By default `recv` is asynchrnous. We use `recvorblock`:
```
    extern crate time;

    use net::Net;
    use endpoint::Endpoint;
    use rawmessage::RawMessage;

    use std::io::timer::sleep;
    use std::time::duration::Duration;
    use time::Timespec;

    fn funnyworker(mut net: Net, dbgid: uint) {
        // Create our endpoint.
        let ep: Endpoint = net.new_endpoint();
        let mut rawmsg: RawMessage;

        loop {
            sleep(Duration::seconds(1));
            // Read anything we can.
            loop { 
                println!("thread[{}] recving", dbgid);
                let result = ep.recvorblock(Timespec { sec: 1, nsec: 0 });
                match result {
                    Ok(msg) => {
                        println!("thread[{}] got message", dbgid);
                    },
                    Err(err) => {
                        println!("thread[{}] no more messages", dbgid);
                        break;
                    }
                }
            }
            // Send something random.
            println!("thread[{}] sending random message", dbgid);
            {
                rawmsg = RawMessage::new(32);
                rawmsg.dstsid = 0;
                rawmsg.dsteid = 0;
                ep.send(&rawmsg);
            }
        }
    }

    fn main() {
        // Create net with ID 234.
        let mut net: Net = Net::new(234);

        // Spawn threads.
        let netclone = net.clone();
        spawn(move || { funnyworker(netclone, 0); });
        let netclone = net.clone();
        spawn(move || { funnyworker(netclone, 1); });
        let netclone = net.clone();
        spawn(move || { funnyworker(netclone, 2); });

        println!("main thread done");
        loop { }
    }
```

Routing 
===

_At the moment you can only communicate with a directly connected net.._

Hopefully, in the future I can implement routing which will allow nets to be connected
into a graph and messages can be routed between the nets by special endpoints. At the
moment to address a net you _must_ be connected to that net. With routing you could have
the nets look kind of like:
```
            netA <----> netB <-----> netC
                         ^
                         |
                         |
                         |
                        netD <------> netE
```
So routing would allow `netA` to address `netD`, `netC`, and `netE`. This would be done
by special endpoints that would detect traffic destined for `netE` on `netB` and forward
this to `netD` would would then forward to `netE` making the circuit complete.

_With out routing `netA` would have to directly be connected to `netE` or any other net
that it wishes to address._

Threads And Memory
===

Currently, for `recvorblock` support one thread is spawned per net. Also, per TCP link
two threads are spawned where one handles sending and the other recieving. This adds
overhead and I may at some point in time reduce the number of TCP link threads to one
by making the sending thread try to actually perform the send. Also memory consumption
is not a large concern even though each endpoint stores messages because the actual
message data is shared by using `clone` on each message. When the message is pulled out
of the endpoint the message is `dup`ed.