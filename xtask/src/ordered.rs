//! JSON that keeps the order of object keys, so that a tool can rewrite a data file and
//! keep its layout (serde_json's `Value` sorts the keys without its `preserve_order`
//! feature, and xtask adds no dependency for it). The parser only finds the tokens; each
//! number and each string is parsed by serde_json, so floats keep every bit. The printer
//! writes serde_json's pretty layout.

use serde_json::Number;

/// A JSON value with its object keys in their order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// An object from its entries in this order.
    pub fn object(entries: Vec<(&str, Json)>) -> Json {
        Json::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    pub fn str(text: impl Into<String>) -> Json {
        Json::String(text.into())
    }

    /// A finite float; panics on NaN or an infinity, which JSON cannot hold.
    pub fn float(value: f64) -> Json {
        Json::Number(Number::from_f64(value).unwrap_or_else(|| panic!("a finite float")))
    }

    pub fn int(value: u64) -> Json {
        Json::Number(Number::from(value))
    }

    /// The value under `key` of an object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Json> {
        match self {
            Json::Object(entries) => entries.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Sets `key` of an object: in place if it is there, else at the end.
    pub fn set(&mut self, key: &str, value: Json) {
        if let Some(slot) = self.get_mut(key) {
            *slot = value;
        } else if let Json::Object(entries) = self {
            entries.push((key.to_owned(), value));
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(number) => number.as_u64(),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Json>> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Parses JSON text, keeping the order of the keys.
    pub fn parse(text: &str) -> Result<Json, String> {
        let mut parser = Parser { text, at: 0 };
        let value = parser.value()?;
        parser.blank();
        if parser.at == text.len() {
            Ok(value)
        } else {
            Err(format!("text after the value at byte {}", parser.at))
        }
    }

    /// The text of the data files: serde_json's pretty layout, a final newline.
    pub fn pretty(&self) -> Result<String, String> {
        let mut out = String::new();
        self.write(&mut out, 0)?;
        out.push('\n');
        Ok(out)
    }

    fn write(&self, out: &mut String, depth: usize) -> Result<(), String> {
        let indent = |out: &mut String, depth: usize| out.push_str(&"  ".repeat(depth));
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Json::Number(number) => {
                out.push_str(&serde_json::to_string(number).map_err(|e| e.to_string())?);
            }
            Json::String(text) => {
                out.push_str(&serde_json::to_string(text).map_err(|e| e.to_string())?);
            }
            Json::Array(items) if items.is_empty() => out.push_str("[]"),
            Json::Array(items) => {
                out.push_str("[\n");
                for (index, item) in items.iter().enumerate() {
                    indent(out, depth + 1);
                    item.write(out, depth + 1)?;
                    out.push_str(if index + 1 < items.len() { ",\n" } else { "\n" });
                }
                indent(out, depth);
                out.push(']');
            }
            Json::Object(entries) if entries.is_empty() => out.push_str("{}"),
            Json::Object(entries) => {
                out.push_str("{\n");
                for (index, (key, value)) in entries.iter().enumerate() {
                    indent(out, depth + 1);
                    out.push_str(&serde_json::to_string(key).map_err(|e| e.to_string())?);
                    out.push_str(": ");
                    value.write(out, depth + 1)?;
                    out.push_str(if index + 1 < entries.len() {
                        ",\n"
                    } else {
                        "\n"
                    });
                }
                indent(out, depth);
                out.push('}');
            }
        }
        Ok(())
    }
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn blank(&mut self) {
        let rest = &self.text[self.at..];
        self.at += rest.len() - rest.trim_start().len();
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        self.blank();
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!(
                "expected {:?} at byte {}",
                char::from(byte),
                self.at
            ))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.blank();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(_) => self.number(),
            None => Err("unexpected end".to_owned()),
        }
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(format!("unexpected text at byte {}", self.at))
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let rest = &self.text[self.at..];
        let len = rest
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
            .unwrap_or(rest.len());
        let number: Number = serde_json::from_str(&rest[..len])
            .map_err(|error| format!("a number at byte {}: {error}", self.at))?;
        self.at += len;
        Ok(Json::Number(number))
    }

    /// A string token from its opening quote to its closing one, read by serde_json.
    fn string(&mut self) -> Result<String, String> {
        let bytes = self.text.as_bytes();
        let start = self.at;
        let mut end = start + 1;
        while end < bytes.len() && bytes[end] != b'"' {
            end += if bytes[end] == b'\\' { 2 } else { 1 };
        }
        if end >= bytes.len() {
            return Err(format!("an unclosed string at byte {start}"));
        }
        self.at = end + 1;
        serde_json::from_str(&self.text[start..=end])
            .map_err(|error| format!("a string at byte {start}: {error}"))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.blank();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.blank();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.at)),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut entries = Vec::new();
        self.blank();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.blank();
            let key = self.string()?;
            self.expect(b':')?;
            entries.push((key, self.value()?));
            self.blank();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Object(entries));
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.at)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file goes through unchanged: keys in their order, floats as written, escapes.
    #[test]
    fn a_data_file_round_trips_byte_for_byte() {
        let text = "{\n  \"b\": 1,\n  \"a\": [\n    0.00005,\n    1.0,\n    -2,\n    \"x \\\"q\\\" \\u00e9\"\n  ],\n  \"c\": {\n    \"z\": null,\n    \"y\": true,\n    \"e\": [],\n    \"o\": {}\n  }\n}\n";
        let json = Json::parse(text).expect("json");
        assert_eq!(json.pretty().expect("text"), text.replace("\\u00e9", "é"));
    }
}
