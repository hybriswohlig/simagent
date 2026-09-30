//! Full-range u64 values (seeds, keys, hashes) in human-readable formats: exactly 16
//! lowercase hex digits without prefix (ADR-0004 section 3), because JavaScript loses
//! precision above 2^53. Binary formats (postcard) keep the plain u64.
//!
//! Use with `#[serde(with = "crate::hex_u64")]`.

use alloc::format;
use alloc::string::String;

use serde::{Deserialize, Deserializer, Serializer, de::Error};

pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    if serializer.is_human_readable() {
        serializer.serialize_str(&format!("{value:016x}"))
    } else {
        serializer.serialize_u64(*value)
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    if deserializer.is_human_readable() {
        let text = String::deserialize(deserializer)?;
        parse(&text).ok_or_else(|| D::Error::custom("hex_u64: expected 16 lowercase hex digits"))
    } else {
        u64::deserialize(deserializer)
    }
}

/// Exactly 16 lowercase hex digits.
pub fn parse(text: &str) -> Option<u64> {
    let lower_hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
    if text.len() != 16 || !text.bytes().all(lower_hex) {
        return None;
    }
    u64::from_str_radix(text, 16).ok()
}
