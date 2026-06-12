//! 点号路径导航的白箱测试：读、改、增（含创建中间结构）、删、序列下标。

use super::ValuePathExt;
use ron::Value;

fn value(text: &str) -> Value {
    ron::from_str(text).expect("测试值应为合法 RON")
}

#[test]
fn get_reads_nested_map_and_seq() {
    let root = value(r#"(stats: (hp: 10), bag: ["sword", "shield"])"#);
    assert_eq!(root.path_get("stats.hp").expect("应存在"), &value("10"));
    assert_eq!(root.path_get("bag.0").expect("应存在"), &value("\"sword\""));
}

#[test]
fn get_empty_path_returns_root() {
    let root = value("(hp: 1)");
    assert_eq!(root.path_get("").expect("空路径指向根"), &root);
}

#[test]
fn set_modifies_existing_leaf() {
    let mut root = value("(hp: 10)");
    root.path_set("hp", value("3")).expect("应能改写");
    assert_eq!(root.path_get("hp").expect("应存在"), &value("3"));
}

#[test]
fn set_creates_missing_intermediate_maps() {
    let mut root = value("{}");
    root.path_set("a.b.c", value("42"))
        .expect("应能创建中间结构后写入");
    assert_eq!(root.path_get("a.b.c").expect("应存在"), &value("42"));
}

#[test]
fn set_appends_at_seq_end() {
    let mut root = value(r#"(bag: ["a"])"#);
    root.path_set("bag.1", value("\"b\""))
        .expect("末尾下标应追加");
    assert_eq!(root.path_get("bag.1").expect("应存在"), &value("\"b\""));
}

#[test]
fn remove_deletes_map_key() {
    let mut root = value("(hp: 10, mp: 5)");
    root.path_remove("mp").expect("应能删除");
    assert!(root.path_get("mp").is_err(), "删除后键不应存在");
    assert!(root.path_get("hp").is_ok(), "其余键应保留");
}

#[test]
fn remove_deletes_seq_element() {
    let mut root = value(r#"(bag: ["a", "b", "c"])"#);
    root.path_remove("bag.1").expect("应能删除序列元素");
    assert_eq!(root.path_get("bag.1").expect("应存在"), &value("\"c\""));
}

#[test]
fn step_into_non_container_errors() {
    let root = value("(hp: 10)");
    assert!(root.path_get("hp.x").is_err(), "在标量上下探应报错");
}

#[test]
fn non_numeric_seq_index_errors() {
    let root = value(r#"(bag: ["a"])"#);
    assert!(root.path_get("bag.x").is_err(), "非数字下标应报错");
}
