//! Cursor grab control as a [`VmPlugin`].
//!
//! Bevy's `CursorOptions` component governs cursor visibility and grab
//! mode on the primary window. It's state, not events — there is no Bevy
//! "request to grab" event we can simply pass through. So this plugin
//! manufactures one: a VM→host typed event channel that scripts can emit
//! to ask the window for the grab state they want.
//!
//! ```rhai
//! emit("CursorGrab",    #{}); // FPS / mouse-look mode
//! emit("CursorRelease", #{}); // free cursor
//! ```
//!
//! The host-side system reads these on each Bevy `Update` and writes the
//! appropriate `CursorGrabMode` + `visible` to the primary window.
//!
//! Mouse pointer events themselves continue to flow through
//! [`crate::plugin::input::InputPlugin`]; this plugin only governs the
//! grab/visibility state. Add both if you want a full FPS pipeline.
//!
//! Why VM-driven instead of viewer-side keybinding: it's a game decision.
//! A first-person shooter wants grab on launch; a UI sandbox wants
//! release. Pause menus toggle. Putting it on the script side keeps the
//! library opinion-free — the harness just obeys.

use crate::VmWorldBuilder;
use crate::error::VmError;
use crate::plugin::VmPlugin;
use bevy::app::{App, Update};
use bevy::ecs::message::MessageReader;
use bevy::ecs::query::With;
use bevy::ecs::system::Query;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use serde::{Deserialize, Serialize};

/// Channel name for [`CursorGrabRequest`].
pub const CURSOR_GRAB: &str = "CursorGrab";
/// Channel name for [`CursorReleaseRequest`].
pub const CURSOR_RELEASE: &str = "CursorRelease";

/// VM→host event: ask the host to lock + hide the cursor.
///
/// Default-constructable so VM scripts can `emit("CursorGrab", #{})` with
/// an empty Rhai map; the deserialization fills in defaults.
#[derive(Serialize, Deserialize, Clone, Default, bevy::prelude::Message)]
pub struct CursorGrabRequest {
    /// Placeholder — Rhai's empty map `#{}` cannot deserialize a unit
    /// struct. A single defaulted field lets the registration round-trip.
    #[serde(default)]
    _marker: bool,
}

/// VM→host event: ask the host to unlock + show the cursor.
#[derive(Serialize, Deserialize, Clone, Default, bevy::prelude::Message)]
pub struct CursorReleaseRequest {
    /// See [`CursorGrabRequest::_marker`].
    #[serde(default)]
    _marker: bool,
}

/// Bridges VM `CursorGrab` / `CursorRelease` events to the primary
/// window's [`CursorOptions`].
///
/// Add the **same instance** on both sides:
///
/// ```ignore
/// use bevy_vm::{VmWorldBuilder, plugin::{AppVmPluginExt, BuilderVmPluginExt, cursor::CursorPlugin}};
/// # use bevy::app::App;
/// let plugin = CursorPlugin;
/// let vm = VmWorldBuilder::new().add_plugin(&plugin)?.load("world.ron")?;
/// let mut app = App::new();
/// app.add_vm_plugin(&plugin);
/// # Ok::<(), bevy_vm::VmError>(())
/// ```
pub struct CursorPlugin;

impl VmPlugin for CursorPlugin {
    fn build_vm(&self, builder: VmWorldBuilder) -> Result<VmWorldBuilder, VmError> {
        builder
            .with_event_default::<CursorGrabRequest>(CURSOR_GRAB)?
            .with_event_default::<CursorReleaseRequest>(CURSOR_RELEASE)
    }

    fn build_app(&self, app: &mut App) {
        use crate::render::VmEventAppExt;
        app.add_vm_event_out::<CursorGrabRequest>(CURSOR_GRAB)
            .add_vm_event_out::<CursorReleaseRequest>(CURSOR_RELEASE)
            .add_systems(Update, apply_cursor_state);
    }
}

/// Read both VM-driven request channels on each `Update` and reflect them
/// into the primary window's [`CursorOptions`]. Release wins ties — safer
/// to expose the cursor than accidentally trap the user.
fn apply_cursor_state(
    mut grab_requests: MessageReader<CursorGrabRequest>,
    mut release_requests: MessageReader<CursorReleaseRequest>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let release = release_requests.read().next().is_some();
    let grab = grab_requests.read().next().is_some();
    if !release && !grab {
        return;
    }
    let Ok(mut cursor) = windows.single_mut() else {
        return;
    };
    if release {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if grab {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}
