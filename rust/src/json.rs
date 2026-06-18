//! JSON document query interface.
//!
//! Port of TIC-80's `src/ext/json.c` which wraps the jsmn tokenizer.
//!
//! Uses `serde_json::Value` internally (the de-facto standard Rust JSON
//! library).  The API mirrors the original C flat-index style but uses
//! `&Value` references as "parent scopes" instead of numeric token ids.

use serde_json::Value;

// ---------------------------------------------------------------------------
// Parsed JSON document
// ---------------------------------------------------------------------------

/// A parsed JSON document providing DOM-style query access.
///
/// Usage mirrors the original C API:
///
/// ```ignore
/// let doc = JsonDoc::parse(r#"{"x":42,"y":{"z":"hello"}}"#).unwrap();
/// let x = json_int("x", doc.root());           // 42
/// let y = json_object("y", doc.root()).unwrap();
/// let z = json_string("z", y).unwrap();         // "hello"
/// ```
pub struct JsonDoc {
    root: Value,
}

impl JsonDoc {
    /// Parse a JSON string.  Returns `None` on syntax error.
    pub fn parse(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok().map(|root| JsonDoc { root })
    }

    /// Get a reference to the root value.
    pub fn root(&self) -> &Value {
        &self.root
    }

    /// Consume the document and return the root `Value`.
    pub fn into_value(self) -> Value {
        self.root
    }
}

// ---------------------------------------------------------------------------
// Query functions
// ---------------------------------------------------------------------------

/// Get an integer field from `parent`.
///
/// Returns 0 if the key is missing or not a number (matching C's `atoi`
/// fallback, which returns 0 for non-numeric strings).
pub fn json_int(key: &str, parent: &Value) -> i32 {
    parent
        .get(key)
        .and_then(|v| v.as_i64())
        .map(|n| n as i32)
        .unwrap_or(0)
}

/// Get a boolean field from `parent`.
///
/// Returns `false` if the key is missing or not a boolean.
pub fn json_bool(key: &str, parent: &Value) -> bool {
    parent.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Get a string field from `parent`.
///
/// Returns `None` if the key is missing or not a string.
pub fn json_string<'a>(key: &str, parent: &'a Value) -> Option<&'a str> {
    parent.get(key).and_then(|v| v.as_str())
}

/// Get an array field from `parent`.
///
/// Returns `None` if the key is missing or not an array.
pub fn json_array<'a>(key: &str, parent: &'a Value) -> Option<&'a Value> {
    parent.get(key).filter(|v| v.is_array())
}

/// Get the number of elements in an array `Value`.
pub fn json_array_size(arr: &Value) -> usize {
    arr.as_array().map_or(0, |a| a.len())
}

/// Get the `index`-th element of an array `Value`.
///
/// Returns `None` if the index is out of bounds.
pub fn json_array_item<'a>(arr: &'a Value, index: usize) -> Option<&'a Value> {
    arr.as_array().and_then(|a| a.get(index))
}

/// Get an object field from `parent`.
///
/// Returns `None` if the key is missing or not an object.
pub fn json_object<'a>(key: &str, parent: &'a Value) -> Option<&'a Value> {
    parent.get(key).filter(|v| v.is_object())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "name": "TIC-80",
        "version": 42,
        "active": true,
        "nested": {
            "inner": "value",
            "count": 99
        },
        "tags": ["retro", "fantasy", "console"],
        "empty": null
    }"#;

    #[test]
    fn parse_success() {
        let doc = JsonDoc::parse(SAMPLE).expect("should parse valid JSON");
        assert!(doc.root().is_object());
    }

    #[test]
    fn parse_invalid() {
        assert!(JsonDoc::parse("not valid json {").is_none());
        assert!(JsonDoc::parse("").is_none());
    }

    #[test]
    fn int_field() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        let root = doc.root();
        assert_eq!(json_int("version", root), 42);
    }

    #[test]
    fn int_missing_defaults_to_zero() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        assert_eq!(json_int("nonexistent", doc.root()), 0);
    }

    #[test]
    fn bool_field() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        assert!(json_bool("active", doc.root()));
    }

    #[test]
    fn bool_missing_defaults_to_false() {
        let doc = JsonDoc::parse(r#"{}"#).unwrap();
        assert!(!json_bool("missing", doc.root()));
    }

    #[test]
    fn string_field() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        assert_eq!(json_string("name", doc.root()), Some("TIC-80"));
    }

    #[test]
    fn string_missing() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        assert_eq!(json_string("nonexistent", doc.root()), None);
    }

    #[test]
    fn nested_object() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        let nested = json_object("nested", doc.root()).expect("nested should exist");
        assert_eq!(json_string("inner", nested), Some("value"));
        assert_eq!(json_int("count", nested), 99);
    }

    #[test]
    fn object_missing() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        assert!(json_object("missing", doc.root()).is_none());
    }

    #[test]
    fn array_field() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        let arr = json_array("tags", doc.root()).expect("tags should exist");
        assert_eq!(json_array_size(arr), 3);
    }

    #[test]
    fn array_items() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        let arr = json_array("tags", doc.root()).unwrap();

        let item0 = json_array_item(arr, 0).expect("item 0");
        assert_eq!(item0.as_str(), Some("retro"));

        let item1 = json_array_item(arr, 1).expect("item 1");
        assert_eq!(item1.as_str(), Some("fantasy"));

        let item2 = json_array_item(arr, 2).expect("item 2");
        assert_eq!(item2.as_str(), Some("console"));
    }

    #[test]
    fn array_out_of_bounds() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        let arr = json_array("tags", doc.root()).unwrap();
        assert!(json_array_item(arr, 99).is_none());
    }

    #[test]
    fn array_missing() {
        let doc = JsonDoc::parse(SAMPLE).unwrap();
        assert!(json_array("missing", doc.root()).is_none());
    }

    #[test]
    fn array_size_empty() {
        let doc = JsonDoc::parse(r#"{"items":[]}"#).unwrap();
        let arr = json_array("items", doc.root()).unwrap();
        assert_eq!(json_array_size(arr), 0);
    }

    #[test]
    fn not_an_array_returns_none() {
        let doc = JsonDoc::parse(r#"{"x":42}"#).unwrap();
        assert!(json_array("x", doc.root()).is_none());
    }

    #[test]
    fn not_an_object_returns_none() {
        let doc = JsonDoc::parse(r#"{"x":42}"#).unwrap();
        assert!(json_object("x", doc.root()).is_none());
    }

    #[test]
    fn object_in_array() {
        // Simulate: folders[0].name pattern from fs.c
        let doc = JsonDoc::parse(r#"{
            "folders": [
                {"name": "src", "id": 1},
                {"name": "docs", "id": 2}
            ]
        }"#).unwrap();

        let folders = json_array("folders", doc.root()).unwrap();
        assert_eq!(json_array_size(folders), 2);

        let first = json_array_item(folders, 0).unwrap();
        assert_eq!(json_string("name", first), Some("src"));
        assert_eq!(json_int("id", first), 1);

        let second = json_array_item(folders, 1).unwrap();
        assert_eq!(json_string("name", second), Some("docs"));
    }

    #[test]
    fn into_value_roundtrip() {
        let doc = JsonDoc::parse(r#"{"a":1}"#).unwrap();
        let v = doc.into_value();
        assert_eq!(v["a"].as_i64(), Some(1));
    }
}
