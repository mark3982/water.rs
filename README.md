Overview
==

Water is a library that provides a network like communication structure. It is intended to replace channels. It allows you to create just a local net for communication and join your local net if you desire to another remote net either on the same machine or a remote machine across a network. It can also be used just to communicate between two threads! 

_This project was born because of the need to handle multiple channels for complex interconnection between threads. I found that it would be much simplier if I could use a single instance of an object to communicate with multiple other threads, ability to present messages to multiple recievers and have only one recieve the message, ability to send different types down the same pipe, and the ability to segment and prioritize, and wait on multiple communication pipes. By trying to do these things using the normal Rust channels it was very difficult and naturally this library evolved._

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
 * special sync messages avalible to all recievers but only one can reciever (difficult to implement with channels)
 * builtin network communication support using bridges
 * injection of messages back into endpoint (you can send a message to be recieved by the same endpoint)
 * can wait on multiple endpoints/channels
 * handles varying sized types efficiently over the same endpoint versus a channel using an enum

Some disadvantages over channels:

 * takes just a bit more code than channels due to the features provided but hardly any more
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

Documentation
===

You can find some documentation hosted at, http://kmcg3413.net/water.rs/doc/lib/ . This should be kept up to date with the master branch and the https://crates.io/water repository. You should also be able to generate documentation locally by cloning this repository and then doing `rustdoc ./src/lib.rs` which will create a doc sub-directory containing the HTML documentation.
