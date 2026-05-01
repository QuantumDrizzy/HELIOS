pub mod protocol {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub enum Request {
        Authenticate {
            identity: String,
            nonce: [u8; 32],
            signature: Vec<u8>,
        },
        GetPeerKemPk { peer_id: String },
        InitiateShmHandshake {
            initiator: String,
            target: String,
        },
        SignCheckpoint {
            hash: [u8; 32],
        },
        VerifyBinary {
            binary_hash: [u8; 32],
            signature: Vec<u8>,
        },
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub enum Response {
        Authenticated { session_handle: u64 },
        PeerKemPk { peer_id: String, pk_bytes: Vec<u8> },
        ShmHandshakeTokens {
            session_id: u64,
            initiator_ct: Vec<u8>,
            target_ct: Vec<u8>,
        },
        Signature(Vec<u8>),
        Verified(bool),
        Error(String),
    }
}

pub mod error {
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum SentinelError {
        #[error("IO: {0}")]
        Io(#[from] std::io::Error),
        #[error("Crypto operation failed: {0}")]
        Crypto(&'static str),
        #[error("Authentication failed for peer {0}")]
        Auth(String),
        #[error("Unknown peer: {0}")]
        UnknownPeer(String),
        #[error("Invalid protocol message")]
        InvalidMessage,
        #[error("Trust anchor verification failed")]
        Untrusted,
        #[error("Key access denied")]
        KeyAccessDenied,
    }
}
