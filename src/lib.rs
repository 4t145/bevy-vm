//! bevy-vm: 让 AI 产出的配置 + 行为规则驱动一个运行时可变的世界——但所有
//! 实体直接住在主 Bevy `World` 里，没有第二个 World、没有桥接。
//!
//! 一个 [`VmInstance`] = 一个独立的脚本调度单元 + 注册表 + 事件存储。它每
//! tick 接收外部 `&mut World`，把所有 spawn 出来的实体打上 [`VmTag`]，并按
//! 配置在 plugins 里挂的脚本顺序逐 tick 执行。
//!
//! 同一个 `World` 可以同时容纳多个 [`VmInstance`]——它们由 [`VmTag`] 隔离，
//! 互相看不到对方的实体。`VmRegistry` 容器集中管理活跃实例。

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
pub mod vm_id;
pub mod world_access;

pub use error::VmError;
pub use vm::{VmInstance, VmInstanceBuilder, VmRegistry};
pub use vm_id::{VmId, VmTag};
