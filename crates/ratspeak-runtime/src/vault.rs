//! At-rest passcode encryption for software identities.
//!
//! Seals the 64-byte Reticulum private key with the user's passcode:
//! ```text
//!   PRK = Argon2id(passcode, salt, {m,t,p})                    (32 B, memory-hard)
//!   KEK = HKDF-SHA256(ikm = PRK, info = canonical(ver,kdf,m,t,p,salt))  (64 B)
//!   blob = token::encrypt(key64, KEK)        (AES-256-CBC + HMAC-SHA256)
//! ```
//! A wrong passcode — or any tampered KDF param (m/t/p/salt are Argon2 inputs and
//! are *also* bound into the KEK via the HKDF `info`, defeating param downgrade) —
//! yields a different KEK, so `token::decrypt` fails authentication. The on-disk
//! `identity.enc` carries the params so a future device can re-derive; only the
//! passcode is secret. The key never leaves memory once unlocked.

use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

use rns_crypto::{hkdf, random, token};
use rns_identity::identity::Identity;

const VERSION: u32 = 1;
const PRK_LEN: usize = 32;
const KEK_LEN: usize = 64; // token: 32 B HMAC + 32 B AES-256

/// Argon2id cost parameters. Chosen per-platform at encrypt time; decrypt always
/// honors the params stored in the file.
#[derive(Debug, Clone, Copy)]
pub struct VaultParams {
    pub m_cost: u32, // KiB
    pub t_cost: u32,
    pub p_cost: u32,
}

impl VaultParams {
    /// Platform-tuned defaults for *encrypting*. Mobile uses the OWASP floor to
    /// avoid OOM/stall on low-end devices; desktop goes higher.
    pub fn recommended() -> Self {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            VaultParams { m_cost: 19 * 1024, t_cost: 2, p_cost: 1 }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            VaultParams { m_cost: 47 * 1024, t_cost: 3, p_cost: 1 }
        }
    }
}

/// On-disk `identity.enc` (JSON). Contains no secret — only the passcode unlocks it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedVault {
    pub version: u32,
    pub kdf: String, // "argon2id"
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt: String,  // hex
    pub token: String, // hex (AES-CBC + HMAC blob)
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("key derivation failed: {0}")]
    Kdf(String),
    #[error("key expansion failed")]
    Hkdf,
    #[error("incorrect passcode or corrupt vault")]
    Auth,
    #[error("invalid vault: {0}")]
    Invalid(String),
    #[error("vault io: {0}")]
    Io(String),
}

/// Bind version + kdf + params + salt into the KEK so unauthenticated file fields
/// cannot be downgraded without breaking decryption.
fn canonical_info(p: VaultParams, salt: &[u8]) -> Vec<u8> {
    let mut v =
        format!("ratspeak-vault-v{VERSION}|argon2id|{}|{}|{}|", p.m_cost, p.t_cost, p.p_cost)
            .into_bytes();
    v.extend_from_slice(salt);
    v
}

