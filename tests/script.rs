//! 脚本 system 能表达复杂逻辑的测试：条件分支、动态组件读写、实体引用、沙箱。

use bevy_vm::VmInstance;
use std::path::PathBuf;

fn world_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/worlds")
        .join(name)
}

fn number(value: &serde_json::Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("期望数值，得到 {value:?}"))
}

fn string(value: &serde_json::Value) -> String {
    value
        .as_str()
        .unwrap_or_else(|| panic!("期望字符串，得到 {value:?}"))
        .to_owned()
}

#[test]
fn conditional_script_branches_per_entity() {
    let mut world = bevy_ecs::world::World::new();
    let mut vm = VmInstance::load(&mut world, world_path("combat.ron")).expect("配置应能成功构建世界");
    vm.tick(&mut world).expect("tick 不应失败");

    let mut healthy_state = None;
    let mut weak_state = None;
    for entity in vm.query(&mut world, "Health") {
        let hp = number(
            &vm.get(&world, entity, "Health", "value")
                .expect("应能读 Health.value"),
        );
        let state = string(
            &vm.get(&world, entity, "Health", "state")
                .expect("脚本应已写入 state"),
        );
        if hp > 5.0 {
            healthy_state = Some(state);
        } else {
            weak_state = Some(state);
        }
    }

    assert_eq!(
        healthy_state.expect("应存在一个健康实体"),
        "fighting",
        "Health=8（扣血后）的实体应进入 fighting"
    );
    assert_eq!(
        weak_state.expect("应存在一个虚弱实体"),
        "fleeing",
        "Health=4 的实体应进入 fleeing"
    );
}

#[test]
fn entity_reference_via_inventory_ids() {
    let mut world = bevy_ecs::world::World::new();
    let mut vm = VmInstance::load(&mut world, world_path("inventory.ron")).expect("配置应能成功构建世界");
    vm.tick(&mut world).expect("tick 不应失败");

    let owners = vm.query(&mut world, "Inventory");
    assert_eq!(owners.len(), 1, "应只有一个背包持有者");
    let slots = vm
        .get(&world, owners[0], "Inventory", "slots")
        .expect("应能读 Inventory.slots");
    let serde_json::Value::Array(ids) = slots else {
        panic!("slots 应为数组，得到 {slots:?}");
    };
    assert_eq!(ids.len(), 2, "背包应存有两个物品 id");

    // 物品是独立实体：剑应存活、药水（耐久 0）应已被脚本销毁。
    let items = vm.query(&mut world, "Item");
    assert_eq!(items.len(), 1, "耐久耗尽的物品实体应已被 despawn，只剩剑");
    let kind = string(&vm.get(&world, items[0], "Item", "kind").expect("应能读 Item.kind"));
    assert_eq!(kind, "sword", "存活的物品应是剑");
}

#[test]
fn infinite_loop_is_stopped_by_operation_limit() {
    let mut world = bevy_ecs::world::World::new();
    let mut vm = VmInstance::load(&mut world, world_path("runaway.ron")).expect("配置应能成功构建世界");
    let result = vm.tick(&mut world);
    assert!(result.is_err(), "死循环应被操作数上限中断为运行时错误");
}

#[test]
fn compile_error_surfaces_at_construction() {
    let mut world = bevy_ecs::world::World::new();
    let result = VmInstance::load(&mut world, world_path("bad_syntax.ron"));
    assert!(result.is_err(), "非法脚本应在构建期编译失败");
}
