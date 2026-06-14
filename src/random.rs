//! 决定性 RNG 资源。
//!
//! 暴露给脚本的随机函数（`random()` / `random_range(...)` /
//! `random_int(...)`）都从同一个 [`VmRng`] 取——这样"同样配置 + 同样事件
//! 输入序列 → 同样脚本输出"的承诺成立，测试可重现。
//!
//! 选用 [`ChaCha8Rng`]：
//! - 状态小（256 bit）、克隆/序列化容易；
//! - 速度足够（远快于游戏 tick 需求）；
//! - 流可重现，跨平台稳定（`StdRng` 的具体后端不固定，跨 Rust 版本可能变）。
//!
//! 配置层通过 [`WorldConfig::seed`](crate::config::WorldConfig::seed) 指定
//! 种子；缺省时构建期用 OS entropy 现取一次。

use bevy_ecs::resource::Resource;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// VM 的决定性 RNG 资源。挂在 [`bevy_ecs::world::World`] 里，host 函数
/// （脚本侧 `random*` / `set_seed`）通过 `world.resource_mut::<VmRng>()`
/// 取得。
#[derive(Resource, Debug, Clone)]
pub struct VmRng {
    inner: ChaCha8Rng,
}

impl VmRng {
    /// 用确定的整数种子构造一个 RNG。
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        Self {
            inner: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    /// 用 OS entropy 现取一次性种子构造 RNG。
    ///
    /// 各次启动结果不同——不希望测试依赖此分支。
    #[must_use]
    pub fn from_entropy() -> Self {
        Self {
            inner: ChaCha8Rng::from_os_rng(),
        }
    }

    /// 半开区间 `[0.0, 1.0)` 上的均匀 f64。
    pub fn next_f64(&mut self) -> f64 {
        self.inner.random::<f64>()
    }

    /// 半开区间 `[min, max)` 上的均匀 f64。
    ///
    /// 当 `min >= max` 时返回 `min`——脚本 callsite 已用整数边界惯例
    /// 兜底，这里不抛异常。
    pub fn next_f64_range(&mut self, min: f64, max: f64) -> f64 {
        if min >= max {
            return min;
        }
        self.inner.random_range(min..max)
    }

    /// 半开区间 `[low, high)` 上的均匀 i64。
    ///
    /// 当 `low >= high` 时返回 `low`。
    pub fn next_i64_range(&mut self, low: i64, high: i64) -> i64 {
        if low >= high {
            return low;
        }
        self.inner.random_range(low..high)
    }
}
