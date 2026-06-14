//! 相机相关的强类型组件，与 Bevy 0.18 的相机模型忠实对齐。
//!
//! - [`Camera3d`] 走主 3D 渲染管线；投影 (`CameraProjection`) 可透视可正交。
//!   `target`/`up` 由同步层翻译为 `Transform::looking_at` 的旋转。
//! - [`Camera2d`] 走 2D 渲染管线；投影固定为正交，但 [`OrthoScalingMode`]
//!   暴露了 Bevy 的全部六种 [`ScalingMode`](bevy::camera::ScalingMode) variant。
//!   默认 [`OrthoScalingMode::WindowSize`]——1 世界单位 = 1 屏幕像素，与 Bevy
//!   原生 `Camera2d::default()` 一致。
//!
//! 不依赖 `bevy-bridge` feature 编译——纯逻辑层的 typed component，headless
//! 模拟也能挂；真正的渲染同步只在 `bevy-bridge` feature 下挂载（见 [`crate::render`]）。

use bevy_color::Color;
use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;

/// 默认透视相机的视角（度）。Bevy 0.18 默认 π/4 (45°)，这里用 60° 是更
/// 通用的"viewer 友好"取值。
const DEFAULT_FOV_DEGREES: f32 = 60.0;
/// 3D 投影的近裁剪面。
const DEFAULT_NEAR_3D: f32 = 0.1;
/// 3D 投影的远裁剪面。
const DEFAULT_FAR_3D: f32 = 1000.0;
/// 2D 正交投影的近裁剪面。Bevy `OrthographicProjection::default_2d()` 用 -1000。
const DEFAULT_NEAR_2D: f32 = -1000.0;
/// 2D 正交投影的远裁剪面。
const DEFAULT_FAR_2D: f32 = 1000.0;
/// 2D 相机的默认 `transform.z`。Bevy 默认 999.9，让 z=0 的 2D 物体落在视锥内。
const DEFAULT_Z_2D: f32 = 999.9;
/// 默认朝上方向：Y 轴。
const DEFAULT_UP: [f32; 3] = [0.0, 1.0, 0.0];
/// 正交投影 `scale` 默认值（Bevy `OrthographicProjection::default_3d()` 用 1.0）。
const DEFAULT_ORTHO_SCALE: f32 = 1.0;

/// Bevy 主 3D 渲染管线的相机。
///
/// VM 实体上挂这个组件 + `Position` 即可被同步层 spawn 一个 Bevy `Camera3d`
/// 实体。`target` 是世界坐标里的注视点，由同步层与 `Position` 一同算出
/// `Transform::looking_at`，所以**不要**也挂 `Rotation`——会被相机同步覆盖。
///
/// 同一世界里可以有任意多个 `Camera3d` 实体（多视口/小地图等），通过
/// [`Self::order`] 控制 z-order，[`Self::active`] 临时关闭某个相机。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component, Default)]
pub struct Camera3d {
    /// 投影方式（透视 / 正交）。
    pub projection: CameraProjection,
    /// 朝上方向，用作 `lookAt` 的 `up`。默认 `[0, 1, 0]`。
    pub up: [f32; 3],
    /// 注视点（世界坐标）。
    pub target: [f32; 3],
    /// 多相机 z-order，对齐 Bevy 的 `Camera::order`：值越大越后渲染（越靠前）。
    pub order: i32,
    /// 是否启用。`false` 时该相机不参与渲染。
    pub active: bool,
    /// 自定义清屏色；`None` 沿用 Bevy `ClearColorConfig::Default`。
    pub clear_color: Option<Color>,
}

impl Default for Camera3d {
    fn default() -> Self {
        Self {
            projection: CameraProjection::default(),
            up: DEFAULT_UP,
            target: [0.0, 0.0, 0.0],
            order: 0,
            active: true,
            clear_color: None,
        }
    }
}

