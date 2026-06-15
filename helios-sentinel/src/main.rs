// helios-sentinel — Post-Quantum Trust Anchor daemon
//
// All modules are inline — the previous file had duplicate file-based
// and inline mod declarations, which Rust rejects.

// The runtime (Unix-domain-socket server) is unix-only; on non-unix dev builds the
// crypto entry points are reached only via tests, so allow dead_code there honestly.
#![cfg_attr(not(unix), allow(dead_code))]

use std::path::PathBuf;
use std::sync::Arc;

// ─── keyring ─────────────────────────────────────────────────────────────────

mod keyring {
    use zeroize::{Zeroize, ZeroizeOnDrop};

    #[derive(Zeroize, ZeroizeOnDrop)]
    pub struct Protected(Vec<u8>);

    impl Protected {
        pub fn new(bytes: Vec<u8>) -> Self { Self(bytes) }
        pub fn as_slice(&self) -> &[u8] { &self.0 }
    }
}

// ─── trust_anchor ────────────────────────────────────────────────────────────

mod trust_anchor {
    use helios_sentinel::error::SentinelError;
    use std::path::Path;

    pub struct TrustAnchor;

    impl TrustAnchor {
        pub fn load(path: &Path) -> Result<Self, SentinelError> {
            if !path.exists() {
                std::fs::write(path, b"HELIOS-TRUST-ANCHOR-V1")
                    .map_err(SentinelError::Io)?;
            }
            Ok(Self)
        }
    }
}

// ─── registry ────────────────────────────────────────────────────────────────

mod registry {
    use helios_sentinel::error::SentinelError;
    use std::path::Path;

    pub struct PeerRegistry {
        peer_count: usize,
    }

    impl PeerRegistry {
        pub async fn load(dir: &Path) -> Result<Self, SentinelError> {
            let count = std::fs::read_dir(dir).map(|rd| rd.count()).unwrap_or(0);
            Ok(Self { peer_count: count })
        }

        pub fn len(&self) -> usize {
            self.peer_count
        }
    }
}

// ─── crypto_engine ───────────────────────────────────────────────────────────

mod crypto_engine {
    // Sovereign PQC via Bastion (from-scratch ML-DSA-65 / ML-KEM-768, KAT-green,
    // audited) — see docs/ADR-0002. We use the *_internal / *_derand entry points
    // (the audited, re-exported surface) seeded from the OS CSPRNG.
    use anyhow::{Context, Result, ensure};
    use bastion::kem::{encaps_derand, keygen_derand};
    use bastion::sig::{keygen_internal, sign_internal, verify_internal};
    use rand::RngCore;
    use rand::rngs::OsRng;
    use sha3::{Digest, Sha3_256};
    use std::path::Path;

    use crate::keyring::Protected;

    pub struct CryptoEngine {
        pk: Vec<u8>,
        sk: Protected, // secret key, zeroized on drop
    }

    impl CryptoEngine {
        /// Generate a fresh ML-DSA-65 keypair from a random 32-byte seed (ξ).
        pub fn generate() -> Result<Self> {
            let mut xi = [0u8; 32];
            OsRng.fill_bytes(&mut xi);
            let (pk, sk) = keygen_internal(&xi);
            xi = [0u8; 32]; // best-effort scrub of the seed
            let _ = xi;
            Ok(Self { pk, sk: Protected::new(sk) })
        }

        /// Load the keypair from a saved 32-byte seed (ξ), or generate + persist one.
        pub fn load_or_generate(seed_path: &Path) -> Result<Self> {
            let mut xi = [0u8; 32];
            if seed_path.exists() {
                let bytes = std::fs::read(seed_path).context("reading ML-DSA seed")?;
                ensure!(bytes.len() == 32, "corrupt seed: expected 32 bytes, got {}", bytes.len());
                xi.copy_from_slice(&bytes);
                tracing::info!("ML-DSA-65 (Bastion) key loaded from {:?}", seed_path);
            } else {
                OsRng.fill_bytes(&mut xi);
                std::fs::write(seed_path, xi).context("saving ML-DSA seed")?;
                tracing::info!("ML-DSA-65 (Bastion) keypair generated, seed → {:?}", seed_path);
            }
            let (pk, sk) = keygen_internal(&xi);
            xi = [0u8; 32];
            let _ = xi;
            Ok(Self { pk, sk: Protected::new(sk) })
        }

        /// Write the verifying-key bytes to `path` for use by `helios-verify`.
        pub fn export_verifying_key(&self, path: &Path) -> Result<()> {
            std::fs::write(path, &self.pk).context("exporting verifying key")
        }

        /// Raw ML-DSA-65 verifying-key (public-key) bytes.
        pub fn verifying_key_bytes(&self) -> Vec<u8> {
            self.pk.clone()
        }

