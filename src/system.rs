//! 行为单元（system）：每 tick 对世界施加变更的可插拔逻辑。
//!
//! 一个世界可加载多个 system，tick 时按加载顺序依次执行。system 是多态的——
//! 当前只有脚本实现 [`ScriptSystem`]（运行一段 Rhai 脚本），未来可新增其它类型
//! （如预编译 Rust 行为、行为树等），只需实现 [`System`] trait。

pub mod script;

pub use script::ScriptSystem;

use crate::error::VmError;
use bevy_ecs::world::World;

/// 一个可对世界施加每-tick 变更的行为单元。
pub trait System {
    /// 在世界上执行一次该 system。
    ///
    /// # Errors
    ///
    /// 当行为执行失败（如脚本运行期抛错）时返回对应的 [`VmError`]。
    fn run(&self, world: &mut World) -> Result<(), VmError>;
}
