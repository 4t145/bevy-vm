//! Dotted-path navigation: locate, read, insert/modify, and delete fields in a
//! [`serde_json::Value`] tree using `a.b.0`-style paths.
//!
//! A path is a sequence of dot-separated segments. For objects each segment is
//! a string key; for arrays it is a decimal index. The empty path refers to
//! the root value itself. [`ValuePathExt::path_set`] auto-creates missing
//! intermediate object keys with empty-object placeholders before descending
//! further.
//!
//! Capabilities are exposed via the [`ValuePathExt`] extension trait,
//! currently implemented only for [`serde_json::Value`].

#[cfg(test)]
mod tests;

use serde_json::Value;
use thiserror::Error;

/// Errors raised by [`ValuePathExt`] operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PathError {
    /// Object does not contain the given key.
    #[error("key `{key}` not found in object")]
    KeyNotFound {
        /// Missing key.
        key: String,
    },
    /// Array index is past the end of the array.
    #[error("array index {index} out of bounds (length {len})")]
    IndexOutOfBounds {
        /// Requested index.
        index: usize,
        /// Array length at the time of access.
        len: usize,
    },
    /// Tried to descend into a value that is neither an object nor an array.
    #[error("cannot descend into non-container value at segment `{segment}`")]
    NotContainer {
        /// Path segment that triggered the failure.
        segment: String,
    },
    /// Tried to assign past the end of an array (only `array.len` is allowed
    /// as an append position).
    #[error("cannot assign at array index {index}: length is {len}")]
    AssignOutOfBounds {
        /// Requested index.
        index: usize,
        /// Array length at the time of access.
        len: usize,
    },
    /// An array index path segment is not a non-negative integer.
    #[error("array index must be a non-negative integer, got `{segment}`")]
    NonIntegerIndex {
        /// Offending path segment.
        segment: String,
    },
    /// Attempted to remove the root value, which has no parent container.
    #[error("cannot remove the root value")]
    RemoveRoot,
}

/// Extension trait granting [`serde_json::Value`] dotted-path read/write/delete.
pub trait ValuePathExt {
    /// Reads the value at `path` by reference.
    ///
    /// # Errors
    ///
    /// Returns [`PathError`] if any intermediate value is not a container, or
    /// if a key/index is missing.
    fn path_get(&self, path: &str) -> Result<&Value, PathError>;

    /// Writes `value` at `path`, auto-creating any missing intermediate
    /// object keys.
    ///
    /// # Errors
    ///
    /// Returns [`PathError`] if an intermediate value is not a container, or
    /// the terminal segment is invalid for its parent (e.g. out-of-bounds
    /// array index).
    fn path_set(&mut self, path: &str, value: Value) -> Result<(), PathError>;

    /// Removes the value at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::RemoveRoot`] if `path` is empty, or other variants
    /// when the parent container is missing or the key/index does not exist.
    fn path_remove(&mut self, path: &str) -> Result<(), PathError>;
}

impl ValuePathExt for Value {
    fn path_get(&self, path: &str) -> Result<&Value, PathError> {
        let mut current = self;
        for segment in segments(path) {
            current = step(current, segment)?;
        }
        Ok(current)
    }

    fn path_set(&mut self, path: &str, value: Value) -> Result<(), PathError> {
        let parts: Vec<&str> = segments(path).collect();
        let Some((last, parents)) = parts.split_last() else {
            *self = value;
            return Ok(());
        };
        let mut current = self;
        for segment in parents {
            current = step_mut_or_create(current, segment)?;
        }
        assign(current, last, value)
    }

    fn path_remove(&mut self, path: &str) -> Result<(), PathError> {
        let parts: Vec<&str> = segments(path).collect();
        let Some((last, parents)) = parts.split_last() else {
            return Err(PathError::RemoveRoot);
        };
        let mut current = self;
        for segment in parents {
            current = step_mut(current, segment)?;
        }
        delete(current, last)
    }
}

/// Splits `path` into segments; an empty path yields no segments.
fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('.').filter(|segment| !segment.is_empty())
}

/// Read-only descend by one segment.
fn step<'v>(value: &'v Value, segment: &str) -> Result<&'v Value, PathError> {
    match value {
        Value::Object(map) => map.get(segment).ok_or_else(|| PathError::KeyNotFound {
            key: segment.to_owned(),
        }),
        Value::Array(seq) => {
            let index = parse_index(segment)?;
            seq.get(index).ok_or(PathError::IndexOutOfBounds {
                index,
                len: seq.len(),
            })
        }
        _ => Err(PathError::NotContainer {
            segment: segment.to_owned(),
        }),
    }
}

/// Mutable descend by one segment (segment must already exist).
fn step_mut<'v>(value: &'v mut Value, segment: &str) -> Result<&'v mut Value, PathError> {
    match value {
        Value::Object(map) => map.get_mut(segment).ok_or_else(|| PathError::KeyNotFound {
            key: segment.to_owned(),
        }),
        Value::Array(seq) => {
            let index = parse_index(segment)?;
            let len = seq.len();
            seq.get_mut(index)
                .ok_or(PathError::IndexOutOfBounds { index, len })
        }
        _ => Err(PathError::NotContainer {
            segment: segment.to_owned(),
        }),
    }
}

/// Mutable descend by one segment; create an empty object when the key is
/// missing from an object (arrays still require the index to exist).
fn step_mut_or_create<'v>(value: &'v mut Value, segment: &str) -> Result<&'v mut Value, PathError> {
    if let Value::Array(_) = value {
        return step_mut(value, segment);
    }
    let Value::Object(map) = value else {
        return Err(PathError::NotContainer {
            segment: segment.to_owned(),
        });
    };
    Ok(map
        .entry(segment.to_owned())
        .or_insert_with(|| Value::Object(serde_json::Map::new())))
}

/// Assign `value` at the terminal segment within `container`.
fn assign(container: &mut Value, segment: &str, value: Value) -> Result<(), PathError> {
    match container {
        Value::Object(map) => {
            map.insert(segment.to_owned(), value);
            Ok(())
        }
        Value::Array(seq) => {
            let index = parse_index(segment)?;
            if index == seq.len() {
                seq.push(value);
            } else if index < seq.len() {
                seq[index] = value;
            } else {
                return Err(PathError::AssignOutOfBounds {
                    index,
                    len: seq.len(),
                });
            }
            Ok(())
        }
        _ => Err(PathError::NotContainer {
            segment: segment.to_owned(),
        }),
    }
}

/// Delete the terminal segment within `container`.
fn delete(container: &mut Value, segment: &str) -> Result<(), PathError> {
    match container {
        Value::Object(map) => {
            map.remove(segment)
                .map(|_| ())
                .ok_or_else(|| PathError::KeyNotFound {
                    key: segment.to_owned(),
                })
        }
        Value::Array(seq) => {
            let index = parse_index(segment)?;
            let len = seq.len();
            if index >= len {
                return Err(PathError::IndexOutOfBounds { index, len });
            }
            seq.remove(index);
            Ok(())
        }
        _ => Err(PathError::NotContainer {
            segment: segment.to_owned(),
        }),
    }
}

/// Parse a path segment as an array index.
fn parse_index(segment: &str) -> Result<usize, PathError> {
    segment
        .parse::<usize>()
        .map_err(|_| PathError::NonIntegerIndex {
            segment: segment.to_owned(),
        })
}
