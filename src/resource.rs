//! 资源构建桥：把"配置侧的资源描述"翻译成 Bevy 的 `Handle<R>`。
//!
//! # 模型
//!
//! VM 端组件字段直接持 `ResourceBuilder`（如 [`mesh::MeshBuilder`]、
//! [`material::MaterialBuilder`]），它们是**纯数据**——纯字段、可 serde。
//! 组件可以跨 tick 持有它们，配置文件可以原样写它们。
//!
//! 渲染同步层第一次见到一个 builder 时调用 [`ResourceBuilder::build`]
//! 拿到 Bevy 端的 `Handle<R>`，并在 [`ResourceCache`] 里按 [`CacheKey`]
//! 缓存——同 builder 描述（cache_key 相等）只会 build 一次，后续帧从
//! cache 取 handle。
//!
//! 这等价于 Bevy 用户层的 `let h = server.load(...)`：作者写一份"资源指
//! 令"，运行时获得共享句柄。我们把"两步"压成"一步" + 自动缓存。
//!
//! # 模块划分
//!
//! - `image` / `mesh` / `material` —— 各自的 builder enum，纯数据。
//!   不依赖 `bevy-bridge` feature，便于 headless 配置加载也能解析它们。
//! - 这里（`resource.rs` 顶层）—— `ResourceBuilder` trait + `BuildContext`
//!   + `CacheKey<R>` + `ResourceCache`，依赖 Bevy，gate 在 `bevy-bridge` feature 下。

pub mod image;
pub mod material;
pub mod mesh;

#[cfg(feature = "bevy-bridge")]
mod build;

#[cfg(feature = "bevy-bridge")]
pub use build::{BuildContext, ResourceBuilder, ResourceCache};

use serde::{Deserialize, Serialize};

/// 一个 [`ResourceBuilder`] 的稳定 key——由 `cache_key()` 返回。
/// 同 key 在 [`ResourceCache`] 中复用同一份 Bevy `Handle<R>`。
///
/// `R` 通过 [`PhantomData`] 加在 newtype 上，使得 `CacheKey<Mesh>` 与
/// `CacheKey<Image>` 在类型层面互不兼容——避免按 u64 串错桶。
///
/// `Copy` / `Clone` 等 trait 手实现，避免 `derive` 的 `R: Copy + Clone`
/// 传染——`R` 只是 phantom 类型参数，本身不被存储。
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheKey<R> {
    bits: u64,
    #[serde(skip)]
    _phantom: std::marker::PhantomData<fn() -> R>,
}

impl<R> Clone for CacheKey<R> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<R> Copy for CacheKey<R> {}
impl<R> PartialEq for CacheKey<R> {
    fn eq(&self, other: &Self) -> bool {
        self.bits == other.bits
    }
}
impl<R> Eq for CacheKey<R> {}
impl<R> std::hash::Hash for CacheKey<R> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bits.hash(state);
    }
}

impl<R> CacheKey<R> {
    /// Construct a key from a hash bit pattern.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self {
            bits,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Underlying bit pattern.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.bits
    }
}
