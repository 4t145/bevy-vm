//! [`ron::Value`] 与 [`rhai::Dynamic`] 之间的手写双向转换。
//!
//! 不走 serde 往返：`ron::Value` 是带标签的枚举，serde 化后脚本会看到
//! `#{ Number: #{ Float: 3.0 } }` 这种内部表示。手写映射保证脚本侧看到的是
//! 朴素的数字/字符串/数组/对象。

#[cfg(test)]
mod tests;

use rhai::{Dynamic, Map};
use ron::Value;
use ron::value::Number;

/// 把一个 [`ron::Value`] 转成脚本侧的 [`Dynamic`]。
pub fn to_dynamic(value: &Value) -> Dynamic {
    match value {
        Value::Unit | Value::Option(None) => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(*b),
        Value::Char(c) => Dynamic::from(*c),
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Number(number) => number_to_dynamic(*number),
        Value::Option(Some(inner)) => to_dynamic(inner),
        Value::Seq(items) => Dynamic::from_array(items.iter().map(to_dynamic).collect()),
        Value::Map(map) => {
            let entries = map
                .iter()
                .filter_map(|(key, val)| map_key(key).map(|name| (name.into(), to_dynamic(val))));
            Dynamic::from_map(entries.collect::<Map>())
        }
    }
}

/// 把脚本侧的 [`Dynamic`] 转回 [`ron::Value`]。
///
/// # Errors
///
/// 遇到无法表示为 [`ron::Value`] 的脚本值（如自定义宿主类型）时返回描述性错误。
pub fn from_dynamic(value: Dynamic) -> Result<Value, String> {
    if value.is_unit() {
        return Ok(Value::Unit);
    }
    if let Ok(b) = value.as_bool() {
        return Ok(Value::Bool(b));
    }
    if let Ok(i) = value.as_int() {
        return Ok(Value::Number(Number::new(i)));
    }
    if let Ok(f) = value.as_float() {
        return Ok(Value::Number(Number::new(f)));
    }
    if let Ok(c) = value.as_char() {
        return Ok(Value::Char(c));
    }
    if value.is_string() {
        let text = value
            .into_string()
            .map_err(|t| format!("无法读取字符串值: {t}"))?;
        return Ok(Value::String(text));
    }
    if value.is_array() {
        let array = value
            .into_array()
            .map_err(|t| format!("无法读取数组值: {t}"))?;
        let items = array
            .into_iter()
            .map(from_dynamic)
            .collect::<Result<_, _>>()?;
        return Ok(Value::Seq(items));
    }
    if value.is_map() {
        return map_from_dynamic(value);
    }
    Err(format!(
        "脚本值类型 `{}` 无法转为 RON 值",
        value.type_name()
    ))
}

/// 把一个 RON 数值转成脚本侧 [`Dynamic`]（整数走 INT，浮点走 FLOAT）。
fn number_to_dynamic(number: Number) -> Dynamic {
    match number.as_i64() {
        Some(i) => Dynamic::from(i),
        None => Dynamic::from(number.into_f64()),
    }
}

/// 取映射键的字符串形式；非字符串键不可表示，返回 `None`。
fn map_key(key: &Value) -> Option<String> {
    match key {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// 把脚本侧 map 转成 RON 映射。
fn map_from_dynamic(value: Dynamic) -> Result<Value, String> {
    let map = value.cast::<Map>();
    let mut result = ron::Map::new();
    for (key, val) in map {
        result.insert(Value::String(key.into()), from_dynamic(val)?);
    }
    Ok(Value::Map(result))
}
