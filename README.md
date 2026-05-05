# AeroWAN
Aerowan is a combination of two distinct networking technologies: the [reticulum network](https://reticulum.network/) stack and the [iroh](https://www.iroh.computer/) networking stack which allows for modular, peer to peer connections between devices.

To get started with the application, you must have rust installed on your system; After cloning the repository, run :
  `cargo -run -- --tui`
To connect with anoter node, press the `c` key, and enter the node ID.

To obtain the node ID at the moment, the easiest method is to query the `status` endpoint for another peer using a tool like curl over the command line.

Once connected to another peer it is possible to chat with another peer over the existing connection.
