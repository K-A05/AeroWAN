use crate::daemon::config::{Config, InterfaceConfig};
use reticulum::iface::tcp_client::TcpClient;
use reticulum::iface::tcp_server::TcpServer;
use reticulum::transport::{Transport, TransportConfig};

pub struct ReticulumTransport;

impl ReticulumTransport {
    pub async fn init(
        config: &Config,
        config_dir: &std::path::Path,
    ) -> Result<Transport, Box<dyn std::error::Error>> {
        let identity = crate::utils::identity::load_or_create_reticulum_identity(config_dir)?;
        let transport = Transport::new({
            let mut cfg = TransportConfig::new(
                "aerowan-daemon",
                &identity,
                config.reticulum.enable_transport,
            );
            cfg.set_retransmit(config.reticulum.enable_transport);
            cfg
        });

        let iface_manager = transport.iface_manager();

        for (name, iface_config) in &config.interfaces {
            match iface_config {
                InterfaceConfig::TCPServerInterface {
                    interface_enabled,
                    bind_host,
                    bind_port,
                } => {
                    if *interface_enabled {
                        let addr = format!("{}:{}", bind_host.trim_end_matches(':'), bind_port);
                        log::info!("Enabling '{}': TCP Server on {}", name, addr);
                        iface_manager.lock().await.spawn(
                            TcpServer::new(addr, iface_manager.clone()),
                            TcpServer::spawn,
                        );
                    }
                }
                InterfaceConfig::TCPClientInterface {
                    interface_enabled,
                    target_host,
                    target_port,
                } => {
                    if *interface_enabled {
                        let addr = format!("{}:{}", target_host.trim_end_matches(':'), target_port);
                        log::info!("Enabling '{}': TCP Client to {}", name, addr);
                        iface_manager
                            .lock()
                            .await
                            .spawn(TcpClient::new(addr), TcpClient::spawn);
                    }
                }
            }
        }

        Ok(transport)
    }
}
