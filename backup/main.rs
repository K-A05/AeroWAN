use std::fmt;
use anyhow::Result;
use iroh::{Endpoint};
use tokio::sync::mpsc;
use reticulum::packet::Packet;
use reticulum::identity::PrivateIdentity;
use reticulum::destination::SingleInputDestination;

//AeroWAN: a self replicating struct that wraps around an Iroh endpoint + Reticulum
// address.

#[derive(Debug)]
struct AeroWAN{
    iroh_endpoint:  Endpoint,
    reticulum_node:  ReticulumNode,
    protocols: Vec<Protocol>,
    interfaces: Vec<NetworkIF>,
    
}
impl AeroWAN{
    pub async fn new() -> Result<Self> { // New() acts as the single entry point for the aerowan class.
        let iroh_endpoint = bootstrap_iroh().await?; //bootstrap iroh endpoint and return
        let reticulum_node = bootstrap_reticulum()?; // bootstrap reticulum
        let interfaces = detect_interfaces()?; // detect list of interfaces and return list.
        Ok(Self{
            iroh_endpoint, 
            reticulum_node,
            protocols: vec![Protocol::Iroh, Protocol::Reticulum],
            interfaces, 
        })
    }

}

async fn bootstrap_iroh() -> Result<Endpoint> {
    let endpoint = Endpoint::builder() // create an endpoint and start listening for incoming connections.
        .bind()
        .await?;
    println!("Endpoint NodeID: {:?}", endpoint.secret_key());
    Ok(endpoint)
}

fn bootstrap_reticulum() -> Result<ReticulumNode> {
    todo!("implement reticulum bootstrap")
}

fn detect_interfaces() -> Result<Vec<NetworkIF>> {
    todo!("implement interface detection")
}

struct ReticulumNode{
    identity: PrivateIdentity,
    destination: SingleInputDestination,
}

impl fmt::Debug for ReticulumNode{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result{
    f.debug_struct("ReticulumNode")
        .field("identity", &"<PrivateIdentity>")
        .field("destination", &"<Destination>")
        .finish()
    }

}

#[derive(Debug)]
enum Protocol{
    Iroh, 
    Reticulum, 
}
#[derive(Debug)]
enum Interface{
    Wifi, 
    Ethernet,
    Bluetooth, 
    LoRa, 
    Serial, 
}
#[derive(Debug)]
struct NetworkIF{
    interface: Interface,
    name: String,
    power: bool,
}


#[tokio::main]
async fn main() -> Result<()> {
    let _node = AeroWAN::new().await?;
    println!("AeroWAN initialized!");
    // Start running the node
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_iroh_endpoint_creation() {
        let endpoint = bootstrap_iroh().await;
        assert!(endpoint.is_ok(), "Failed to create Iroh endpoint");
        
        let endpoint = endpoint.unwrap();
        let node_id = endpoint.node_id();
        println!("Created Iroh endpoint with ID: {}", node_id);
    }

    #[tokio::test]
    async fn test_aerowan_new() {
        // This will fail until you implement the other bootstrap functions
        // but it tests the overall structure
        let result = AeroWAN::new().await;
        println!("AeroWAN creation result: {:?}", result);
    }
}