fn derive_kek(
    passcode: &str,
    salt: &[u8],
    p: VaultParams,
) -> Result<Zeroizing<[u8; KEK_LEN]>, VaultError> {
    let params = Params::new(p.m_cost, p.t_cost, p.p_cost, Some(PRK_LEN))
        .map_err(|e| VaultError::Kdf(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut prk = Zeroizing::new([0u8; PRK_LEN]);
    argon
        .hash_password_into(passcode.as_bytes(), salt, prk.as_mut())
        .map_err(|e| VaultError::Kdf(e.to_string()))?;
    let okm = hkdf::hkdf_sha256(KEK_LEN, prk.as_ref(), None, Some(&canonical_info(p, salt)))
        .map_err(|_| VaultError::Hkdf)?;
    let mut kek = Zeroizing::new([0u8; KEK_LEN]);
    kek.copy_from_slice(&okm);
    Ok(kek)
}

/// Seal a 64-byte private identity key under `passcode`.
pub fn encrypt_key(passcode: &str, key: &[u8; 64]) -> Result<EncryptedVault, VaultError> {
    let p = VaultParams::recommended();
    let salt = random::random_16();
    let kek = derive_kek(passcode, &salt, p)?;
    let blob = token::encrypt(key, kek.as_ref()).map_err(|e| VaultError::Invalid(e.to_string()))?;
    Ok(EncryptedVault {
        version: VERSION,
        kdf: "argon2id".into(),
        m_cost: p.m_cost,
        t_cost: p.t_cost,
        p_cost: p.p_cost,
        salt: hex::encode(salt),
        token: hex::encode(blob),
    })
}

/// Recover the 64-byte private key from a vault. Wrong passcode / tamper → `Auth`.
pub fn decrypt_key(passcode: &str, v: &EncryptedVault) -> Result<Zeroizing<[u8; 64]>, VaultError> {
    if v.version != VERSION {
        return Err(VaultError::Invalid(format!("unsupported version {}", v.version)));
    }
    if v.kdf != "argon2id" {
        return Err(VaultError::Invalid(format!("unsupported kdf {}", v.kdf)));
    }
    let salt = hex::decode(&v.salt).map_err(|_| VaultError::Invalid("salt".into()))?;
    let blob = hex::decode(&v.token).map_err(|_| VaultError::Invalid("token".into()))?;
    let p = VaultParams { m_cost: v.m_cost, t_cost: v.t_cost, p_cost: v.p_cost };
    let kek = derive_kek(passcode, &salt, p)?;
    let pt = token::decrypt(&blob, kek.as_ref()).map_err(|_| VaultError::Auth)?;
    if pt.len() != 64 {
        return Err(VaultError::Invalid("decrypted key length".into()));
    }
    let mut key = Zeroizing::new([0u8; 64]);
    key.copy_from_slice(&pt);
    Ok(key)
}

pub fn write_vault(path: &Path, v: &EncryptedVault) -> Result<(), VaultError> {
    let json = serde_json::to_vec_pretty(v).map_err(|e| VaultError::Io(e.to_string()))?;
    // Atomic: write to a temp then rename, so a crash never leaves a partial vault.
    let tmp = path.with_extension("enc.tmp");
    std::fs::write(&tmp, &json).map_err(|e| VaultError::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| VaultError::Io(e.to_string()))?;
    Ok(())
}

pub fn read_vault(path: &Path) -> Result<EncryptedVault, VaultError> {
    let bytes = std::fs::read(path).map_err(|e| VaultError::Io(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| VaultError::Invalid(e.to_string()))
}

/// Add or change a passcode on a software identity at `id_dir`. `current` (the old
/// passcode) is required when the identity is already protected. Hardware (`.hwid`)
/// identities are rejected. Writes `identity.enc` and verifies it decrypts before
/// removing the plaintext `identity`, so an interrupted call can't lose the key.
pub fn protect_identity(
    id_dir: &Path,
    passcode: &str,
    current: Option<&str>,
) -> Result<(), VaultError> {
    if passcode.len() < 6 {
        return Err(VaultError::Invalid("passcode must be at least 6 characters".into()));
    }
    if id_dir.join("identity.hwid").exists() {
        return Err(VaultError::Invalid(
            "hardware identity is unlocked with its PIN, not a passcode".into(),
        ));
    }
    let id_file = id_dir.join("identity");
    let enc_file = id_dir.join("identity.enc");

    let key: Zeroizing<[u8; 64]> = if enc_file.exists() {
        let cur = current
            .ok_or_else(|| VaultError::Invalid("current passcode required to change".into()))?;
        decrypt_key(cur, &read_vault(&enc_file)?)?
    } else if id_file.exists() {
        let id = Identity::from_file(&id_file)
            .map_err(|e| VaultError::Invalid(format!("read identity: {e}")))?;
        id.get_private_key()
            .ok_or_else(|| VaultError::Invalid("identity has no private key".into()))?
    } else {
        return Err(VaultError::Invalid("identity not found".into()));
    };

    let vault = encrypt_key(passcode, &key)?;
    write_vault(&enc_file, &vault)?;
    // Read it back from disk and confirm it decrypts before destroying the plaintext.
    let check = decrypt_key(passcode, &read_vault(&enc_file)?)?;
    if check.as_ref() != key.as_ref() {
        let _ = std::fs::remove_file(&enc_file);
        return Err(VaultError::Invalid("vault verification failed".into()));
    }
    if id_file.exists() {
        std::fs::remove_file(&id_file).map_err(|e| VaultError::Io(e.to_string()))?;
    }
    Ok(())
}

/// Remove a passcode: decrypt `identity.enc` back to a plaintext `identity` file.
pub fn unprotect_identity(id_dir: &Path, passcode: &str) -> Result<(), VaultError> {
    let enc_file = id_dir.join("identity.enc");
    let id_file = id_dir.join("identity");
    if !enc_file.exists() {
        return Err(VaultError::Invalid("identity is not passcode-protected".into()));
    }
    let key = decrypt_key(passcode, &read_vault(&enc_file)?)?;
    let id = Identity::from_private_key(key.as_ref())
        .map_err(|e| VaultError::Invalid(format!("rebuild identity: {e}")))?;
    id.to_file(&id_file)
        .map_err(|e| VaultError::Io(format!("write identity: {e}")))?;
    // Confirm the plaintext loads before removing the vault.
    Identity::from_file(&id_file)
        .map_err(|e| VaultError::Invalid(format!("verify identity: {e}")))?;
    std::fs::remove_file(&enc_file).map_err(|e| VaultError::Io(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cheap params so tests run fast (real defaults are memory-hard).
    fn fast() -> VaultParams {
        VaultParams { m_cost: 8 * 1024, t_cost: 1, p_cost: 1 }
    }

    fn encrypt_with(passcode: &str, key: &[u8; 64], p: VaultParams) -> EncryptedVault {
        let salt = random::random_16();
        let kek = derive_kek(passcode, &salt, p).unwrap();
        let blob = token::encrypt(key, kek.as_ref()).unwrap();
        EncryptedVault {
            version: VERSION,
            kdf: "argon2id".into(),
            m_cost: p.m_cost,
            t_cost: p.t_cost,
            p_cost: p.p_cost,
            salt: hex::encode(salt),
            token: hex::encode(blob),
        }
    }

    #[test]
    fn roundtrip() {
        let key = [7u8; 64];
        let v = encrypt_with("correct horse battery", &key, fast());
        let out = decrypt_key("correct horse battery", &v).unwrap();
        assert_eq!(out.as_ref(), &key);
    }

    #[test]
    fn wrong_passcode_fails() {
        let v = encrypt_with("right-passcode", &[3u8; 64], fast());
        assert!(matches!(decrypt_key("wrong-passcode", &v), Err(VaultError::Auth)));
    }

    #[test]
    fn param_tamper_fails() {
        // Downgrading the stored params must not yield a usable KEK, even with the
        // correct passcode (proves params are bound to the KEK).
        let v = encrypt_with("pw", &[9u8; 64], fast());
        let mut tampered = v.clone();
        tampered.t_cost = v.t_cost + 1; // valid but different
        assert!(matches!(decrypt_key("pw", &tampered), Err(VaultError::Auth)));
        let mut tampered2 = v.clone();
        tampered2.m_cost = v.m_cost * 2;
        assert!(matches!(decrypt_key("pw", &tampered2), Err(VaultError::Auth)));
    }

    #[test]
    fn corrupt_blob_fails() {
        let v = encrypt_with("pw", &[1u8; 64], fast());
        let mut t = v.clone();
        // flip a byte in the ciphertext
        let mut blob = hex::decode(&t.token).unwrap();
        let n = blob.len();
        blob[n / 2] ^= 0xFF;
        t.token = hex::encode(blob);
        assert!(matches!(decrypt_key("pw", &t), Err(VaultError::Auth)));
    }
}
