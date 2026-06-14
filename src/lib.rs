//! bevy-vm: 让 AI 产出的配置 + 行为规则驱动一个运行时可变的独立世界。
//!
//! 一个 [`vm::VmWorld`] = 一个完备、独立的交互世界，承载在一个独立的
//! `bevy_ecs::World` 上。世界之间数据隔离、零共享借用，因此可由上层管理者
//! 直接分发到任务池并行 tick。
//!
//! 单个世界内部由一个**独占解释器**串行驱动：它持有 `&mut World`，按配置
//! spawn 实体、用反射填充组件初值，并逐 tick 执行批量行为规则。

pub mod component;
pub mod config;
pub mod error;
pub mod event;
pub mod plugin;
pub mod plugin_loader;
pub mod random;
#[cfg(feature = "bevy-bridge")]
pub mod render;
pub mod resource;
pub mod system;
pub mod vm;
pub mod world_access;

pub use error::VmError;
pub use vm::{EntitySnapshot, VmWorld, VmWorldBuilder, WorldSnapshot};
