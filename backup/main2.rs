use std::fmt;
use anyhow::Result;
use iroh::Endpoint;
use reticulum::identity::PrivateIdentity;
use reticulum::destination::{
    Destination, DestinationName, Input, Single, SingleInputDestination,
};
use reticulum::transport::{Transport, TransportConfig};
use reticulum::iface::InterfaceManager;
use rand_core::OsRng;

// AeroWAN: a self-configuring node wrapping Iroh (peer-to-peer QUIC) + Reticulum
// (mesh networking stack). Together they provide: encrypted peer discovery (Iroh)
// + cryptographic mesh routing to the global Ret network (Reticulum).
#[derive(Debug)]
struct AeroWAN {
    iroh_endpoint: Endpoint,
    reticulum_node: ReticulumNode,
    protocols: Vec<Protocol>,
    interfaces: Vec<NetworkIF>,
}

impl AeroWAN {
    /// Single entry point. Bootstraps all subsystems in order:
    ///  1. Iroh QUIC endpoint (peer-to-peer transport layer)
    ///  2. Reticulum node (identity + destination + transport)
    ///  3. Physical interface detection
    pub async fn new() -> Result<Self> {
        let iroh_endpoint = bootstrap_iroh().await?;
        let reticulum_node = bootstrap_reticulum()?;
        let interfaces = detect_interfaces()?;

        Ok(Self {
            iroh_endpoint,
            reticulum_node,
            protocols: vec![Protocol::Iroh, Protocol::Reticulum],
            interfaces,
        })
    }
}

// ---------------------------------------------------------------------------
// Iroh bootstrap
// ---------------------------------------------------------------------------

async fn bootstrap_iroh() -> Result<Endpoint> {
    let endpoint = Endpoint::builder()
        .bind()
        .await?;
    println!("Iroh NodeID: {}", endpoint.id());
    println!("Iroh Address:{:?}", endpoint.addr());
    Ok(endpoint)
}

// ---------------------------------------------------------------------------
// Reticulum bootstrap
// ---------------------------------------------------------------------------
//
// The process mirrors what the Python reference implementation does on first
// start, but using the Reticulum-rs types from the rustdoc:
//
//   1. Generate a fresh PrivateIdentity (X25519 encryption key + Ed25519
//      signing key — a 512-bit EC keyset in total).
//
//   2. Create a SingleInputDestination from that identity. The destination
//      name uses a dotted aspect notation: "aerowan.node". Reticulum will
//      automatically append the public key and hash the result to produce
//      the 128-bit destination hash that identifies this node on the mesh.
//
//   3. Build a Transport with a default TransportConfig. The transport owns
//      the path table, handles announce forwarding, and manages all active
//      interfaces.
//
//   4. Attach an InterfaceManager. On a real deployment you would call
//      iface_manager.add(TcpClient::new(...)) or add(UdpInterface::new(...))
//      here to connect to the backbone. The manager is kept in the node so
//      interfaces can be added later (e.g. once detect_interfaces() has run).
//
//   5. Print the destination hash — this is what peers need to reach you,
//      the equivalent of your "address" on the Reticulum network.

const APP_NAME: &str = "aerowan";

fn bootstrap_reticulum() -> Result<ReticulumNode> {
    // Step 1 — Identity
    // PrivateIdentity holds the private halves of both keypairs and is the
    // only type that implements DecryptIdentity. Keep it secret.
    let identity = PrivateIdentity::new_from_rand(OsRng);

    // Step 2 — Destination
    // Destination<Input, Single> is the type alias SingleInputDestination.
    // Direction = Input  → this node receives packets/links addressed to it.
    // Type      = Single → encrypted, uniquely addressed by identity hash.
    // DestinationName encodes the dotted aspect name used in the hash.
    let dest_name = DestinationName::new(APP_NAME, ["node"]);
    let destination: SingleInputDestination =
        Destination::<PrivateIdentity, Input, Single>::new(identity, dest_name)?;

    // Print the 16-byte destination hash as 32 hex chars.
    // This is your Reticulum "address" — share it with peers out-of-band.
    println!(
        "Reticulum destination hash: {}",
        hex::encode(destination.hash())
    );

    // Step 3 — Transport
    // TransportConfig::default() sets sane defaults (PATHFINDER_M hop limit,
    // announce propagation rules, rate limiting, etc.)
    let _transport = Transport::new(TransportConfig::default());

    // Step 4 — Interface manager
    // Kept here so interfaces discovered later can be registered without
    // restarting the transport. On a production node you would attach:
    //   - AutoInterface equivalent (UDP multicast on the local WiFi segment)
    //   - TCPClientInterface to a backbone entrypoint for global reachability
    let _iface_manager = InterfaceManager::new();

    // TODO: attach interfaces from detect_interfaces() output once that
    // function returns real adapters. Example:
    //   let iface = UdpInterface::new(local_port, upstream).await?;
    //   iface_manager.add(iface);
    //   let iface = TcpClient::new("dublin.connect.reticulum.network:4965").await?;
    //   iface_manager.add(iface);

    Ok(ReticulumNode {
        identity,
        destination,
    })
}

