use ed25519_dalek::SigningKey;
use zeroize::{Zeroize, ZeroizeOnDrop};

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SolanaResult {
    pub address: String,
    pub private_key_bs58: String,
    pub keypair_bytes_json: String,
}

pub struct SolanaGenerator;

impl SolanaGenerator {
    #[inline(always)]
    pub fn generate_raw() -> (SigningKey, [u8; 32]) {
        let mut csprng = rand::thread_rng();
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_bytes = verifying_key.to_bytes();
        (signing_key, pubkey_bytes)
    }
}

#[inline(always)]
pub fn matches_prefix_case_insensitive(pubkey_bytes: &[u8; 32], prefix_lower: &str) -> bool {
    let mut buf = [0u8; 45];
    let len = match bs58::encode(pubkey_bytes).onto(&mut buf[..]) {
        Ok(l) => l,
        Err(_) => return false,
    };

    let prefix_bytes = prefix_lower.as_bytes();
    if prefix_bytes.len() > len {
        return false;
    }

    for (i, &p) in prefix_bytes.iter().enumerate() {
        if buf[i].to_ascii_lowercase() != p {
            return false;
        }
    }

    true
}

#[inline(always)]
pub fn matches_prefix_case_sensitive(pubkey_bytes: &[u8; 32], prefix: &str) -> bool {
    let mut buf = [0u8; 45];
    let len = match bs58::encode(pubkey_bytes).onto(&mut buf[..]) {
        Ok(l) => l,
        Err(_) => return false,
    };

    let prefix_bytes = prefix.as_bytes();
    if prefix_bytes.len() > len {
        return false;
    }

    &buf[..prefix_bytes.len()] == prefix_bytes
}

pub fn format_solana_result(signing_key: SigningKey, pubkey_bytes: [u8; 32]) -> SolanaResult {
    let address = bs58::encode(&pubkey_bytes).into_string();

    let secret_bytes = signing_key.to_bytes();
    let mut full_keypair = [0u8; 64];
    full_keypair[..32].copy_from_slice(&secret_bytes);
    full_keypair[32..].copy_from_slice(&pubkey_bytes);

    let private_key_bs58 = bs58::encode(&full_keypair).into_string();
    let keypair_bytes_json = format!("{:?}", &full_keypair[..]);

    SolanaResult {
        address,
        private_key_bs58,
        keypair_bytes_json,
    }
}

pub fn validate_solana_prefix(prefix: &str) -> Result<String, String> {
    if prefix.is_empty() {
        return Err("Prefix cannot be empty.".to_string());
    }
    if prefix.len() > 44 {
        return Err("Prefix length cannot exceed 44 characters.".to_string());
    }

    for c in prefix.chars() {
        if !BASE58_ALPHABET.contains(&(c as u8)) {
            let hint = match c {
                '0' => " ('0' is omitted in Base58)",
                'O' => " ('O' is omitted in Base58)",
                'I' => " ('I' is omitted in Base58)",
                'l' => " ('l' is omitted in Base58)",
                _ => "",
            };
            return Err(format!("Invalid character '{}' for Solana Base58{hint}.", c));
        }
    }

    Ok(prefix.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solana_generation() {
        let (sk, pubkey) = SolanaGenerator::generate_raw();
        let res = format_solana_result(sk, pubkey);
        assert!(!res.address.is_empty());
        assert!(!res.private_key_bs58.is_empty());
        assert!(res.keypair_bytes_json.starts_with('['));

        let prefix = &res.address[..3];
        assert!(matches_prefix_case_sensitive(&pubkey, prefix));
        assert!(matches_prefix_case_insensitive(&pubkey, &prefix.to_ascii_lowercase()));
    }

    #[test]
    fn test_solana_validation() {
        assert!(validate_solana_prefix("abc").is_ok());
        assert!(validate_solana_prefix("ABC").is_ok());
        // Base58 excludes 0, O, I, l
        assert!(validate_solana_prefix("0").is_err());
        assert!(validate_solana_prefix("O").is_err());
        assert!(validate_solana_prefix("I").is_err());
        assert!(validate_solana_prefix("l").is_err());
        assert!(validate_solana_prefix("").is_err());
    }
}
