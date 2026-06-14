//! 世界配置：AI 产出的世界描述。
//!
//! 配置定义一个世界里有哪些实体、每个实体挂哪些组件、以及组件字段的初值（结构），
//! 并声明一组每 tick 依次执行的 system（行为，见 [`crate::system`]）。
//!
//! system 是多态的：[`SystemConfig`] 当前只有「脚本」一种来源，脚本从独立文件
//! 加载（不内联进配置），未来可扩展更多来源。
//!
//! # Multiple text formats
//!
//! 配置文件可以用多种文本格式书写，[`ConfigFormat`] 枚举它们。每种格式由
//! 自己的 `serde::Deserializer` 直接驱动到 [`WorldConfig`]——schema 一致性
//! 由 derive 保证；不走任何中间动态值类型，避免 enum tag 在 ron::Value
//! 这类不存 enum 的中间表示里丢失的问题。
//!
//! 默认开启的格式由 cargo features 控制：
//! - `config-json`（默认开）— 标准 JSON（`*.json`）。
//! - `config-ron`（默认开）— Rusty Object Notation（`*.ron`）。
//! - `config-yaml`（默认关）— 占位，暂未实装。
//! - `config-toon`（默认关）— 占位，暂未实装。
//!
//! 通过 [`WorldConfig::from_path`] 按扩展名自动分派；测试 / inline 文本可
//! 用 [`WorldConfig::from_text`] + 明确格式，或仍用 [`WorldConfig::from_json`]
//! / [`WorldConfig::from_ron`] 这类单格式入口。

