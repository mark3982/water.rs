Overview
==

Water is a library that provides a network like communication structure. It is intended to replace channels. It allows you to create just a local net for communication and join your local net if you desire to another remote net either on the same machine or a remote machine across a network. It can also be used just to communicate between two threads! 

The endpoint is a reciever and sender of messages and resides on a specific net.
Each net has an ID and each endpoint has an ID. This means you can address:

 * specific endpoint on specific network
 * specific endpoint on all networks
 * any endpoint on a specific network
 * any endpoint on any network

Some uses:

 * two processes to communicate on the same machine
 * two processes to communicate on different machines (over network)
 * intra-process communication between threads (channels)
 * distributed system software
 * replacement of channels

Some advantages over channels:

 * simpler and easier to manage a single endpoint than multiple channels
 * special sync messages avalible to all recievers but only one can reciever (difficult to implement in channels)
 * has no compiler dependancies (pure rust)
 * builtin network communication support using bridges
 * injection of messages back into endpoint (you can send a message to be recieved by the same endpoint)
 * can wait on multiple endpoints/channels

Some disadvantages:

 * takes just a bit more code than channels due to the features provided but not much
 * uses a thread per net to provide async I/O for endpoints at this time
 * uses two threads per TCP bridge connector and three threadsper TCP bridge acceptor
 * more complicated message sending due to need to specify destination (although for simple cases it can be ignored)

Some planned features:
 * priority of messages (higher priority serviced before lower) (also fail back for full endpoint queues)
 * _see github issues for other things relavent to development_

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

For more examples check out the `tests` directory in the source tree! Each test will demonstrate different parts of the library and how to use it. I aim to have everything working and compiling on the master branch. If you want bleeding edge you can select the `dev` branch or any other to find less tester but possibly newer features.

Difference Of Channels
===

A Rust channel produces two devices. One transmits messages and the other recieves messages. To produce a bi-directional channel you need to do some work. Water provides bi-directional communication with two endpoints. Also, water allows one endpoint to transmit to one or more endpoints. You can however use Water to produce Rust channel like behavior by creating a single net for each two endpoints, and only using one or the other for transmitting and one for receiving.

A channel is typed to a specific message type although you can use enums to send multiple types of messages. The problem with the enum is that it is always the size of it's largest type therefore it becomes inefficient in memory/network bandwidth to send larger messages along side of small messages with a channel depending on how large the difference is in size. Water can relieve this problem as all messages are dynamically sized to their contents. Water also makes the transition onto the network or into another process easier to implement because you do not need to know the content of the message to optimize it (by reducing the size as you would have to do with an enum to reduce the size). So sending a one megabyte message following a signal message using Rust channels will consume one megabyte of memory bandwidth for both messages wasting a lot of memory bandwidth. For example:

    enum Foo {
        Apple,
        Grape([u8, ..4096]),
    }

    fn main() {
        let (atx, arx) = channel::<Foo>();
        
        atx.send(Foo::Apple);
        atx.send(Foo::Grape([0u8, ..4096]));
        
        arx.recv();
        arx.recv();
    } 

_A `size_of::<Foo>()` will return 4097 bytes, which includes the extra byte used to determine the type of the enum._

Network Bridge Types Supported
===

A bridge joins two nets to provide remote process (machine) or intra-process communication.

Currently, the library only provides support for the TCP bridge, but I plan to add others such as UDP which will sport the same cons and pros of it.

Performance 
===

I recently did a pingpong test comparing the channels that Rust uses and water. The results were encouraging. Water was just a bit slower which is expected since it performs more.

_If you have any benchmarks or information please share it with me at kmcg3413@gmail.com_

Asynchronous Versus Synchronous Calls
===

The `recv` and `send` calls are asynchronous and will not block but can fail. The failure is actually a feature. The `send` can have a partial failure where certain endpoints were not able to recieve because there message queue reached it's limit. A queue reaching its limits can be caused by the endpoint not being read enough or an overload situation in which it is impossible to read it fast enough which depends entirely on the situation.

The `recvorblock` call is synchronous and will block until success or timeout.

Limits
===

_Limits have yet to actually be enforced in the latest version!_

A limit is specified to prevent memory system exhaustion which would eventually happen and result in run away memory usage that could leave the system unstable and unusable. To prevent this all endpoints have a default limit which may or may not be ideal for your application. You are encouraged to adjust this limit to your needs with `setlimitpending` where the value represents the maximum number of pending messages. This maximum number is not related to the memory consumed by the messages. To limit based on memory used in the incoming queue for each endpoint you should use `setlimitmemory`. The memory limit is a little misleading as message data is actually shared between endpoints. So a one megabyte message does not consume two megabyte for two endpoints but rather one megabyte plus roughly thirty-two bytes for each endpoint who recieves the message.


Examples
==
You can find decent examples in the `/tests/` directory at the source tree root. 

The most basic example is `basic.rs` which has multiple threads send messages to themselves and the main thread. After so many messages are sent they send a termination message which main counts. Once main has recieved enough termination messages it will also terminate which ends the test. This example uses raw messages.

A good example of sync messages can be found in `safesync.rs`. It was named so because of the, hopefully, memory safe design of sending these messages. The intentions of sync messages are to be a replacement for channels which can be found in the standard library.

Also, if you are looking for a network bridge example you can find one in `tcpnet.rs` which demonstrates using a TCP bridge to join two networks.

Documentation
===

You can find some documentation hosted at, http://kmcg3413.net/water.rs/doc/ . This should be kept up to date
with the master branch and the https://crates.io/water repository. You should also be able to generate documentation locally by cloning this repository and then doing `rustdoc ./src/lib.rs` which will create a doc sub-directory containing the HTML documentation.

Compatibility Layer
===

I have included a compatibility layer which emulates the native Rust threads. It is still missing the ability to select on multiple channels as that is pending my implementation of waiting on multiple endpoints which I am working on. I only provide stream mode at the moment but you can easily get shared mode with a little hacking (plan to officially support it soon). The underlying infrastructure of the proxy object standing the place of `std::comm::Receiver<T>` and `std::comm::Sender<T>` are very simple and can be found under `/src/compat.rs` if you need to work on them or become curious about how it works.

Sync/Clone/Raw Messages
===

There are three different messages types. The easiest to use are the sync and clone. The sync is essentially
a specialized version of clone. The clone messages requires the type being sent to implement the `Clone` trait because it can be sent to multiple endpoints and each endpoint needs a clone of the instance of the type.

The sync message can be sent to one or more endpoints and made avaliable to them, but only one can actually receive the message. This makes it easier to a build a pool of threads that process work where only one of them will recieve the message instead of having to implement logic to decide who does the work you can simply expect that only one will get the message.

The raw message is the most basic type of message and supports network bridges. A network bridge at this time will ignore sync and clone type messages because there is no easy way to send instances of certain types safely across process boundaries. For network communication you can use a raw message. The raw message consists of a stream of bytes. There are functions provided to support writing type instances into raw messages but they are dangerous and some techniques are used to make this safer. A better alternative to sending types across process boundaries are the usage of an serialization library (either built-in to the standard Rust distribution like JSON) or something like https://crates.io/crates/bincode . By using these you can serialize a structure safely then write it to a raw message and have it unserialized at the other end. I have decide not to include support for this because it is better if you used another library that could specialize in this and do it really well.

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