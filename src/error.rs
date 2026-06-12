//! 世界构建与执行过程中的错误类型。

use thiserror::Error;

/// 构建或运行一个 [`crate::VmWorld`] 时可能发生的失败。
///
/// 每个变体携带足够的上下文，使得单个世界的失败可被上层管理者捕获并丢弃，
/// 而不影响其它独立世界。
#[derive(Debug, Error)]
pub enum VmError {
    /// 读取配置文件或脚本文件失败。
    #[error("读取文件 `{path}` 失败: {reason}")]
    Io {
        /// 尝试读取的文件路径。
        path: String,
        /// 底层 IO 错误描述。
        reason: String,
    },

    /// 配置文本无法被解析为预期的世界描述。
    #[error("解析配置失败: {0}")]
    Parse(String),

    /// 配置引用了一个未在组件注册表中登记的组件名。
    #[error("未知组件类型: {0}")]
    UnknownComponent(String),

    /// 通过反射路径写入组件属性时失败。
    #[error("设置属性 `{path}` 失败: {reason}")]
    SetProperty {
        /// 反射路径，例如 `translation.x`。
        path: String,
        /// 底层反射错误的描述。
        reason: String,
    },

    /// 给定的字符串不是合法的 JSON Pointer（RFC 6901）。
    #[error("非法属性路径 `{path}`: {reason}")]
    InvalidPath {
        /// 原始路径字符串。
        path: String,
        /// 解析失败原因。
        reason: String,
    },

    /// 按路径解析或变更动态属性时失败。
    #[error("属性路径 `{path}` 操作失败: {reason}")]
    PropertyPath {
        /// JSON Pointer 路径。
        path: String,
        /// 底层错误描述。
        reason: String,
    },

    /// 脚本无法编译为合法的 Rhai AST。
    #[error("脚本编译失败: {0}")]
    ScriptCompile(String),

    /// 脚本运行期抛错（含超出操作数上限、宿主函数报错等）。
    #[error("脚本运行失败: {0}")]
    ScriptRuntime(String),
}