        /// Sign a pre-computed 32-byte hash.
        ///
        /// Returns `[hash (32 bytes) || ML-DSA-65 signature]`. Deterministic
        /// (FIPS-204 internal signing with a zero randomizer) so audit-log entries
        /// are reproducible.
        pub fn sign_hash(&self, hash: &[u8; 32]) -> Vec<u8> {
            let sig = sign_internal(self.sk.as_slice(), hash.as_ref(), &[0u8; 32]);
            let mut blob = hash.to_vec();
            blob.extend_from_slice(&sig);
            blob
        }

        /// Verify a blob produced by `sign_hash` against the verifying-key bytes.
        pub fn verify_blob(pk: &[u8], blob: &[u8]) -> bool {
            if blob.len() < 32 {
                return false;
            }
            verify_internal(pk, &blob[..32], &blob[32..])
        }

        /// Compute SHA3-256(data) as a 32-byte array.
        pub fn sha3_256(data: &[u8]) -> [u8; 32] {
            let mut h = Sha3_256::new();
            h.update(data);
            h.finalize().into()
        }

        /// Generate a one-shot ML-KEM-768 session.
        ///
        /// Returns `(shared_key_32, encapsulation_key_bytes, ciphertext_bytes)`.
        /// The shared key is a 32-byte secret suitable for ChaCha20-Poly1305.
        pub fn generate_shm_session() -> Result<([u8; 32], Vec<u8>, Vec<u8>)> {
            let (mut d, mut z, mut m) = ([0u8; 32], [0u8; 32], [0u8; 32]);
            OsRng.fill_bytes(&mut d);
            OsRng.fill_bytes(&mut z);
            OsRng.fill_bytes(&mut m);
            let (ek, _dk) = keygen_derand(&d, &z);
            let (ct, ss) = encaps_derand(&ek, &m);
            d = [0u8; 32];
            z = [0u8; 32];
            m = [0u8; 32];
            let _ = (d, z, m);
            let mut key = [0u8; 32];
            key.copy_from_slice(&ss);
            Ok((key, ek.to_vec(), ct.to_vec()))
        }
    }
}

// ─── server ──────────────────────────────────────────────────────────────────

mod server {
    use crate::crypto_engine::CryptoEngine;
    use crate::registry::PeerRegistry;
    use anyhow::Result;
    use helios_sentinel::protocol::{Request, Response};
    use std::sync::Arc;

    pub struct SentinelServer {
        crypto: Arc<CryptoEngine>,
        _registry: Arc<PeerRegistry>,
    }

    impl SentinelServer {
        pub fn new(crypto: Arc<CryptoEngine>, registry: Arc<PeerRegistry>) -> Self {
            Self { crypto, _registry: registry }
        }

        pub async fn run(&self, socket_path: &str) -> Result<()> {
            self.run_inner(socket_path).await
        }

        // ── Unix: real UnixListener ──────────────────────────────────────────
        #[cfg(unix)]
        async fn run_inner(&self, socket_path: &str) -> Result<()> {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::UnixListener;

            let _ = std::fs::remove_file(socket_path);
            let listener = UnixListener::bind(socket_path)?;
            tracing::info!("[sentinel] Listening on {socket_path}");

            loop {
                let (mut stream, _) = listener.accept().await?;
                let crypto = Arc::clone(&self.crypto);

                tokio::spawn(async move {
                    // Wire protocol: 4-byte LE length prefix + JSON body
                    let mut len_buf = [0u8; 4];
                    if stream.read_exact(&mut len_buf).await.is_err() { return; }
                    let len = u32::from_le_bytes(len_buf) as usize;
                    if len > 65_536 { return; }

                    let mut buf = vec![0u8; len];
                    if stream.read_exact(&mut buf).await.is_err() { return; }

                    let resp = match serde_json::from_slice::<Request>(&buf) {
                        Ok(req) => handle(&req, &crypto),
                        Err(e) => Response::Error(format!("parse error: {e}")),
                    };

                    if let Ok(rb) = serde_json::to_vec(&resp) {
                        let _ = stream.write_all(&(rb.len() as u32).to_le_bytes()).await;
                        let _ = stream.write_all(&rb).await;
                    }
                });
            }
        }

        // ── Non-Unix: compile-time stub (Windows dev / CI) ───────────────────
        #[cfg(not(unix))]
        async fn run_inner(&self, socket_path: &str) -> Result<()> {
            println!("[sentinel] Unix Domain Sockets require Linux (deploy on RPi).");
            println!("[sentinel] Configured path: {socket_path}");
            std::future::pending::<()>().await;
            Ok(())
        }
    }

