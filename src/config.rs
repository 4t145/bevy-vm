//! 世界配置：AI 产出的世界描述。
//!
//! 配置定义一个世界里有哪些实体、每个实体挂哪些组件、以及组件字段的初值（结构），
//! 并声明一组每 tick 依次执行的 system（行为，见 [`crate::system`]）。
//!
//! system 是多态的：[`SystemConfig`] 当前只有「脚本」一种来源，脚本从独立文件
//! 加载（不内联进配置），未来可扩展更多来源。
//!
//! 当前采用 RON 文本格式，便于手写与人工校对；接 AI 生成时格式不变。

use crate::error::VmError;
use ron::Value;
use serde::Deserialize;
use std::collections::HashMap;

/// 一个完整世界的声明式描述。
#[derive(Debug, Clone, Deserialize)]
pub struct WorldConfig {
    /// 该世界自声明的内容层动态组件。
    ///
    /// 引擎层类型化组件（如 `Position`）无需在此声明，它们内建可用。
    #[serde(default)]
    pub components: Vec<ComponentDecl>,
    /// 世界中初始 spawn 的实体列表。
    pub entities: Vec<EntityConfig>,
    /// 该世界每 tick 依次执行的 system 列表。
    #[serde(default)]
    pub systems: Vec<SystemConfig>,
}

/// 一个动态组件的声明：名字 + spawn 时的默认值模板。
#[derive(Debug, Clone, Deserialize)]
pub struct ComponentDecl {
    /// 组件名，对实体配置与脚本可见。
    pub name: String,
    /// spawn 实体时写入的默认值；实体配置可对其做局部覆盖。
    ///
    /// 省略时默认为空映射 `{}`，便于声明「键全靠运行时长出」的组件。
    #[serde(default = "empty_map")]
    pub default: Value,
}

/// `default` 字段省略时的回退值：一个空 RON 映射。
fn empty_map() -> Value {
    Value::Map(ron::Map::new())
}

/// 一个 system 的声明，描述其行为来源。
///
/// 这是一个可扩展的多态枚举：新增 system 类型时在此添加变体，并在
/// [`crate::vm`] 的加载逻辑中处理。
#[derive(Debug, Clone, Deserialize)]
pub enum SystemConfig {
    /// 从给定文件加载并运行的 Rhai 脚本。
    ///
    /// 路径相对于世界配置文件所在目录解析。
    Script {
        /// 脚本文件路径（相对配置文件目录）。
        path: String,
    },
}

/// 单个实体的描述：它挂载哪些组件，以及各组件的初值。
#[derive(Debug, Clone, Deserialize)]
pub struct EntityConfig {
    /// 该实体挂载的组件，键为组件名，值为该组件的初值。
    ///
    /// 对引擎层类型化组件，初值按反射路径写入（如 `{ "x": 1.0 }`）；对内容层
    /// 动态组件，初值是任意 JSON，与声明的默认值做深合并。
    pub components: HashMap<String, Value>,
}

impl WorldConfig {
    /// 从 RON 文本解析世界配置。
    ///
    /// # Errors
    ///
    /// 当文本不是合法 RON 或不匹配 [`WorldConfig`] 结构时，返回
    /// [`VmError::Parse`]。
    pub fn from_ron(text: &str) -> Result<Self, VmError> {
        ron::from_str(text).map_err(|e| VmError::Parse(e.to_string()))
    }
}