// ---------------------------------------------------------------------------
// Interface detection
// ---------------------------------------------------------------------------
//
// Uses the `network-interface` crate to enumerate OS-level network adapters
// and map them into the AeroWAN NetworkIF type. Each adapter maps to one of
// the Interface variants based on its name heuristic. On a real deployment
// you would pass these back to bootstrap_reticulum() so it can attach the
// right Reticulum interface type for each adapter (AutoInterface for WiFi/
// Ethernet, SerialInterface for serial ports, RNodeInterface for LoRa, etc.)

fn detect_interfaces() -> Result<Vec<NetworkIF>> {
    use network_interface::{NetworkInterface, NetworkInterfaceConfig};

    let raw = NetworkInterface::show()?;
    let mut interfaces = Vec::new(); // mutable vector to store list of returned interfaces.

    for iface in raw {
        // Skip loopback — not useful for Reticulum
        if iface.name == "lo" || iface.name.starts_with("lo") {
            continue;
        }

        let kind = classify_interface(&iface.name);
        let power = true; // assume up; a production version would check flags

        println!(
            "  Detected interface: {} ({:?})",
            iface.name, kind
        );

        interfaces.push(NetworkIF {
            interface: kind,
            name: iface.name,
            power,
        });
    }

    if interfaces.is_empty() {
        println!("  No usable network interfaces detected.");
    }

    Ok(interfaces)
}

/// Classify an OS interface name into the AeroWAN Interface enum.
/// These heuristics cover the most common Linux and macOS naming conventions.
fn classify_interface(name: &str) -> Interface {
    let n = name.to_lowercase();
    if n.starts_with("wlan") || n.starts_with("wlp") || n.starts_with("wi") || n.starts_with("en0") {
        Interface::Wifi
    } else if n.starts_with("eth") || n.starts_with("enp") || n.starts_with("eno") || n.starts_with("en1") {
        Interface::Ethernet
    } else if n.starts_with("tty") || n.starts_with("serial") || n.starts_with("cu.") || n.starts_with("ttyusb") {
        Interface::Serial
    } else if n.contains("bt") || n.contains("bluetooth") {
        Interface::Bluetooth
    } else {
        // LoRa-based RNodes present as serial in practice; "lora" label is
        // reserved for explicitly named virtual interfaces.
        Interface::Ethernet
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

struct ReticulumNode {
    identity: PrivateIdentity,
    destination: SingleInputDestination,
}

// PrivateIdentity and Destination don't implement Debug, so we write it manually.
impl fmt::Debug for ReticulumNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReticulumNode")
            .field("identity", &"<PrivateIdentity [X25519 + Ed25519]>")
            .field(
                "destination_hash",
                &hex::encode(self.destination.hash()),
            )
            .finish()
    }
}

#[derive(Debug)]
enum Protocol {
    Iroh,
    Reticulum,
}

#[derive(Debug)]
enum Interface {
    Wifi,
    Ethernet,
    Bluetooth,
    LoRa,
    Serial,
}

#[derive(Debug)]
struct NetworkIF {
    interface: Interface,
    name: String,
    power: bool,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    println!("--- AeroWAN bootstrap ---");
    let node = AeroWAN::new().await?;
    println!("\nAeroWAN initialized: {:#?}", node);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_iroh_endpoint_creation() {
        let endpoint = bootstrap_iroh().await;
        assert!(endpoint.is_ok(), "Failed to create Iroh endpoint");
        let endpoint = endpoint.unwrap();
        println!("Iroh NodeID: {}", endpoint.id());
    }

    #[test]
    fn test_reticulum_bootstrap() {
        let node = bootstrap_reticulum();
        assert!(node.is_ok(), "Failed to bootstrap Reticulum node");
        let node = node.unwrap();
        // Hash must be exactly 16 bytes / 32 hex chars
        assert_eq!(hex::encode(node.destination.hash()).len(), 32);
        println!("Reticulum node: {:?}", node);
    }

    #[test]
    fn test_interface_detection() {
        let ifaces = detect_interfaces();
        assert!(ifaces.is_ok(), "Interface detection failed");
        println!("Interfaces: {:?}", ifaces.unwrap());
    }

    #[test]
    fn test_interface_classification() {
        assert!(matches!(classify_interface("wlan0"), Interface::Wifi));
        assert!(matches!(classify_interface("wlp3s0"), Interface::Wifi));
        assert!(matches!(classify_interface("eth0"), Interface::Ethernet));
        assert!(matches!(classify_interface("enp4s0"), Interface::Ethernet));
        assert!(matches!(classify_interface("ttyUSB0"), Interface::Serial));
    }

    #[tokio::test]
    async fn test_aerowan_new() {
        let result = AeroWAN::new().await;
        assert!(result.is_ok(), "AeroWAN init failed: {:?}", result.err());
        println!("AeroWAN: {:#?}", result.unwrap());
    }
}
