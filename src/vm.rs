//! VM 子系统：解释器实例 + 身份 / 标签 + Bevy `App` 接入插件。
//!
//! 公开内容（`crate::` 顶层 re-export 见 `lib.rs`）：
//! - [`instance::VmInstance`] / [`instance::VmInstanceBuilder`] /
//!   [`instance::VmRegistry`] —— 解释器与多实例容器。
//! - [`id::VmId`] / [`id::VmTag`] —— 实例身份与"实体归属"组件。
//! - [`plugin::VmAppPlugin`] / [`plugin::insert_vm_instance`] /
//!   [`plugin::despawn_tagged_entities`] / [`plugin::VmEventAppExt`] /
//!   [`plugin::VmTickSet`] —— 单 World 模式下把 VM 接入 Bevy `App` 的桥。

pub mod id;
pub mod instance;
#[cfg(feature = "bevy-bridge")]
pub mod plugin;

pub use id::{VmId, VmTag};
pub use instance::{VmInstance, VmInstanceBuilder, VmRegistry};

#[cfg(feature = "bevy-bridge")]
pub use plugin::{VmAppPlugin, VmTickSet, despawn_tagged_entities, insert_vm_instance};