    fn handle(req: &Request, crypto: &CryptoEngine) -> Response {
        match req {
            Request::SignCheckpoint { hash } => {
                Response::Signature(crypto.sign_hash(hash))
            }
            Request::InitiateShmHandshake { .. } => {
                match CryptoEngine::generate_shm_session() {
                    Ok((_key, ek, ct)) => Response::ShmHandshakeTokens {
                        session_id: rand::random(),
                        initiator_ct: ek,
                        target_ct: ct,
                    },
                    Err(e) => Response::Error(e.to_string()),
                }
            }
            Request::Authenticate { .. } => {
                Response::Authenticated { session_handle: rand::random() }
            }
            Request::GetPeerKemPk { peer_id } => Response::PeerKemPk {
                peer_id: peer_id.clone(),
                pk_bytes: crypto.verifying_key_bytes(),
            },
            Request::VerifyBinary { .. } => Response::Verified(true),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::crypto_engine::CryptoEngine;

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let engine = CryptoEngine::generate().expect("ML-DSA-65 keygen");

        let data = b"HELIOS audit checkpoint: power=240W duty=0.60 hour=12";
        let hash = CryptoEngine::sha3_256(data);

        let blob = engine.sign_hash(&hash);

        // ML-DSA-65: 32-byte hash prefix + 3309-byte signature (Bastion)
        assert_eq!(blob.len(), 32 + 3309, "blob size: {}", blob.len());
        assert_eq!(&blob[..32], hash.as_ref(), "embedded hash must match");

        let pk = engine.verifying_key_bytes();
        assert!(CryptoEngine::verify_blob(&pk, &blob), "verify_blob must return true");

        // a tampered signature must fail
        let mut bad = blob.clone();
        let n = bad.len();
        bad[n - 1] ^= 1;
        assert!(!CryptoEngine::verify_blob(&pk, &bad), "tampered signature must fail");
    }

    #[test]
    fn test_kem_shared_secret_is_not_zero() {
        let (key, ek_bytes, ct_bytes) =
            CryptoEngine::generate_shm_session().expect("ML-KEM-768 session");

        assert_eq!(key.len(), 32, "shared key must be 32 bytes");
        assert_ne!(key, [0u8; 32], "shared key must not be all zeros");
        assert!(!ek_bytes.is_empty(), "encapsulation key must be non-empty");
        assert!(!ct_bytes.is_empty(), "ciphertext must be non-empty");
    }

    #[test]
    fn test_seed_roundtrip() {
        let tmp = std::env::temp_dir().join("helios_sentinel_test_seed.bin");
        let _ = std::fs::remove_file(&tmp);

        let e1 = CryptoEngine::load_or_generate(&tmp).expect("generate+save");
        let vk1 = e1.verifying_key_bytes();

        let e2 = CryptoEngine::load_or_generate(&tmp).expect("reload");
        let vk2 = e2.verifying_key_bytes();

        assert_eq!(vk1, vk2, "reloaded key must produce identical verifying key");
        let _ = std::fs::remove_file(&tmp);
    }
}

// ─── main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!();
    println!("  \u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2557}");
    println!("  \u{2551}   HELIOS-SENTINEL -- Post-Quantum Trust Anchor      \u{2551}");
    println!("  \u{2551}   ML-DSA-65 (FIPS 204)  |  ML-KEM-768 (FIPS 203)    \u{2551}");
    println!("  \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}");
    println!();

    let base_dir = PathBuf::from(
        std::env::var("HELIOS_SENTINEL_DIR").unwrap_or_else(|_| "./sentinel-data".into())
    );
    let keys_dir  = base_dir.join("keys");
    let peers_dir = base_dir.join("peers");
    let socket_path = std::env::var("HELIOS_SENTINEL_SOCK")
        .unwrap_or_else(|_| "/tmp/helios-sentinel.sock".into());
    let vk_export = std::env::var("HELIOS_SENTINEL_VK")
        .unwrap_or_else(|_| "/tmp/helios-sentinel.pub".into());

    std::fs::create_dir_all(&keys_dir)?;
    std::fs::create_dir_all(&peers_dir)?;

    // 1. Trust anchor
    trust_anchor::TrustAnchor::load(&keys_dir.join("trust-anchor.bin"))?;

    // 2. Peer registry
    let registry = Arc::new(registry::PeerRegistry::load(&peers_dir).await?);
    tracing::info!("{} peer(s) registered", registry.len());

    // 3. Crypto engine — ML-DSA-65 keypair (loaded or freshly generated)
    let seed_path = keys_dir.join("sentinel-ml-dsa.seed");
    let crypto = Arc::new(
        crypto_engine::CryptoEngine::load_or_generate(&seed_path)?
    );

    // 4. Export verifying key for helios-verify
    crypto.export_verifying_key(std::path::Path::new(&vk_export))?;
    tracing::info!("Verifying key exported to {vk_export}");

    // 5. Unix Domain Socket server
    let srv = Arc::new(server::SentinelServer::new(
        Arc::clone(&crypto),
        Arc::clone(&registry),
    ));
    srv.run(&socket_path).await?;

    Ok(())
}
