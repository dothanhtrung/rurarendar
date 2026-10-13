use argon2::{
    Algorithm,
    Argon2,
    Params,
    Version,
};
use chacha20poly1305::aead::{
    Aead,
    KeyInit,
    Payload,
};
use chacha20poly1305::{
    Key,
    XChaCha20Poly1305,
    XNonce,
};
use rand::TryRng;
use serde::{
    Deserialize,
    Serialize,
};

const VERIFIER_PLAIN: &[u8] = b"lich-am-duong-data-key-v1";
pub const ALG_NONE: &str = "none";
pub const ALG_AEAD: &str = "xchacha20poly1305";

#[derive(Clone)]
pub struct DataKey(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KdfParams {
    pub alg: String,
    pub salt: String,
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
    pub key_len: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blob {
    pub alg: String,
    pub nonce: String,
    pub ciphertext: String,
    pub key_revision: i32,
}

pub fn random_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    rand::rngs::SysRng.try_fill_bytes(&mut salt).expect("os rng");
    salt
}

pub fn default_kdf(salt: &[u8]) -> KdfParams {
    KdfParams {
        alg: "argon2id".into(),
        salt: b64(salt),
        m_kib: 19456,
        t: 2,
        p: 1,
        key_len: 32,
    }
}

pub fn derive(passphrase: &str, kdf: &KdfParams) -> anyhow::Result<DataKey> {
    let salt = unb64(&kdf.salt)?;
    let params =
        Params::new(kdf.m_kib, kdf.t, kdf.p, Some(kdf.key_len as usize)).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), &salt, &mut key)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(DataKey(key))
}

pub fn encrypt(key: &DataKey, aad: &[u8], plaintext: &[u8], key_revision: i32) -> anyhow::Result<Blob> {
    let mut nonce_bytes = [0u8; 24];
    rand::rngs::SysRng.try_fill_bytes(&mut nonce_bytes).expect("os rng");
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key.0));
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, Payload { msg: plaintext, aad })
        .map_err(|_| anyhow::anyhow!("encrypt failed"))?;
    Ok(Blob {
        alg: ALG_AEAD.into(),
        nonce: b64(&nonce_bytes),
        ciphertext: b64(&ciphertext),
        key_revision,
    })
}

pub fn seal(key: Option<&DataKey>, aad: &[u8], plaintext: &[u8], key_revision: i32) -> anyhow::Result<Blob> {
    match key {
        None => Ok(Blob {
            alg: ALG_NONE.into(),
            nonce: String::new(),
            ciphertext: b64(plaintext),
            key_revision: 0,
        }),
        Some(k) => encrypt(k, aad, plaintext, key_revision),
    }
}

pub fn is_plain(blob: &Blob) -> bool {
    blob.alg == ALG_NONE || blob.alg.is_empty()
}

pub fn open(key: Option<&DataKey>, aad: &[u8], blob: &Blob) -> anyhow::Result<Vec<u8>> {
    if is_plain(blob) {
        return unb64(&blob.ciphertext);
    }
    let Some(k) = key else {
        anyhow::bail!("encrypted item but encrypt key is empty");
    };
    decrypt(k, aad, blob)
}

pub fn open_with_keys(current: Option<&DataKey>, old: Option<&DataKey>, aad: &[u8], blob: &Blob) -> anyhow::Result<Vec<u8>> {
    if is_plain(blob) {
        return unb64(&blob.ciphertext);
    }
    if let Some(k) = current
        && let Ok(plain) = decrypt(k, aad, blob)
    {
        return Ok(plain);
    }
    if let Some(k) = old
        && let Ok(plain) = decrypt(k, aad, blob)
    {
        return Ok(plain);
    }
    anyhow::bail!("không giải mã được (sai khoá mã hoá)")
}

pub fn decrypt(key: &DataKey, aad: &[u8], blob: &Blob) -> anyhow::Result<Vec<u8>> {
    let nonce_bytes = unb64(&blob.nonce)?;
    let ciphertext = unb64(&blob.ciphertext)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key.0));
    let nonce = XNonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, Payload { msg: &ciphertext, aad })
        .map_err(|_| anyhow::anyhow!("decrypt failed"))
}

pub fn verifier_blob(key: &DataKey, key_revision: i32) -> anyhow::Result<Blob> {
    encrypt(key, b"verifier", VERIFIER_PLAIN, key_revision)
}

pub fn verify_blob(key: &DataKey, blob: &Blob) -> bool {
    matches!(decrypt(key, b"verifier", blob), Ok(plain) if plain == VERIFIER_PLAIN)
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(s: &str) -> anyhow::Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| anyhow::anyhow!("invalid base64"))
}
