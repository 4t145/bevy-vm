//! Two-sided plugin abstraction.
//!
//! Some extensions need to wire the **same concept** across both halves of
//! the system: register a typed event channel on the [`VmWorldBuilder`]
//! **and** install a corresponding pump system on the Bevy [`App`]. The
//! input event bridge is the canonical example.
//!
//! [`VmPlugin`] captures both responsibilities behind one trait, with two
//! extension entry points:
//! - [`VmWorldBuilder::add_plugin`](crate::VmWorldBuilder::add_plugin)
//!   invokes [`VmPlugin::build_vm`] before the world is constructed.
//! - [`AppVmPluginExt::add_vm_plugin`] (only present under the
//!   `bevy-bridge` feature) invokes [`VmPlugin::build_app`] when the host
//!   wires its Bevy `App`.
//!
//! The same plugin instance can be used in both places — pass it by
//! reference both times. This makes the contract explicit: a plugin's two
//! halves share whatever runtime state they need (channel names, type
//! parameters) by closing over fields on the plugin struct itself.
//!
//! ```ignore
//! use bevy_vm::{VmWorldBuilder, plugin::input::InputPlugin};
//! # use bevy::app::App;
//! # use bevy_vm::plugin::AppVmPluginExt;
//! let plugin = InputPlugin;
//!
//! let vm = VmWorldBuilder::new()
//!     .add_plugin(&plugin)?
//!     .load("worlds/example.ron")?;
//!
//! let mut app = App::new();
//! app.add_vm_plugin(&plugin);
//! # Ok::<(), bevy_vm::VmError>(())
//! ```

#[cfg(feature = "bevy-bridge")]
pub mod cursor;
#[cfg(feature = "bevy-bridge")]
pub mod input;
#[cfg(feature = "bevy-bridge")]
pub mod picking;

use crate::VmWorldBuilder;
use crate::error::VmError;

#[cfg(feature = "bevy-bridge")]
use bevy::app::App;

/// Bridge a feature across the VM-side builder and the Bevy-side app.
///
/// Implementors typically register a typed event under a specific name on
/// both sides, and install whatever pump system the bridge needs. See
/// [`input::InputPlugin`] for a concrete example.
pub trait VmPlugin {
    /// Apply this plugin's VM-side configuration to `builder`.
    ///
    /// Runs once per [`VmWorldBuilder::add_plugin`] call. Typical work:
    /// register typed event channels via
    /// [`VmWorldBuilder::with_event`](crate::VmWorldBuilder::with_event).
    ///
    /// # Errors
    ///
    /// Returns the underlying [`VmError`] when registration fails — name
    /// clashes are the most common failure.
    fn build_vm(&self, builder: VmWorldBuilder) -> Result<VmWorldBuilder, VmError>;

    /// Apply this plugin's Bevy-side configuration to `app`.
    ///
    /// Runs once per [`AppVmPluginExt::add_vm_plugin`] call. Typical work:
    /// register Bevy events with [`App::add_message`](bevy::app::App::add_message)
    /// and install pump systems via the
    /// [`crate::render::VmEventAppExt::add_vm_event`] helper so they line up
    /// with the same channel names registered on `build_vm`.
    ///
    /// Default implementation does nothing — VM-only plugins (e.g. ones that
    /// just register dynamic events) need not override.
    #[cfg(feature = "bevy-bridge")]
    fn build_app(&self, app: &mut App) {
        let _ = app;
    }
}

/// Extension trait letting [`VmWorldBuilder`] consume a [`VmPlugin`] inline.
pub trait BuilderVmPluginExt: Sized {
    /// Apply the VM-side half of `plugin` to this builder.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`VmError`] from [`VmPlugin::build_vm`].
    fn add_plugin<P: VmPlugin + ?Sized>(self, plugin: &P) -> Result<Self, VmError>;
}

impl BuilderVmPluginExt for VmWorldBuilder {
    fn add_plugin<P: VmPlugin + ?Sized>(self, plugin: &P) -> Result<Self, VmError> {
        plugin.build_vm(self)
    }
}

/// Extension trait letting [`bevy::app::App`] consume a [`VmPlugin`].
///
/// Only available under the `bevy-bridge` feature, since the Bevy half of any
/// plugin lives there.
#[cfg(feature = "bevy-bridge")]
pub trait AppVmPluginExt {
    /// Apply the Bevy-side half of `plugin` to this app.
    fn add_vm_plugin<P: VmPlugin + ?Sized>(&mut self, plugin: &P) -> &mut Self;
}

#[cfg(feature = "bevy-bridge")]
impl AppVmPluginExt for App {
    fn add_vm_plugin<P: VmPlugin + ?Sized>(&mut self, plugin: &P) -> &mut Self {
        plugin.build_app(self);
        self
    }
}
