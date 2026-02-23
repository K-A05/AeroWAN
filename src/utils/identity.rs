use std::path::Path;
use reticulum::identity::PrivateIdentity;
use rand_core::OsRng;

const IDENTITY: &str = "identity.key";

pub fn load_or_create_reticulum_identity(config_dir: &Path) -> Result<PrivateIdentity, Box<dyn std::error::Error>> {
    let key_path = config_dir.join(IDENTITY);

    if key_path.exists() {
        let hex = std::fs::read_to_string(&key_path)?;
        let identity = PrivateIdentity::new_from_hex_string(hex.trim()).map_err(|e| format!("Failed to load Reticulum identity: {:?}", e))?;
        log::info!("Loaded Reticulum identity from {}", key_path.display());
        Ok(identity)
    } else {
        let identity = PrivateIdentity::new_from_rand(OsRng);
        std::fs::write(&key_path, identity.to_hex_string())?;
        log::info!("Generated new Reticulum identity, saved to {}", key_path.display());
        Ok(identity)
    }
}

