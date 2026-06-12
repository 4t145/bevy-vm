//! RON 值与 Rhai 值双向转换的白箱测试，重点验证 round-trip 与朴素表示。

use super::{from_dynamic, to_dynamic};
use ron::Value;

fn value(text: &str) -> Value {
    ron::from_str(text).expect("测试值应为合法 RON")
}

fn round_trip(text: &str) -> Value {
    from_dynamic(to_dynamic(&value(text))).expect("应能往返转换")
}

#[test]
fn integer_round_trips_as_integer() {
    assert_eq!(round_trip("42"), value("42"));
}

#[test]
fn float_round_trips_as_float() {
    assert_eq!(round_trip("3.5"), value("3.5"));
}

#[test]
fn string_round_trips() {
    assert_eq!(round_trip("\"hello\""), value("\"hello\""));
}

#[test]
fn bool_and_unit_round_trip() {
    assert_eq!(round_trip("true"), value("true"));
    assert_eq!(round_trip("()"), value("()"));
}

#[test]
fn nested_map_and_seq_round_trip() {
    let original = value(r#"(stats: (hp: 10, mp: 5), bag: ["a", "b"])"#);
    let restored = from_dynamic(to_dynamic(&original)).expect("应能往返");
    assert_eq!(restored, original);
}

#[test]
fn number_exposed_as_plain_scalar_not_tagged() {
    // 关键：脚本侧看到的应是朴素数字，而非 ron 的枚举标签表示。
    let dynamic = to_dynamic(&value("7"));
    assert_eq!(dynamic.as_int().expect("应为整数"), 7);
}
