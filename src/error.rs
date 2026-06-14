//! Error types raised when constructing or running a [`crate::VmWorld`].

use crate::component::RegistryError;
use crate::component::typed::TypedComponentError;
use crate::event::EventError;
use crate::world_access::WorldAccessError;
use thiserror::Error;

/// Failures that can occur while building or running a [`crate::VmWorld`].
///
/// Each variant carries enough context to let the supervisor of multiple
/// independent worlds catch and discard a single failing world without
/// affecting the others.
#[derive(Debug, Error)]
pub enum VmError {
    /// Failed to read a config or script file.
    #[error("failed to read file `{path}`: {reason}")]
    Io {
        /// File path that was attempted.
        path: String,
        /// Underlying I/O error description.
        reason: String,
    },

    /// Failed to parse the config text into a world description.
    #[error("failed to parse config: {0}")]
    Parse(String),

    /// Config referenced a component name that is not registered.
    #[error("unknown component: {0}")]
    UnknownComponent(String),

    /// Failed to register a dynamic component declared by the config.
    #[error("failed to register component: {0}")]
    Registry(#[from] RegistryError),

    /// Failed to populate a typed component during config-driven entity init.
    #[error("failed to initialize component `{component}`: {source}")]
    InitTypedComponent {
        /// Component name as written in the config.
        component: String,
        /// Underlying typed-component bridge error.
        #[source]
        source: TypedComponentError,
    },

    /// Failed to auto-insert a required component declared by `requires(...)`.
    #[error("failed to apply requires for component `{component}`: {source}")]
    InitTypedRequired {
        /// Component whose `requires` triggered the auto-insert.
        component: String,
        /// Underlying world-access error.
        #[source]
        source: WorldAccessError,
    },

    /// Failed to populate a dynamic component during config-driven entity init.
    #[error("failed to initialize component `{component}` field `{field}`: {source}")]
    InitDynamicField {
        /// Component name.
        component: String,
        /// Field path within the component.
        field: String,
        /// Underlying world-access error.
        #[source]
        source: WorldAccessError,
    },

    /// Failed to insert a dynamic component's default instance.
    #[error("failed to insert default for component `{component}`: {source}")]
    InsertDynamicDefault {
        /// Component name.
        component: String,
        /// Underlying world-access error.
        #[source]
        source: WorldAccessError,
    },

    /// Failed to register a typed or dynamic event channel.
    #[error("failed to register event: {0}")]
    Event(#[from] EventError),

    /// Script source could not be compiled to a valid Rhai AST.
    #[error("script compilation failed: {0}")]
    ScriptCompile(String),

    /// Script raised an error at runtime (operation budget exceeded, host
    /// function failure, etc.).
    #[error("script runtime error: {0}")]
    ScriptRuntime(String),

    /// Module loader detected a cycle in dependency graph.
    #[error("module dependency cycle: {chain}")]
    ModuleCycle {
        /// `a -> b -> c -> a`-style chain that closes the cycle.
        chain: String,
    },

    /// Module declared a dependency on a name that no module in the load
    /// graph provides.
    #[error("module `{module}` depends on `{missing}` but no such module was loaded")]
    ModuleMissingDependency {
        /// Module declaring the dependency.
        module: String,
        /// Dependency name that could not be resolved.
        missing: String,
    },

    /// Two modules declare a component / event of the same fully-qualified
    /// name. Always a programmer error — namespacing should make this
    /// impossible in normal use; this variant catches collisions on the
    /// global (host) namespace.
    #[error("module namespace collision: `{name}` declared by both `{first}` and `{second}`")]
    ModuleNameCollision {
        /// Fully qualified `<module>::<short>` (or `<short>` for global) name.
        name: String,
        /// First module that registered the name.
        first: String,
        /// Second module that tried to register the same name.
        second: String,
    },

    /// System `before` / `after` graph contains a cycle—无法 topo 排序。
    #[error("system ordering cycle: {chain}")]
    SystemOrderCycle {
        /// `a -> b -> ... -> a` 形式的闭环描述。
        chain: String,
    },

    /// System `before` / `after` 引用了一个不存在的 set / system 名。
    #[error("system `{system}` references unknown set `{missing}` in {kind}")]
    UnknownSystemRef {
        /// 当前 system 的 plugin::stem 名。
        system: String,
        /// 没有匹配 set 也没有匹配 system 的引用名。
        missing: String,
        /// `"before"` 或 `"after"`——便于诊断。
        kind: &'static str,
    },
}
