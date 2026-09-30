//! Data IDs (ADR-0004 section 2): `namespace:name`, both parts `[a-z][a-z0-9_]*`, at
//! most 64 bytes. Random keys, hashes, the log and exports use the string or its
//! `str_key`, never the load position.

use alloc::string::String;

/// Longest data ID in bytes, colon included.
pub const MAX_DATA_ID_LEN: usize = 64;

/// A checked data ID such as `core:height`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DataId(String);

/// Why a string is not a data ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataIdError {
    TooLong,
    NoNamespace,
    BadPart,
}

impl DataId {
    pub fn new(text: &str) -> Result<DataId, DataIdError> {
        if text.len() > MAX_DATA_ID_LEN {
            return Err(DataIdError::TooLong);
        }
        let (namespace, name) = text.split_once(':').ok_or(DataIdError::NoNamespace)?;
        if !is_part(namespace) || !is_part(name) {
            return Err(DataIdError::BadPart);
        }
        Ok(DataId(String::from(text)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `[a-z][a-z0-9_]*`
fn is_part(part: &str) -> bool {
    let mut bytes = part.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The 64-bit key of a data ID: `stable_hash64` over its UTF-8 bytes (ADR-0004). The
/// only hash for data IDs; ADR-0001 uses it for random keys.
pub fn str_key(id: &DataId) -> u64 {
    crate::hash::stable_hash64(id.0.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_data_ids() {
        for text in ["core:height", "test:fork_no_density", "a:b", "mod_1:x9_y"] {
            assert_eq!(
                DataId::new(text).map(|id| id.as_str().len()),
                Ok(text.len()),
                "{text}"
            );
        }
        let longest = std::format!("core:{}", "a".repeat(MAX_DATA_ID_LEN - 5));
        assert!(DataId::new(&longest).is_ok());
    }

    #[test]
    fn invalid_data_ids() {
        let too_long = std::format!("core:{}", "a".repeat(MAX_DATA_ID_LEN - 4));
        assert_eq!(DataId::new(&too_long), Err(DataIdError::TooLong));
        assert_eq!(DataId::new("height"), Err(DataIdError::NoNamespace));
        for text in [
            "Core:height",
            "core:Height",
            ":height",
            "core:",
            "1core:height",
            "core:1height",
            "core:height:x",
            "core:hei-ght",
            "core: height",
            "core:höhe",
        ] {
            assert_eq!(DataId::new(text), Err(DataIdError::BadPart), "{text}");
        }
    }

    /// ADR-0004 test 14: values from the reference (PyPI xxhash 4.0.1, libxxhash 0.8.3).
    #[test]
    fn str_key_known_answer() {
        let key = |text| str_key(&DataId::new(text).expect("valid"));
        assert_eq!(key("core:height"), 0x9695_e096_e2bb_afc9);
        assert_eq!(key("test:fork_no_density"), 0xa98f_79d3_19f4_9b1a);
    }

    #[test]
    fn data_ids_sort_by_bytes() {
        let a = DataId::new("core:a_b").expect("valid");
        let b = DataId::new("core:ab").expect("valid");
        assert!(a < b, "'_' (0x5f) sorts before 'b' (0x62)");
    }
}
