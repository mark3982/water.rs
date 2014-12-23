Using
==

You can use this library three ways. You can use cargo, build manually, or embed into your own code. I recommend
using Cargo however if you do not use Cargo then you can build it manually, and lastly embedd in your source.

If you are new to Cargo, read http://doc.crates.io/guide.html. The easiest way to use the latest version of
the `water` lib with Cargo is to add the following line in your `Cargo.toml`:
    
    [dependencies.water]

This will cause Cargo to download and setup the water lib which can then be used with `extern crate water`. To
see an example program check out the section _Asynchronous Send And Synchronous Receive Example_ which you can
find further down.

An example `Cargo.toml` is:
    
    [package]
    name = "hello"
    version = "0.0.1"
    authors = ["My Name"]
    [dependencies.water]

Your current directory containing `Cargo.toml` should then contain `./src/main.rs` which you can place the example
code into and then type `cargo build` to produce the program. Also, refer to the Cargo guide! Do not forget to check
out the sample program a little further down as it shows a basic working example of using the library's most basic
features.

To build manually (without Cargo) you can just clone this repository and build with `rustc --crate-type rlib ./src/lib.rs -o libwater.rlib`. Then with that library in your current directory you can build your program with `rustc mymain.rs -L .`. If the library is in another directory change `-L <path>` to reflect this. 

_I recommend using Cargo as it makes managing and building dependancies very easy!_

For more examples check out the `tests` directory in the source tree! Each test will demonstrate different parts of the
library and how to use it. I aim to have everything working and compiling on the master branch. If you want bleeding edge you can select the `dev` branch or any other to find less tester but possibly newer features.

Overview
==

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


Examples
==
You can find decent examples in the `/tests/` directory at the source tree root. 

The most basic example is `basic.rs` which has multiple threads send messages to themselves and the main thread. After so many messages are sent they send a termination message which main counts. Once main has recieved enough termination messages it will also terminate which ends the test. This example uses raw messages.

A good example of sync messages can be found in `safesync.rs`. It was named so because of the, hopefully, memory safe design of sending these messages. The intentions of sync messages are to be a replacement for channels which can be found in the standard library.


Serialization Using Sync Messages
===

The sync type message support is inteded to help replace the usage of channels from the standard Rust library. The sync messaes only support the sending of `Sync` types and they can only be recieved by a single endpoint. You can however still broadcast a sync message to all endpoints on your local net but only one of them will actually get the message. At the time I see no straight forward or easy way to send `Sync` types across process boundaries which would include remote machines.

_If you really need to send a structure across process boundaries or a structure that is not `Sync` then you will have to rely on the raw message which is covered below._

Serialization Using Raw Messages
===

I am looking at including serialization support similar to JSON and HEX. However, at the moment you will have
to rely on another library for serialization and write the output to a raw message structure. You can also
easily and efficiently serialize structures with no pointers using `writestruct` and `readstruct`. 

It is possible to serialize, with this library, structures with pointers using the `writestruct` and then reading them back with `readstructunsafe`  or ``readstructunsaferef` however firstly you just broke the memory safety of Rust (all bets are off). If you are lucky your message arrived on a thread in the same process as the
thread that sent it and in this case if you used the pointer it _might_ work. A skilled programmer could make
it work if he knew the exact circumstances however this is highly dangerous. If your message arrived into another separate process (on the same machine or remotely) your going to end up reading bad values, corrupting memory, or crashing your program. The point is serializing pointers directly is a very bad idea and the only
one case were it would even be useful if it you did _not_ use the pointers or you only used them in the same
process. However, I would recommend just not serializing pointers or at the least not using them. You have to be careful because some types like `Arc`, `Box`, `Rc`, and many others have pointers internally that are not visible to you the programmer. This makes these types unsuitable for serialization. You should try to use only the `writestruct` and `readstruct` and _only_ tag stuctures that you know for sure have no pointers.

I also have included functions for reading and writing varying sized integer types such as `readu32` or `writeu32`. At the moment I support reading and writing signed and unsigned integers of sizes 32, 16, and 8 bits. This gives a safe way to build messages.

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