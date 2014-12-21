_This library is missing support for remote nets! However, I will talk about
it like it has that support so you understand what to expect from it._

Water is a library that provides a network like communication structure. It allows
you to create just a local net for communication and join your local net if you
desire to another remote net either on the same machine or a remote machine across
a network.

The endpoint is the a reciever and sender of messages and resides on a specific net.
Each net has an ID and each endpoint has an ID. This means you can address:

 * specific endpoint on specific network
 * specific endpoint on all networks
 * any endpoint on a specific network
 * any endpoint on any network

Hopefully, in the future I can implement routing which will allow nets to be connected
into a graph and messages can be routed between the nets by special endpoints.

Some uses:

 * two processes to communicate on the same machine
 * two processes to communicate on different machines
 * interprocess communication between threads

Some advantages:

 * simple and easy to manage a single endpoint than multiple channels since you can send and recieve to multiple endpoints (threads) from a single endpoint
 * easy to segment loads by creating a separate network