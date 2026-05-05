use iroh::SecretKey;
use rand_core::{OsRng, RngCore};
use reticulum::identity::PrivateIdentity;
use std::path::Path;

const IDENTITY: &str = "identity.key"; // identity file for the reticulum network stack.
const IROH_KEY: &str = "iroh.key"; // identity file for the iroh networking stack.

pub fn load_or_create_reticulum_identity(
    // function to load the persisted cryptographic idenity value  from the configuration directory (reticulum)
    config_dir: &Path,
) -> Result<PrivateIdentity, Box<dyn std::error::Error>> {
    let key_path = config_dir.join(IDENTITY);

    if key_path.exists() {
        // check to see if the file exists, or generate it on first launch.
        let hex = std::fs::read_to_string(&key_path)?;
        let identity = PrivateIdentity::new_from_hex_string(hex.trim())
            .map_err(|e| format!("Failed to load Reticulum identity: {:?}", e))?;
        log::info!("Loaded Reticulum identity from {}", key_path.display());
        Ok(identity)
    } else {
        log::info!("No existing reticulum key found");
        let identity = PrivateIdentity::new_from_rand(OsRng);
        std::fs::write(&key_path, identity.to_hex_string())?;
        log::info!(
            "Generated new Reticulum identity, saved to {}",
            key_path.display()
        );
        Ok(identity)
    }
}

pub fn load_or_create_iroh_key(config_dir: &Path) -> Result<SecretKey, Box<dyn std::error::Error>> {
    let key_path = config_dir.join(IROH_KEY);

    if key_path.exists() {
        let bytes = std::fs::read(&key_path)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "iroh key file has wrong length")?;
        let key = SecretKey::from_bytes(&arr);
        log::info!("Loaded iroh secret key from {}", key_path.display());
        Ok(key)
    } else {
        log::info!("No existing reticulum key found");
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let key = SecretKey::from_bytes(&bytes);
        std::fs::write(&key_path, key.to_bytes())?;
        log::info!(
            "Generated new iroh secret key, saved to {}",
            key_path.display()
        );
        Ok(key)
    }
}

pub fn load_api_key(config_dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    // function to load or generate the API key.
    let key_path = config_dir.join("api.key");

    if key_path.exists() {
        let key = std::fs::read_to_string(&key_path)?;
        Ok(key.trim().to_string())
    } else {
        let key = hex::encode({
            let mut bytes = [0u8, 32];
            OsRng.fill_bytes(&mut bytes);
            bytes
        });
        std::fs::write(&key_path, &key)?;
        log::info!("Generate a new API key, saved to {}", key_path.display());
        Ok(key)
    }
}
