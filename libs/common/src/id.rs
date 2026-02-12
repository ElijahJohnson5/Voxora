use ulid::Ulid;

/// Generates a new ULID-based ID with the given prefix.
///
/// # Examples
/// ```
/// let id = voxora_common::id::prefixed_ulid("usr");
/// assert!(id.starts_with("usr_"));
/// ```
pub fn prefixed_ulid(prefix: &str) -> String {
    format!("{}_{}", prefix, Ulid::new().to_string())
}

/// Marker trait for types that represent a prefixed ID.
pub trait PrefixedId {
    const PREFIX: &'static str;

    fn generate() -> String {
        prefixed_ulid(Self::PREFIX)
    }
}

/// Well-known ID prefixes.
pub mod prefix {
    pub const USER: &str = "usr";
    pub const SESSION: &str = "ses";
    pub const POD: &str = "pod";
    pub const COMMUNITY: &str = "com";
    pub const CHANNEL: &str = "ch";
    pub const ROLE: &str = "role";
    pub const ATTACHMENT: &str = "att";
    pub const AUDIT: &str = "aud";
    pub const SIA: &str = "sia";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefixed_ulid_format() {
        let id = prefixed_ulid("usr");
        assert!(id.starts_with("usr_"));
        // ULID is 26 chars, plus prefix + underscore
        assert_eq!(id.len(), 4 + 26);
    }

    #[test]
    fn test_uniqueness() {
        let a = prefixed_ulid("usr");
        let b = prefixed_ulid("usr");
        assert_ne!(a, b);
    }
}
