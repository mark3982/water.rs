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

Routing (Communicate With Indirectly Connected Nets)
===

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