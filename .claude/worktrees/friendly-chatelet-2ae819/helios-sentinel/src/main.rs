mod crypto_engine;
mod registry;
mod server;
mod trust_anchor;
mod keyring;

use helios_sentinel::error::SentinelError;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!();
    println!("  ╔════════════════════════════════════════════════════════════╗");
    println!("  ║        HELIOS-SENTINEL :: Post-Quantum Trust Anchor        ║");
    println!("  ║              Zero-Cloud :: Bare-Metal :: Sovereign         ║");
    println!("  ╚════════════════════════════════════════════════════════════╝");
    println!();

    // Rutas por defecto adaptadas a tu entorno Windows/Local
    let base_dir = PathBuf::from("c:/Users/Drizzy/Desktop/helios-sentinel/config");
    let keys_dir = base_dir.join("keys");
    let peers_dir = base_dir.join("peers");
    let socket_path = "c:/Users/Drizzy/Desktop/helios-sentinel/sentinel.sock";

    // Asegurar directorios
    std::fs::create_dir_all(&keys_dir)?;
    std::fs::create_dir_all(&peers_dir)?;

    // 1. Cargar Trust Anchor (vk raíz)
    // Para el demo, si no existe la creamos (en producción es offline)
    let anchor_path = keys_dir.join("slh-dsa-root.vk");
    if !anchor_path.exists() {
        std::fs::write(&anchor_path, b"DUMMY_ROOT_VK_SLH_DSA")?;
    }
    
    let anchor = Arc::new(trust_anchor::TrustAnchor::load_from_file(&anchor_path)?);

    // 2. Cargar Registro de Peers
    let registry = Arc::new(registry::PeerRegistry::load_from_dir(&peers_dir, &anchor).await?);
    tracing::info!("{} peers validados cargados.", registry.list().len());

    // 3. Cargar Crypto Engine (sk del daemon)
    let sk_path = keys_dir.join("sentinel-ml-dsa.sk");
    if !sk_path.exists() {
        std::fs::write(&sk_path, vec![0u8; 3300])?; // Mock SK
    }
    let crypto = Arc::new(crypto_engine::CryptoEngine::load(&sk_path)?);

    // 4. Iniciar Servidor (UDS)
    let server = Arc::new(server::SentinelServer::new(crypto, registry));
    
    // En Windows usaremos TCP localhost para el demo si UDS falla, 
    // pero mantengamos la lógica de socket para cuando migres a Jetson.
    server.run(socket_path).await?;

    Ok(())
}

// --- Módulos internos (simplificados para materialización) ---

mod keyring {
    use zeroize::{Zeroize, ZeroizeOnDrop};
    #[derive(Zeroize, ZeroizeOnDrop)]
    pub struct Protected(Vec<u8>);
    impl Protected {
        pub fn new(bytes: Vec<u8>) -> Self { Self(bytes) }
        pub fn as_slice(&self) -> &[u8] { &self.0 }
    }
    pub struct PeerPublicKeys {
        pub identity: String,
        pub ml_kem_pk: Vec<u8>,
        pub ml_dsa_vk: Vec<u8>,
    }
}

mod trust_anchor {
    use crate::keyring::PeerPublicKeys;
    use helios_sentinel::error::SentinelError;
    pub struct TrustAnchor { pub vk_bytes: Vec<u8> }
    impl TrustAnchor {
        pub fn load_from_file(path: &std::path::Path) -> Result<Self, SentinelError> {
            let b = std::fs::read(path).map_err(|_| SentinelError::Crypto("Anchor missing"))?;
            Ok(Self { vk_bytes: b })
        }
        pub fn verify_manifest(&self, m: &PeerManifest) -> Result<PeerPublicKeys, SentinelError> {
            // Aquí iría la lógica SLH-DSA
            Ok(PeerPublicKeys {
                identity: m.identity.clone(),
                ml_kem_pk: hex::decode(&m.ml_kem_pk_hex).unwrap_or_default(),
                ml_dsa_vk: hex::decode(&m.ml_dsa_vk_hex).unwrap_or_default(),
            })
        }
    }
    #[derive(serde::Deserialize)]
    pub struct PeerManifest {
        pub identity: String,
        pub ml_kem_pk_hex: String,
        pub ml_dsa_vk_hex: String,
        pub root_signature_hex: String,
    }
}

mod registry {
    use crate::keyring::PeerPublicKeys;
    use crate::trust_anchor::{TrustAnchor, PeerManifest};
    use helios_sentinel::error::SentinelError;
    use std::collections::HashMap;
    pub struct PeerRegistry { pub peers: HashMap<String, PeerPublicKeys> }
    impl PeerRegistry {
        pub async fn load_from_dir(dir: &std::path::Path, anchor: &TrustAnchor) -> Result<Self, SentinelError> {
            let mut peers = HashMap::new();
            if let Ok(mut entries) = std::fs::read_dir(dir) {
                while let Some(Ok(entry)) = entries.next() {
                    if let Ok(b) = std::fs::read(entry.path()) {
                        if let Ok(m) = serde_json::from_slice::<PeerManifest>(&b) {
                            if let Ok(p) = anchor.verify_manifest(&m) {
                                peers.insert(p.identity.clone(), p);
                            }
                        }
                    }
                }
            }
            Ok(Self { peers })
        }
        pub fn get(&self, id: &str) -> Result<&PeerPublicKeys, SentinelError> {
            self.peers.get(id).ok_or_else(|| SentinelError::UnknownPeer(id.into()))
        }
        pub fn list(&self) -> Vec<&String> { self.peers.keys().collect() }
    }
}

mod crypto_engine {
    use crate::keyring::Protected;
    use helios_sentinel::error::SentinelError;
    pub struct CryptoEngine { pub sk: Protected }
    impl CryptoEngine {
        pub fn load(path: &std::path::Path) -> Result<Self, SentinelError> {
            let b = std::fs::read(path).map_err(|_| SentinelError::Crypto("SK missing"))?;
            Ok(Self { sk: Protected::new(b) })
        }
        pub fn sign_checkpoint(&self, hash: &[u8; 32]) -> Vec<u8> {
            let mut sig = vec![0u8; 3300];
            sig[..32].copy_from_slice(hash);
            sig
        }
        pub fn generate_shm_session(&self, _pki: &[u8], _pkt: &[u8]) -> Result<([u8; 32], Vec<u8>, Vec<u8>), SentinelError> {
            let key = [0u8; 32];
            Ok((key, vec![0u8; 1088], vec![0u8; 1088]))
        }
    }
}

mod server {
    use crate::crypto_engine::CryptoEngine;
    use crate::registry::PeerRegistry;
    use helios_sentinel::error::SentinelError;
    use helios_sentinel::protocol::{Request, Response};
    use std::sync::Arc;
    pub struct SentinelServer { crypto: Arc<CryptoEngine>, registry: Arc<PeerRegistry> }
    impl SentinelServer {
        pub fn new(crypto: Arc<CryptoEngine>, registry: Arc<PeerRegistry>) -> Self {
            Self { crypto, registry }
        }
        pub async fn run(&self, path: &str) -> Result<(), SentinelError> {
            // En Windows para demo, emulamos el socket
            println!("[helios-sentinel] Iniciando servidor en {}", path);
            Ok(())
        }
        pub async fn handle_connection(&self, _stream: tokio::net::UnixStream) -> Result<(), SentinelError> {
            Ok(())
        }
    }
}
