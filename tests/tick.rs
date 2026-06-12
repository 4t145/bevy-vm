//! 端到端冒烟测试：加载世界 -> tick（脚本 system + 静态移动）-> 检视。
//!
//! Position/Velocity 是引擎层类型化组件；Health 是配置自声明的内容层动态组件。
//! 两者经同一套点号路径 API 读取，验证统一值系统。

use bevy_vm::VmWorld;
use std::path::PathBuf;

const TICKS: usize = 3;
const EXPECTED_X: f64 = 3.0;
const EXPECTED_Y: f64 = 6.0;
const EXPECTED_HEALTH: f64 = 7.0;

fn world_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/worlds")
        .join(name)
}

fn number(value: &ron::Value) -> f64 {
    match value {
        ron::Value::Number(n) => n.into_f64(),
        other => panic!("期望数值，得到 {other:?}"),
    }
}

#[test]
fn script_system_and_static_movement_advance_world() {
    let mut vm = VmWorld::load(world_path("movement.ron")).expect("配置应能成功构建世界");

    for _ in 0..TICKS {
        vm.tick().expect("tick 不应失败");
    }

    let entities = vm.query("Health");
    assert_eq!(entities.len(), 1, "世界中应恰好存在一个实体");
    let entity = entities[0];

    let x = vm.get(entity, "Position", "x").expect("应能读 Position.x");
    let y = vm.get(entity, "Position", "y").expect("应能读 Position.y");
    let hp = vm
        .get(entity, "Health", "value")
        .expect("应能读 Health.value");

    assert_eq!(number(&x), EXPECTED_X, "位置 x 应被速度积分 3 次");
    assert_eq!(number(&y), EXPECTED_Y, "位置 y 应被速度积分 3 次");
    assert_eq!(number(&hp), EXPECTED_HEALTH, "生命值应被脚本递减 3 次");
}
