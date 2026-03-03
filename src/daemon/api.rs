pub struct LANServer{
    handle: tokio::task::JoinHandle<()>,
}

impl LANServer {
    pub fn start(
        config: &Config,
        config_dir: &Path,
        iroh_endpoint: Endpoint,
    ) -> Self{

    }
}