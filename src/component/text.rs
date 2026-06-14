//! 文本渲染相关的强类型组件。
//!
//! 当前提供 [`TextLabel`]，对应 Bevy 的 `Text2d`——必须搭配
//! [`crate::component::camera::Camera2d`] 使用。挂在 VM 实体上，
//! 渲染同步层会 spawn 一个 `Text2d` 镜像实体随 [`crate::component::Position`]
//! 平移。
//!
//! 不依赖 `bevy-bridge` feature 编译；headless 模拟里这个组件只是个数据。

use bevy_color::Color;
use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;

/// 默认字体大小（pt）。Bevy 0.18 默认 24。
const DEFAULT_FONT_SIZE: f32 = 24.0;

/// 一段 2D 文本标签。
///
/// 每帧 [`crate::render`] 同步层会把 `content` / `font_size` / `color`
/// 推到 Bevy 的 `Text2d` 组件，把 `Position` 推到 `Transform.translation`。
/// 旋转 / 缩放当前不暴露——文本通常正向显示。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component, Default)]
pub struct TextLabel {
    /// 文本内容。空串时仍 spawn 实体，但 Bevy 不渲染任何字符。
    pub content: String,
    /// 字号（pt）。
    pub font_size: f32,
    /// 文本颜色。
    pub color: Color,
}

impl Default for TextLabel {
    fn default() -> Self {
        Self {
            content: String::new(),
            font_size: DEFAULT_FONT_SIZE,
            color: Color::WHITE,
        }
    }
}
