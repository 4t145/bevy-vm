//! Hand-written bidirectional conversion between [`serde_json::Value`] and
//! [`rhai::Dynamic`].
//!
//! We do not piggy-back on serde round-trips here: the script side wants
//! plain numbers / strings / arrays / maps, and a serde-driven detour would
//! add little value. Numbers go through `i64` first and fall back to `f64`,
//! matching what scripts expect from arithmetic.

#[cfg(test)]
mod tests;

use rhai::{Dynamic, Map};
use serde_json::Value;
use thiserror::Error;

/// Errors raised while converting a [`rhai::Dynamic`] into a [`Value`].
#[derive(Debug, Error)]
pub enum ConvertError {
    /// Failed to extract a string from a `Dynamic` that reported `is_string`.
    #[error("failed to read string value: {type_name}")]
    String {
        /// Reported Rhai type name.
        type_name: String,
    },
    /// Failed to extract an array from a `Dynamic` that reported `is_array`.
    #[error("failed to read array value: {type_name}")]
    Array {
        /// Reported Rhai type name.
        type_name: String,
    },
    /// Script value type has no [`Value`] representation (e.g. custom host
    /// types registered with the engine).
    #[error("script value of type `{type_name}` cannot be converted to a JSON value")]
    Unsupported {
        /// Reported Rhai type name.
        type_name: String,
    },
}

/// Convert a [`Value`] into a script-side [`Dynamic`].
pub fn to_dynamic(value: &Value) -> Dynamic {
    match value {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(*b),
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Number(number) => number_to_dynamic(number),
        Value::Array(items) => Dynamic::from_array(items.iter().map(to_dynamic).collect()),
        Value::Object(map) => {
            let entries = map
                .iter()
                .map(|(key, val)| (key.clone().into(), to_dynamic(val)));
            Dynamic::from_map(entries.collect::<Map>())
        }
    }
}

/// Convert a script-side [`Dynamic`] back into a [`Value`].
///
/// # Errors
///
/// Returns [`ConvertError::Unsupported`] for script values that have no
/// JSON representation (e.g. custom host types).
pub fn from_dynamic(value: Dynamic) -> Result<Value, ConvertError> {
    if value.is_unit() {
        return Ok(Value::Null);
    }
    if let Ok(b) = value.as_bool() {
        return Ok(Value::Bool(b));
    }
    if let Ok(i) = value.as_int() {
        return Ok(Value::Number(i.into()));
    }
    if let Ok(f) = value.as_float() {
        return Ok(serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null));
    }
    if let Ok(c) = value.as_char() {
        return Ok(Value::String(c.to_string()));
    }
    if value.is_string() {
        let text = value.into_string().map_err(|t| ConvertError::String {
            type_name: t.to_owned(),
        })?;
        return Ok(Value::String(text));
    }
    if value.is_array() {
        let array = value.into_array().map_err(|t| ConvertError::Array {
            type_name: t.to_owned(),
        })?;
        let items = array
            .into_iter()
            .map(from_dynamic)
            .collect::<Result<_, _>>()?;
        return Ok(Value::Array(items));
    }
    if value.is_map() {
        return map_from_dynamic(value);
    }
    Err(ConvertError::Unsupported {
        type_name: value.type_name().to_owned(),
    })
}

/// Map a JSON number to script-side [`Dynamic`] (integers go to `INT`, the
/// rest to `FLOAT`).
fn number_to_dynamic(number: &serde_json::Number) -> Dynamic {
    if let Some(i) = number.as_i64() {
        return Dynamic::from(i);
    }
    if let Some(u) = number.as_u64() {
        // Rhai's INT is i64, fit into it where possible; otherwise fall back
        // to f64.
        if let Ok(signed) = i64::try_from(u) {
            return Dynamic::from(signed);
        }
    }
    Dynamic::from(number.as_f64().unwrap_or(0.0))
}

/// Convert a script-side map into a JSON object.
fn map_from_dynamic(value: Dynamic) -> Result<Value, ConvertError> {
    let map = value.cast::<Map>();
    let mut result = serde_json::Map::new();
    for (key, val) in map {
        result.insert(key.into(), from_dynamic(val)?);
    }
    Ok(Value::Object(result))
}