use crate::error::VmError;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// 一个完整世界 / module 的声明式描述。
///
/// 同一份 schema 既描述顶级 `world.ron` 也描述被 `modules:` 引用的子文件——
/// 后者作为 module 加载时，文件名 stem 即 module 名（与 Rust mod 一致）。
/// module 内声明的 components/events 在注册期带前缀变成 `<module>::<short>`，
/// 顶级 world.ron 自身视作"全局空间"——它的 components/events 不带前缀。
#[derive(Debug, Clone, Deserialize)]
pub struct WorldConfig {
    /// 该世界自声明的内容层动态组件。
    ///
    /// 引擎层类型化组件（如 `Position`）无需在此声明，它们内建可用。
    #[serde(default)]
    pub components: Vec<ComponentDecl>,
    /// 该世界自声明的内容层动态事件。
    ///
    /// 引擎层类型化事件由宿主代码经 [`crate::vm::VmWorldBuilder::with_event`]
    /// 注册，无需在此声明。
    #[serde(default)]
    pub events: Vec<EventDecl>,
    /// 世界中初始 spawn 的实体列表。
    #[serde(default)]
    pub entities: Vec<EntityConfig>,
    /// 该世界每 tick 依次执行的 system 列表。
    #[serde(default)]
    pub systems: Vec<SystemConfig>,
    /// 决定性 RNG 种子。`None` 时构建期用 OS entropy 现取一次性种子，
    /// 此时各次启动结果不同；指定整数后同一配置 + 同样事件输入序列可
    /// 重现完全相同的脚本输出，便于测试。
    ///
    /// 仅顶级 world.ron 生效；module 文件中的 seed 字段被忽略——一个世界
    /// 一个种子，避免 module 拼装出歧义。
    #[serde(default)]
    pub seed: Option<u64>,
    /// 引用的 module 文件路径，相对当前文件所在目录解析。
    ///
    /// 加载顺序最终由拓扑排序决定（依赖在前）；本字段只是声明集合。
    /// 路径可省略 `.ron` 扩展——加载器会自动补全。
    #[serde(default)]
    pub modules: Vec<String>,
    /// 本 module / world 显式声明的依赖（module 名）。加载器据此排序。
    ///
    /// 写在依赖里但没在 `modules:` 里出现的 module 视为缺失，加载报错。
    /// 顶级 world.ron 通常不写本字段——`modules:` 列出的就是它要的全部。
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// 一个动态事件的声明：名字 + emit 时缺省字段的默认值模板。
#[derive(Debug, Clone, Deserialize)]
pub struct EventDecl {
    /// 事件名，对脚本 `emit`/`events` 与外部 `send_event_dynamic` 可见。
    pub name: String,
    /// emit 时未指定字段的默认值；省略时为空映射 `{}`。
    #[serde(default = "empty_map")]
    pub default: Value,
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

/// `default` 字段省略时的回退值：一个空 JSON 对象。
fn empty_map() -> Value {
    Value::Object(serde_json::Map::new())
}

/// 一个 system 的声明，描述其行为来源 + 调度顺序。
///
/// 与 Bevy `add_systems(schedule, foo.before(bar).after(baz).in_set(MySet))`
/// 对齐：plugin 名隐式作为 [`SystemSet`] —— `before: ["cell"]` 表示"在
/// cell plugin 的所有 system 之前"；`in_set: ["physics"]` 把当前 system 加入
/// `physics` set。
///
/// 这是一个可扩展的多态枚举：新增 system 类型时在此添加变体，并在
/// [`crate::vm`] 的加载逻辑中处理。
///
/// [`SystemSet`]: https://docs.rs/bevy/0.18/bevy/ecs/schedule/trait.SystemSet.html
#[derive(Debug, Clone, Deserialize)]
pub enum SystemConfig {
    /// 从给定文件加载并运行的 Rhai 脚本。
    ///
    /// 路径相对于世界配置文件所在目录解析。
    Script {
        /// 脚本文件路径（相对配置文件目录）。
        path: String,
        /// 在哪些 system / set 之前运行。元素是 set 名（plugin 名 / 显式 set 名）。
        #[serde(default)]
        before: Vec<String>,
        /// 在哪些 system / set 之后运行。
        #[serde(default)]
        after: Vec<String>,
        /// 加入哪些 set。每个 plugin 的所有 system 隐式加入"plugin 名"这个
        /// set——这条字段用来加显式自定义 set，让别的 plugin `before/after`
        /// 引用更精细的分组。
        #[serde(default)]
        in_set: Vec<String>,
        /// 运行条件：一组 Rhai 表达式，每帧 eval；**全部** true 才跑该 system。
        /// 仿 Bevy `add_systems(...).run_if(condition)` 链——多条件用列表表达
        /// "且"。
        ///
        /// 表达式可调任何 host 函数（`is_paused()`, `query("X").len() > 0` 等）；
        /// 编译期 compile 为 AST 缓存，每帧 eval 而非 parse。
        ///
        /// ```ron
        /// Script(
        ///     path: "enemy.rhai",
        ///     run_if: ["!is_paused()"],
        /// )
        /// ```
        #[serde(default)]
        run_if: Vec<String>,
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

/// Supported text formats for [`WorldConfig`] files.
///
/// Pick automatically by file extension via [`Self::from_extension`], or pass
/// explicitly to [`WorldConfig::from_text`] for inline strings / tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    /// JSON. Always available.
    #[cfg(feature = "config-json")]
    Json,
    /// Rusty Object Notation.
    #[cfg(feature = "config-ron")]
    Ron,
    /// YAML. Placeholder — not yet wired to a parser, attempting to use it
    /// returns [`VmError::Parse`].
    #[cfg(feature = "config-yaml")]
    Yaml,
    /// Token-Oriented Object Notation. Placeholder — same as YAML above.
    #[cfg(feature = "config-toon")]
    Toon,
}

impl ConfigFormat {
    /// Resolve a file extension (no leading dot, case-insensitive) to a
    /// [`ConfigFormat`]. Returns `None` for unknown extensions or when the
    /// matching feature is disabled.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            #[cfg(feature = "config-json")]
            "json" => Some(Self::Json),
            #[cfg(feature = "config-ron")]
            "ron" => Some(Self::Ron),
            #[cfg(feature = "config-yaml")]
            "yaml" | "yml" => Some(Self::Yaml),
            #[cfg(feature = "config-toon")]
            "toon" => Some(Self::Toon),
            _ => None,
        }
    }
}

impl WorldConfig {
    /// Load a config file from disk, picking the parser by file extension.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Io`] when the file cannot be read,
    /// [`VmError::Parse`] when the extension is unrecognized or the body
    /// fails to parse.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, VmError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| VmError::Io {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        let ext = path.extension().and_then(|e| e.to_str()).ok_or_else(|| {
            VmError::Parse(format!(
                "config file `{}` has no extension; cannot infer format",
                path.display()
            ))
        })?;
        let format = ConfigFormat::from_extension(ext).ok_or_else(|| {
            VmError::Parse(format!(
                "unsupported config extension `{ext}`; enable the matching cargo feature"
            ))
        })?;
        Self::from_text(&text, format)
    }

    /// Parse a config from `text` using the explicit `format`.
    ///
    /// Each format runs its own native `serde::Deserializer` directly into
    /// [`WorldConfig`] — going through a shared `serde_json::Value` IR loses
    /// enum-tag information for formats whose dynamic value type does not
    /// model enums (notably `ron::Value`). Schema-level cross-format
    /// consistency comes from [`WorldConfig`]'s serde derive instead.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Parse`] when the text cannot be parsed by the
    /// requested format, or fails to match the [`WorldConfig`] schema.
    #[allow(
        unused_variables,
        reason = "all match arms feature-gated; with no format enabled both args go unused"
    )]
    pub fn from_text(text: &str, format: ConfigFormat) -> Result<Self, VmError> {
        match format {
            #[cfg(feature = "config-json")]
            ConfigFormat::Json => {
                serde_json::from_str(text).map_err(|e| VmError::Parse(e.to_string()))
            }
            #[cfg(feature = "config-ron")]
            ConfigFormat::Ron => ron::from_str(text).map_err(|e| VmError::Parse(e.to_string())),
            #[cfg(feature = "config-yaml")]
            ConfigFormat::Yaml => Err(VmError::Parse(
                "YAML config support is not yet implemented".to_owned(),
            )),
            #[cfg(feature = "config-toon")]
            ConfigFormat::Toon => Err(VmError::Parse(
                "TOON config support is not yet implemented".to_owned(),
            )),
        }
    }

    /// Parse a config from JSON text. Convenience for the default format.
    ///
    /// # Errors
    ///
    /// See [`Self::from_text`].
    #[cfg(feature = "config-json")]
    pub fn from_json(text: &str) -> Result<Self, VmError> {
        Self::from_text(text, ConfigFormat::Json)
    }

    /// Parse a config from RON text. Convenience for the RON format.
    ///
    /// # Errors
    ///
    /// See [`Self::from_text`].
    #[cfg(feature = "config-ron")]
    pub fn from_ron(text: &str) -> Result<Self, VmError> {
        Self::from_text(text, ConfigFormat::Ron)
    }
}
