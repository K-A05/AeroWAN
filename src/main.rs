use anyhow::Result;
use iroh::{Endpoint, EndpointAddr};
use tokio::sync::mpsc;
use reticulum::packet::Packet;
use reticulum::identity::PrivateIdentity;
use reticulum::destination::SingleInputDestination;

//AeroWAN: a self replicating struct that wraps around an Iroh endpoint + Reticulum
// address.
struct AeroWAN{
    iroh_endpoint:  Endpoint,
    reticulum_node:  ReticulumNode,
    protocols: Vec<Protocol>,
    interfaces: Vec<NetworkIF>,
    
}

impl AeroWAN{
    pub async fn new() -> Result<Self> {
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
    todo!("implement iroh bootstrap")
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

enum Protocol{
    Iroh, 
    Reticulum, 
}

enum Interface{
    Wifi, 
    Ethernet,
    Bluetooth, 
    LoRa, 
    Serial, 
}

struct NetworkIF{
    interface: Interface,
    name: String,
    power: bool,
}


fn main(){
    println!("Hello from AeroWAN");
}
