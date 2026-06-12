//! headless 世界加载验证器：加载一个世界配置，tick 若干次，打印全量快照。
//!
//! 用法：`cargo run --example inspect -- <world.ron> [ticks]`
//!
//! 它验证「动态加载 world」的端到端能力：解析配置、注册动态组件、spawn 实体、
//! 运行脚本 system、并把结果世界状态以可读形式 dump 出来。

use bevy_vm::{VmWorld, WorldSnapshot};
use std::process::ExitCode;

const DEFAULT_TICKS: usize = 1;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("用法: cargo run --example inspect -- <world.ron> [ticks]");
        return ExitCode::FAILURE;
    };
    let ticks = args
        .next()
        .map_or(DEFAULT_TICKS, |s| s.parse().unwrap_or(DEFAULT_TICKS));

    let mut vm = match VmWorld::load(&path) {
        Ok(vm) => vm,
        Err(error) => {
            eprintln!("加载世界失败: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("== 已加载 {path} ==");
    print_snapshot(&vm.inspect(), "初始状态");

    for tick in 1..=ticks {
        if let Err(error) = vm.tick() {
            eprintln!("第 {tick} 次 tick 失败: {error}");
            return ExitCode::FAILURE;
        }
    }
    print_snapshot(&vm.inspect(), &format!("tick x{ticks} 后"));
    ExitCode::SUCCESS
}

/// 以缩进树形式打印一份世界快照。
fn print_snapshot(snapshot: &WorldSnapshot, title: &str) {
    println!("\n-- {title}（{} 个实体）--", snapshot.entities.len());
    for entity in &snapshot.entities {
        println!("实体 {:?}", entity.entity);
        for (name, value) in &entity.components {
            println!("  {name}: {value:?}");
        }
    }
}