/// Bevy 2D 渲染管线的相机。投影固定为正交，但 [`OrthoScalingMode`] 完整
/// 暴露 Bevy 的六种缩放模式。
///
/// 默认 `WindowSize`（1 世界单位 = 1 屏幕像素，与 Bevy `Camera2d::default()`
/// 一致）。改成 `FixedVertical { viewport_height: 10 }` 后，世界单位与像素
/// 不再 1:1，作者要相应放大 `Sprite.size` / `TextLabel.font_size`。
///
/// VM 实体上挂这个组件 + `Position` 后，渲染同步层 spawn 一个 Bevy `Camera2d`
/// 实体。`Position.xy` 直接平移 transform，z 由 [`Self::z`] 单独控制（默认
/// 999.9，让 `z=0` 的 2D 物体落在视锥内）。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component, Default)]
pub struct Camera2d {
    /// 缩放策略——决定世界单位与屏幕像素的换算关系。默认 `WindowSize`。
    pub scaling_mode: OrthoScalingMode,
    /// 正交投影的全局缩放因子（乘在 `scaling_mode` 之上）。Bevy 默认 1.0。
    pub scale: f32,
    /// 正交近裁剪面。Bevy `default_2d` 用 -1000。
    pub near: f32,
    /// 正交远裁剪面。
    pub far: f32,
    /// 相机 `transform.z`，独立于 VM 实体 `Position.z`；默认 999.9。
    pub z: f32,
    /// 多相机 z-order。
    pub order: i32,
    /// 是否启用。
    pub active: bool,
    /// 自定义清屏色。
    pub clear_color: Option<Color>,
}
impl Default for Camera2d {
    fn default() -> Self {
        Self {
            scaling_mode: OrthoScalingMode::default(),
            scale: DEFAULT_ORTHO_SCALE,
            near: DEFAULT_NEAR_2D,
            far: DEFAULT_FAR_2D,
            z: DEFAULT_Z_2D,
            order: 0,
            active: true,
            clear_color: None,
        }
    }
}

/// 3D 相机的投影选择。
///
/// reflect 序列化形态：`{"Perspective": {fov_degrees: ..., near: ..., far: ...}}`。
#[derive(Reflect, Debug, Clone)]
#[reflect(Default)]
pub enum CameraProjection {
    /// 透视投影。
    Perspective {
        /// 垂直视场角（度）。
        fov_degrees: f32,
        /// 近裁剪面。
        near: f32,
        /// 远裁剪面。
        far: f32,
    },
    /// 正交投影。`scaling_mode` 决定世界单位与屏幕像素的换算关系——常用作
    /// 等距视角。详见 [`OrthoScalingMode`]。
    Orthographic {
        /// 缩放策略。
        scaling_mode: OrthoScalingMode,
        /// 全局缩放因子。
        scale: f32,
        /// 近裁剪面。
        near: f32,
        /// 远裁剪面。
        far: f32,
    },
}

impl Default for CameraProjection {
    fn default() -> Self {
        Self::Perspective {
            fov_degrees: DEFAULT_FOV_DEGREES,
            near: DEFAULT_NEAR_3D,
            far: DEFAULT_FAR_3D,
        }
    }
}

/// 正交投影的缩放策略，与 Bevy 0.18 的
/// [`bevy::camera::ScalingMode`](https://docs.rs/bevy/0.18.1/bevy/camera/enum.ScalingMode.html)
/// 一一对齐。
///
/// reflect 序列化形态：unit variant 是裸字符串 `"WindowSize"`，
/// struct variant 是 `{"Fixed": {width: ..., height: ...}}`。
///
/// 影响"1 世界单位 ↔ 多少屏幕像素"的换算关系，从而决定 `Sprite.size` /
/// `TextLabel.font_size` 这些字段的"自然取值范围"：
/// - `WindowSize` 下，1 单位 = 1 像素，作者按像素思考即可（典型像素游戏 / UI）。
/// - `FixedVertical { viewport_height: H }` 下，整个屏幕高 = H 世界单位，
///   宽度按窗口纵横比推算；作者按"世界单位"思考。
#[derive(Reflect, Debug, Clone, Copy, Default)]
#[reflect(Default)]
pub enum OrthoScalingMode {
    /// 视口大小匹配屏幕：1 世界单位 = 1 屏幕像素（在 `scale=1` 下）。
    /// 这是 Bevy `Camera2d::default()` 的默认。
    #[default]
    WindowSize,
    /// 固定视图大小：图像会拉伸以填充窗口（不保持纵横比）。
    Fixed {
        /// 视图宽度（世界单位）。
        width: f32,
        /// 视图高度（世界单位）。
        height: f32,
    },
    /// 保持纵横比，且任一轴不小于给定下限。
    AutoMin {
        /// 最小宽度（世界单位）。
        min_width: f32,
        /// 最小高度。
        min_height: f32,
    },
    /// 保持纵横比，且任一轴不大于给定上限。
    AutoMax {
        /// 最大宽度（世界单位）。
        max_width: f32,
        /// 最大高度。
        max_height: f32,
    },
    /// 锁定视图高度；宽度按窗口纵横比推算。
    FixedVertical {
        /// 视图高度（世界单位）。
        viewport_height: f32,
    },
    /// 锁定视图宽度；高度按窗口纵横比推算。
    FixedHorizontal {
        /// 视图宽度（世界单位）。
        viewport_width: f32,
    },
}
