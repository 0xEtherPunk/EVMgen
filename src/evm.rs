use secp256k1::rand::rngs::ThreadRng;
use secp256k1::{PublicKey, Scalar, Secp256k1, SecretKey};
use tiny_keccak::{Hasher, Keccak};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct EvmResult {
    pub address: String,
    pub private_key_hex: String,
}

pub struct EvmGenerator {
    secp: Secp256k1<secp256k1::All>,
    rng: ThreadRng,
    base_secret: SecretKey,
    current_public: PublicKey,
    offset: u32,
    batch_size: u32,
}

impl EvmGenerator {
    pub fn new(batch_size: u32) -> Self {
        let secp = Secp256k1::new();
        let mut rng = secp256k1::rand::thread_rng();
        let base_secret = SecretKey::new(&mut rng);
        let current_public = PublicKey::from_secret_key(&secp, &base_secret);
        Self {
            secp,
            rng,
            base_secret,
            current_public,
            offset: 0,
            batch_size,
        }
    }

    #[inline(always)]
    pub fn next_address(&mut self) -> [u8; 20] {
        if self.offset >= self.batch_size {
            self.base_secret = SecretKey::new(&mut self.rng);
            self.current_public = PublicKey::from_secret_key(&self.secp, &self.base_secret);
            self.offset = 0;
        }

        let uncompressed = self.current_public.serialize_uncompressed();
        let mut hasher = Keccak::v256();
        hasher.update(&uncompressed[1..65]);
        let mut hash = [0u8; 32];
        hasher.finalize(&mut hash);

        let mut address = [0u8; 20];
        address.copy_from_slice(&hash[12..32]);
        address
    }

    #[inline(always)]
    pub fn step(&mut self) {
        self.current_public = self
            .current_public
            .add_exp_tweak(&self.secp, &Scalar::ONE)
            .expect("Valid point addition");
        self.offset += 1;
    }

    pub fn current_secret_key(&self) -> SecretKey {
        if self.offset == 0 {
            self.base_secret
        } else {
            let mut tweak_bytes = [0u8; 32];
            tweak_bytes[28..32].copy_from_slice(&self.offset.to_be_bytes());
            let tweak = Scalar::from_be_bytes(tweak_bytes).expect("Valid scalar tweak");
            self.base_secret
                .add_tweak(&tweak)
                .expect("Valid secret key tweak")
        }
    }
}

pub fn to_checksum_address(address: &[u8; 20]) -> String {
    let hex_addr = hex::encode(address);
    let mut hasher = Keccak::v256();
    hasher.update(hex_addr.as_bytes());
    let mut hash = [0u8; 32];
    hasher.finalize(&mut hash);

    let mut checksummed = String::with_capacity(42);
    checksummed.push_str("0x");

    for (i, c) in hex_addr.chars().enumerate() {
        if c.is_ascii_digit() {
            checksummed.push(c);
        } else {
            let hash_byte = hash[i / 2];
            let nibble = if i % 2 == 0 {
                (hash_byte >> 4) & 0x0f
            } else {
                hash_byte & 0x0f
            };

            if nibble >= 8 {
                checksummed.push(c.to_ascii_uppercase());
            } else {
                checksummed.push(c.to_ascii_lowercase());
            }
        }
    }

    checksummed
}

#[inline(always)]
pub fn matches_prefix_case_insensitive(address: &[u8; 20], prefix_hex_lower: &str) -> bool {
    let prefix_bytes = prefix_hex_lower.as_bytes();
    let prefix_len = prefix_bytes.len();
    if prefix_len > 40 {
        return false;
    }

    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    for i in 0..prefix_len {
        let byte_idx = i / 2;
        let nibble = if i % 2 == 0 {
            (address[byte_idx] >> 4) & 0x0f
        } else {
            address[byte_idx] & 0x0f
        };
        let c = HEX_CHARS[nibble as usize];
        if c != prefix_bytes[i] {
            return false;
        }
    }

    true
}

#[inline(always)]
pub fn matches_prefix_case_sensitive(address: &[u8; 20], prefix: &str, prefix_lower: &str) -> bool {
    // Fast path: if lowercase hex does not match prefix_lower, it cannot match checksummed prefix
    if !matches_prefix_case_insensitive(address, prefix_lower) {
        return false;
    }
    let checksummed = to_checksum_address(address);
    checksummed[2..].starts_with(prefix)
}

pub fn format_evm_result(secret_key: SecretKey, address: [u8; 20]) -> EvmResult {
    let address_str = to_checksum_address(&address);
    let secret_bytes = secret_key.secret_bytes();
    let priv_key_str = format!("0x{}", hex::encode(secret_bytes));
    EvmResult {
        address: address_str,
        private_key_hex: priv_key_str,
    }
}

pub fn validate_evm_prefix(prefix: &str) -> Result<String, &'static str> {
    let clean = prefix.strip_prefix("0x").or_else(|| prefix.strip_prefix("0X")).unwrap_or(prefix);
    if clean.is_empty() {
        return Err("Prefix cannot be empty.");
    }
    if clean.len() > 40 {
        return Err("Prefix length cannot exceed 40 characters.");
    }
    if !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("EVM prefix must contain only hexadecimal characters (0-9, a-f, A-F).");
    }
    Ok(clean.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evm_checksum_address() {
        // Known test vector: 0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed
        let hex_bytes = hex::decode("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").unwrap();
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hex_bytes);
        let checksummed = to_checksum_address(&addr);
        assert_eq!(checksummed, "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
    }

    #[test]
    fn test_evm_matching() {
        let mut gen = EvmGenerator::new(10);
        let addr = gen.next_address();
        let sk = gen.current_secret_key();
        let res = format_evm_result(sk, addr);
        assert!(res.address.starts_with("0x"));
        assert_eq!(res.address.len(), 42);
        assert_eq!(res.private_key_hex.len(), 66);

        let prefix_lower = res.address[2..6].to_ascii_lowercase();
        let prefix_exact = &res.address[2..6];
        assert!(matches_prefix_case_insensitive(&addr, &prefix_lower));
        assert!(matches_prefix_case_sensitive(&addr, prefix_exact, &prefix_lower));
    }

    #[test]
    fn test_point_addition_correctness() {
        let mut gen = EvmGenerator::new(50);
        let secp = Secp256k1::new();
        for _ in 0..25 {
            let addr = gen.next_address();
            let sk = gen.current_secret_key();
            let pk = PublicKey::from_secret_key(&secp, &sk);
            let uncompressed = pk.serialize_uncompressed();
            let mut hasher = Keccak::v256();
            hasher.update(&uncompressed[1..65]);
            let mut hash = [0u8; 32];
            hasher.finalize(&mut hash);
            let mut expected = [0u8; 20];
            expected.copy_from_slice(&hash[12..32]);
            assert_eq!(addr, expected, "Point addition address must match exact secret key derivation");
            gen.step();
        }
    }

    #[test]
    fn test_evm_prefix_validation() {
        assert!(validate_evm_prefix("0xdead").is_ok());
        assert!(validate_evm_prefix("BEEF").is_ok());
        assert!(validate_evm_prefix("0x123z").is_err());
        assert!(validate_evm_prefix("").is_err());
    }
}
