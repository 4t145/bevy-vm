//! Cross-format polyfill: the same world spec written in JSON and RON loads
//! into identical `VmWorld` state.

use bevy_vm::VmWorld;
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const JSON_TEXT: &str = r#"
{
  "components": [
    {"name": "Health", "default": {"value": 100.0}}
  ],
  "entities": [
    {"components": {"Health": {"value": 42.0}}}
  ]
}
"#;

const RON_TEXT: &str = r#"
(
    components: [
        (name: "Health", default: (value: 100.0)),
    ],
    entities: [
        (components: { "Health": (value: 42.0) }),
    ],
)
"#;

#[cfg(feature = "config-json")]
#[test]
fn json_text_loads_world() {
    let mut vm = VmWorld::from_json(JSON_TEXT, spec_root()).expect("JSON config should load");
    let entities = vm.query("Health");
    assert_eq!(entities.len(), 1);
    let value = vm
        .get(entities[0], "Health", "value")
        .expect("Health.value");
    assert_eq!(value.as_f64(), Some(42.0));
}

#[cfg(feature = "config-ron")]
#[test]
fn ron_text_loads_world() {
    let mut vm = VmWorld::from_ron(RON_TEXT, spec_root()).expect("RON config should load");
    let entities = vm.query("Health");
    assert_eq!(entities.len(), 1);
    let value = vm
        .get(entities[0], "Health", "value")
        .expect("Health.value");
    assert_eq!(value.as_f64(), Some(42.0));
}
