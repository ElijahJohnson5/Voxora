use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{SigningKey, VerifyingKey};
use jsonwebtoken::{DecodingKey, EncodingKey};
use sha2::{Digest, Sha256};

/// Holds the Ed25519 keypair and derived `jsonwebtoken` encoding/decoding keys.
///
/// In production, keys would come from a KMS. For Phase 1, we derive deterministically
/// from `SIGNING_KEY_SEED` so that restarts produce the same key (stable JWKS).
#[derive(Clone)]
pub struct SigningKeys {
    /// Key ID for the JWKS entry.
    pub kid: String,
    /// The `jsonwebtoken` encoding key (private).
    pub encoding: EncodingKey,
    /// The `jsonwebtoken` decoding key (public).
    pub decoding: DecodingKey,
    /// Base64url-encoded Ed25519 public key (for JWKS "x" field).
    pub public_key_b64: String,
}

impl SigningKeys {
    /// Derive an Ed25519 keypair from a seed string.
    ///
    /// The seed is hashed via SHA-256 to produce exactly 32 bytes.
    pub fn from_seed(seed: &str) -> Self {
        let hash = Sha256::digest(seed.as_bytes());
        let mut secret_bytes = [0u8; 32];
        secret_bytes.copy_from_slice(&hash);

        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key: VerifyingKey = (&signing_key).into();

        let secret = signing_key.to_bytes();
        let public_bytes = verifying_key.to_bytes();

        // jsonwebtoken expects PKCS8 DER for the private key (encoding)
        // but raw 32-byte public key bytes for the public key (decoding).
        let pkcs8_der = wrap_ed25519_private_pkcs8(&secret);

        let encoding = EncodingKey::from_ed_der(&pkcs8_der);
        let decoding = DecodingKey::from_ed_der(&public_bytes);

        let public_key_b64 = URL_SAFE_NO_PAD.encode(public_bytes);

        // Use a stable kid derived from the public key (first 8 hex chars of its SHA-256).
        let kid_hash = Sha256::digest(public_bytes);
        let kid = format!("hub-{}", hex_prefix(&kid_hash, 8));

        Self {
            kid,
            encoding,
            decoding,
            public_key_b64,
        }
    }
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    bytes
        .iter()
        .flat_map(|b| [format!("{:02x}", b)])
        .collect::<String>()[..chars]
        .to_string()
}

/// Wrap a raw 32-byte Ed25519 private key in PKCS8 DER encoding.
///
/// Structure: SEQUENCE { INTEGER 0, SEQUENCE { OID 1.3.101.112 }, OCTET STRING { OCTET STRING { key } } }
fn wrap_ed25519_private_pkcs8(secret: &[u8; 32]) -> Vec<u8> {
    let mut der = Vec::with_capacity(48);
    // SEQUENCE (46 bytes)
    der.extend_from_slice(&[0x30, 0x2e]);
    // INTEGER 0 (version)
    der.extend_from_slice(&[0x02, 0x01, 0x00]);
    // SEQUENCE { OID 1.3.101.112 }
    der.extend_from_slice(&[0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70]);
    // OCTET STRING (34 bytes) containing OCTET STRING (32 bytes) of key
    der.extend_from_slice(&[0x04, 0x22, 0x04, 0x20]);
    der.extend_from_slice(secret);
    der
}



impl std::fmt::Debug for SigningKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningKeys")
            .field("kid", &self.kid)
            .field("public_key_b64", &self.public_key_b64)
            .finish_non_exhaustive()
    }
}
