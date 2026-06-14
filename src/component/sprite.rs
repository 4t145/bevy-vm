//! 2D sprite typed component.
//!
//! Mirrors Bevy's [`bevy::sprite::Sprite`] field-for-field, with one
//! VM-friendly substitution: the `image` field is a [`ImageBuilder`]
//! (config-side description) instead of a runtime `Handle<Image>`.

use crate::resource::image::ImageBuilder;
use bevy_color::Color;
use bevy_ecs::component::Component;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;

/// A 2D sprite, rendered under any [`crate::component::camera::Camera2d`].
///
/// 与 Bevy 的 `Sprite` 1:1 对齐，差异只在 `image` 字段——VM 这边不存
/// `Handle<Image>`（Handle 不能 deserialize），而是存一个
/// [`ImageBuilder`] 描述"这张图怎么来"。同步层第一次见到时通过 cache 解析
/// 成 `Handle<Image>`。
///
/// `image: None` + `color` 设为非透明色 → 渲染一个纯色矩形（`custom_size`
/// 决定矩形大小）。Bevy 0.18 的 `Sprite::from_color` 等价物。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component, Default)]
pub struct Sprite2d {
    /// 图像资源描述。`None` 表示纯色 sprite（Bevy 端用 `Image::default()` /
    /// 占位图，颜色由 `color` 决定）。
    pub image: Option<ImageBuilder>,
    /// 颜色染色，与 `image` 相乘。默认白色。
    pub color: Color,
    /// 沿 X 轴翻转。
    pub flip_x: bool,
    /// 沿 Y 轴翻转。
    pub flip_y: bool,
    /// 自定义尺寸（世界单位）。`None` 时使用图像自然尺寸。
    pub custom_size: Option<[f32; 2]>,
}

impl Default for Sprite2d {
    fn default() -> Self {
        Self {
            image: None,
            color: Color::WHITE,
            flip_x: false,
            flip_y: false,
            custom_size: None,
        }
    }
}
