/// Simple XOR-based "encryption" for secrets in config (in-process stub).
/// In production, replace with a real KMS-backed implementation.

pub struct SecretEncryptor {
    key: Vec<u8>,
}

impl SecretEncryptor {
    pub fn new(key: &[u8]) -> Self {
        assert!(!key.is_empty(), "encryption key must not be empty");
        Self { key: key.to_vec() }
    }

    pub fn encrypt(&self, plaintext: &str) -> Vec<u8> {
        plaintext
            .bytes()
            .enumerate()
            .map(|(i, b)| b ^ self.key[i % self.key.len()])
            .collect()
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> String {
        let bytes: Vec<u8> = ciphertext
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ self.key[i % self.key.len()])
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Rotate the key — re-encrypts are needed after rotation.
    pub fn rotate_key(&mut self, new_key: &[u8]) {
        assert!(!new_key.is_empty(), "new key must not be empty");
        self.key = new_key.to_vec();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let enc = SecretEncryptor::new(b"my-secret-key-32");
        let plaintext = "api_key=super_secret_value_123";
        let cipher = enc.encrypt(plaintext);
        assert_ne!(cipher, plaintext.as_bytes());
        let recovered = enc.decrypt(&cipher);
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn key_rotation_changes_ciphertext() {
        let mut enc = SecretEncryptor::new(b"key1");
        let cipher1 = enc.encrypt("hello");
        enc.rotate_key(b"key2");
        let cipher2 = enc.encrypt("hello");
        assert_ne!(cipher1, cipher2);
    }
}
