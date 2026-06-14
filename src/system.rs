//! 行为单元（system）：每 tick 对世界施加变更的可插拔逻辑。
//!
//! 一个世界可加载多个 system，tick 时按加载顺序依次执行。system 是多态的——
//! 当前只有脚本实现 [`ScriptSystem`]（运行一段 Rhai 脚本），未来可新增其它类型
//! （如预编译 Rust 行为、行为树等），只需实现 [`System`] trait。

pub mod script;

pub use script::ScriptSystem;

use crate::error::VmError;
use crate::event::EventStore;
use crate::random::VmRng;
use bevy_ecs::world::World;
use bevy_time::Time;

/// 暂停标志——`pause()` / `resume()` / `is_paused()` 三个 host fn 通过这个
/// 单字段结构与外部通讯。`#[repr(transparent)]` 保留简单的 PoD 布局。
#[derive(Default, Debug, Clone, Copy)]
pub struct Pause {
    /// 是否暂停。
    pub paused: bool,
}

/// 一次 tick 期间提供给 [`System`] 的全部上下文：World + 事件 buffer + 三件
/// per-instance 资源（RNG、Time、Pause）。
///
/// 之前 RNG / Time / Pause 通过临时挂到 World 资源给 host fn 读——多 VM 共
/// 用 World 时容易踩到彼此或污染 host 端同名资源。改为按引用直接传递，
/// 让 [`crate::system::script::ScriptSystem`] 通过 [`script::Slots`] 把指针
/// 桥给 Rhai host fn。
pub struct TickContext<'a> {
    /// 共享的 Bevy `World`。所有 VM 实例共用，VmTag 区分归属。
    pub world: &'a mut World,
    /// 当前 VM 的事件双缓冲。
    pub events: &'a mut EventStore,
    /// 当前 VM 的决定性 RNG。
    pub rng: &'a mut VmRng,
    /// 当前 VM 的本地虚拟时钟。
    pub time: &'a mut Time<()>,
    /// 当前 VM 的暂停标志——脚本调 `pause()` / `resume()` 写它。
    pub pause: &'a mut Pause,
}

/// 一个可对世界施加每-tick 变更的行为单元。
pub trait System {
    /// 在给定上下文上执行一次该 system。
    ///
    /// 实现读写 [`TickContext::world`] 实施 ECS 变更，emit/读 events 走
    /// [`TickContext::events`]，决定性随机走 `rng`，时钟走 `time`，
    /// 暂停标志走 `pause`。tick 末由 [`crate::VmInstance`] 统一 swap event
    /// 缓冲，单个 system 不要自行 swap。
    ///
    /// # Errors
    ///
    /// 当行为执行失败（如脚本运行期抛错）时返回对应的 [`VmError`]。
    fn run(&self, ctx: &mut TickContext<'_>) -> Result<(), VmError>;
}
