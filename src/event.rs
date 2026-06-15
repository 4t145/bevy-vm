//! Event layer — two flavors of channels share one read/emit model.
//!
//! - **Typed events**: Rust types `Message + Serialize + DeserializeOwned`
//!   registered up front via [`crate::VmInstanceBuilder::with_event`]. We do
//!   **not** keep a VM-side buffer — Bevy's own `Messages<T>` resource is the
//!   single source of truth. Each VM keeps a per-channel
//!   [`MessageCursor<T>`]-equivalent; reading scans `Messages<T>` from the
//!   cursor to its tail, serialising each instance to a [`Value`] for the
//!   script. Emitting goes the other way: deserialise a [`Value`] into `T`
//!   and call `world.send_message::<T>(...)`.
//! - **Dynamic events**: declared by world config as `events: [...]`, no Rust
//!   type behind them. Stored as [`Value`]s in a per-VM single buffer with
//!   tick-end clear, supporting same-tick consumption between scripts of one
//!   VM. See "dynamic events" section below.
//!
//! # Why no typed buffer
//!
//! Until single-World, VMs lived in a sandbox World; events had to be copied
//! across the boundary, requiring a type-erased buffer (`Vec<T>` per channel)
//! plus pump_in / pump_out systems to keep both sides in sync. Once the VM
//! shares the host World, every Bevy `Messages<T>` resource is already there,
//! and we can read it directly per-tick. That removes:
//! - the typed double buffer + tick-end swap
//! - `add_vm_event_in/out` pump systems
//! - one full deep-copy per event per VM
//! - `T: Send + 'static` paired with bespoke serialize fn pointers — kept,
//!   just streamlined.
//!
//! # Dynamic events
//!
//! - 脚本 `emit("X", ...)` → 直接写本 VM 的 dynamic buffer
//! - 脚本 `events("X")` → 直接读本 VM 的 dynamic buffer
//! - tick 末 clear——dynamic 事件不跨帧存活
//!
//! 配合 system 拓扑排序，下游 system 在同 tick 能消费上游刚 emit 的事件——
//! Bevy `MessageReader/MessageWriter` 同 schedule 内的语义。

use bevy_ecs::message::{Message, MessageCursor, Messages};
use bevy_ecs::world::World;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use thiserror::Error;

/// Errors raised by event-store operations.
#[derive(Debug, Error)]
pub enum EventError {
    /// Event name is not registered with the [`EventRegistry`].
    #[error("event `{name}` is not registered")]
    UnknownEvent {
        /// Event name as provided by config / script / Bevy bridge.
        name: String,
    },
    /// The named channel is dynamic but a typed-only API was used (or vice
    /// versa).
    #[error("event `{name}` channel kind mismatch (typed vs dynamic)")]
    KindMismatch {
        /// Event name.
        name: String,
    },
    /// Failed to serialize a typed event payload to a [`Value`] for the
    /// script-facing view.
    #[error("failed to serialize event `{name}`: {reason}")]
    Serialize {
        /// Event name.
        name: String,
        /// Underlying serde error message.
        reason: String,
    },
    /// Failed to deserialize a script-supplied [`Value`] payload back into
    /// the typed event when the script `emit`s on a typed channel.
    #[error("failed to deserialize event `{name}`: {reason}")]
    Deserialize {
        /// Event name.
        name: String,
        /// Underlying serde error message.
        reason: String,
    },
    /// Bevy `Messages<T>` resource was not initialised — `with_event::<T>`
    /// is supposed to seed it but the host World lacks it (likely the wrong
    /// World handed to `tick`, or the resource was removed).
    #[error("event `{name}` has no `Messages<T>` resource in the World")]
    MissingResource {
        /// Event name.
        name: String,
    },
}

/// Per-channel, type-erased adapter over Bevy's `Messages<T>`.
///
/// Holds the per-VM read cursor for `T` and the function pointers that read /
/// emit through Bevy's resource. The [`Box<dyn TypedReader>`] kept on each
/// VM is what makes typed channels Send-safe without baking `T` into outer
/// types.
pub trait TypedReader: Send + Any {
    /// Pull all unread `Messages<T>` since this VM's cursor into the
    /// per-channel frame cache. Idempotent within a tick — same-tick callers
    /// see the same set of events without consuming each other's view.
    ///
    /// `frame_cache` is the Vec the [`EventStore`] hands to scripts when they
    /// `events(name)`; it lives one tick (cleared by `end_tick_all`).
    ///
    /// # Errors
    ///
    /// Returns an [`EventError`] if Bevy's `Messages<T>` resource is missing
    /// or serialisation fails.
    fn refresh(
        &mut self,
        world: &World,
        name: &str,
        frame_cache: &mut Vec<Value>,
    ) -> Result<(), EventError>;
    /// Deserialize `value` into `T` and write it onto `Messages<T>`.
    ///
    /// # Errors
    ///
    /// Returns an [`EventError`] when deserialisation fails or Bevy's
    /// `Messages<T>` resource is missing.
    fn emit(&mut self, world: &mut World, name: &str, value: Value) -> Result<(), EventError>;
}

/// Concrete adapter for one Rust message type `T`.
struct CursorReader<T: Message + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static> {
    cursor: MessageCursor<T>,
}

impl<T> TypedReader for CursorReader<T>
where
    T: Message + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    fn refresh(
        &mut self,
        world: &World,
        name: &str,
        frame_cache: &mut Vec<Value>,
    ) -> Result<(), EventError> {
        let Some(messages) = world.get_resource::<Messages<T>>() else {
            return Err(EventError::MissingResource {
                name: name.to_owned(),
            });
        };
        for message in self.cursor.read(messages) {
            let value = serde_json::to_value(message).map_err(|e| EventError::Serialize {
                name: name.to_owned(),
                reason: e.to_string(),
            })?;
            frame_cache.push(value);
        }
        Ok(())
    }

    fn emit(&mut self, world: &mut World, name: &str, value: Value) -> Result<(), EventError> {
        let instance: T = serde_json::from_value(value).map_err(|e| EventError::Deserialize {
            name: name.to_owned(),
            reason: e.to_string(),
        })?;
        let Some(mut messages) = world.get_resource_mut::<Messages<T>>() else {
            return Err(EventError::MissingResource {
                name: name.to_owned(),
            });
        };
        messages.write(instance);
        Ok(())
    }
}

/// Function pointer to seed `Messages<T>` on a fresh World — used by
/// [`crate::VmInstanceBuilder::with_event`] so the host doesn't have to think
/// about Bevy's resource side at all.
type EnsureResourceFn = fn(&mut World);

/// Function pointer to construct a fresh per-VM [`TypedReader`] for `T`,
/// seeding the cursor at the current `Messages<T>` tail so existing
/// (pre-VM-load) messages don't get replayed into a freshly built VM.
type MakeReaderFn = fn(&World) -> Box<dyn TypedReader>;

/// Metadata for a typed event channel — small, all `T` info packed into fn
/// pointers so the registry stays type-erased.
pub struct TypedEvent {
    /// Default payload as a [`Value`], when the registered type implements
    /// [`Default`]. Used as the merge baseline for partial payloads emitted
    /// from scripts. `None` when the type has no `Default`.
    pub default: Option<Value>,
    /// Diagnostic name (the string the event was registered under).
    name: String,
    ensure_resource: EnsureResourceFn,
    make_reader: MakeReaderFn,
}

impl TypedEvent {
    /// Diagnostic name (the string the event was registered under).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Make sure Bevy's `Messages<T>` resource exists on `world` so typed
    /// reads / writes have somewhere to land.
    pub fn ensure_resource(&self, world: &mut World) {
        (self.ensure_resource)(world);
    }

    /// Construct a fresh per-VM reader, with cursor positioned at the
    /// current `Messages<T>` tail so pre-existing messages on the World are
    /// not replayed into the new VM (matches Bevy `MessageReader`'s default
    /// "starts seeing only new messages" behaviour).
    #[must_use]
    pub fn make_reader(&self, world: &World) -> Box<dyn TypedReader> {
        (self.make_reader)(world)
    }
}

/// Metadata for a dynamic event channel.
#[derive(Debug, Clone)]
pub struct DynEvent {
    /// Declared default payload — top-level fields merged into every emit
    /// (matches dynamic-component init semantics).
    pub default: Value,
}

/// What layer an event name belongs to.
pub enum EventKind<'a> {
    /// Typed event: Rust type registered up front.
    Typed(&'a TypedEvent),
    /// Dynamic event: declared by config, payloads are raw [`Value`].
    Dynamic(&'a DynEvent),
}

/// Per-channel single buffer for dynamic events: stores [`Value`] directly。
/// 同帧消费——见模块文档。
#[derive(Debug, Default)]
pub struct DynEventBuffer {
    storage: Vec<Value>,
}

impl DynEventBuffer {
    /// Push a value. Same-tick readers see it.
    pub fn push(&mut self, value: Value) {
        self.storage.push(value);
    }

    /// Drain everything currently in the buffer.
    pub fn drain(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.storage)
    }

    /// Borrow the buffer.
    #[must_use]
    pub fn current(&self) -> &[Value] {
        &self.storage
    }

    /// Clear the buffer at the end of a tick.
    pub fn clear(&mut self) {
        self.storage.clear();
    }
}

/// Registry of every event name a VM can emit or receive.
#[derive(Default)]
pub struct EventRegistry {
    typed: HashMap<String, TypedEvent>,
    dynamic: HashMap<String, DynEvent>,
}

impl EventRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a typed event under `name` for Rust message type `T`.
    ///
    /// `T` does **not** need to implement [`Default`]; payload merging on
    /// script-side emit is skipped for such channels.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::KindMismatch`] when the same name is already
    /// registered (typed or dynamic) — silent re-registration would mask
    /// config bugs.
    pub fn register_typed<T>(&mut self, name: &str) -> Result<(), EventError>
    where
        T: Message + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        self.insert_typed::<T>(name, None)
    }

    /// Like [`Self::register_typed`] but additionally records `T::default()`
    /// as the merge baseline for partial script-emitted payloads.
    ///
    /// # Errors
    ///
    /// Same as [`Self::register_typed`], plus [`EventError::Serialize`] when
    /// `T::default()` cannot be serialized.
    pub fn register_typed_with_default<T>(&mut self, name: &str) -> Result<(), EventError>
    where
        T: Message + Serialize + for<'de> Deserialize<'de> + Default + Send + Sync + 'static,
    {
        let default = serde_json::to_value(T::default()).map_err(|e| EventError::Serialize {
            name: name.to_owned(),
            reason: e.to_string(),
        })?;
        self.insert_typed::<T>(name, Some(default))
    }

    fn insert_typed<T>(&mut self, name: &str, default: Option<Value>) -> Result<(), EventError>
    where
        T: Message + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        if self.typed.contains_key(name) || self.dynamic.contains_key(name) {
            return Err(EventError::KindMismatch {
                name: name.to_owned(),
            });
        }
        self.typed.insert(
            name.to_owned(),
            TypedEvent {
                default,
                name: name.to_owned(),
                ensure_resource: ensure_messages_resource::<T>,
                make_reader: make_typed_reader::<T>,
            },
        );
        Ok(())
    }

    /// Register a dynamic event channel.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::KindMismatch`] if the name collides with an
    /// already-registered event.
    pub fn register_dynamic(&mut self, name: &str, default: Value) -> Result<(), EventError> {
        if self.typed.contains_key(name) || self.dynamic.contains_key(name) {
            return Err(EventError::KindMismatch {
                name: name.to_owned(),
            });
        }
        self.dynamic.insert(name.to_owned(), DynEvent { default });
        Ok(())
    }

    /// Look up an event name; returns `None` when unregistered.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<EventKind<'_>> {
        if let Some(typed) = self.typed.get(name) {
            return Some(EventKind::Typed(typed));
        }
        self.dynamic.get(name).map(EventKind::Dynamic)
    }

    /// Iterate every registered event name.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.typed
            .keys()
            .chain(self.dynamic.keys())
            .map(String::as_str)
    }

    /// Look up a typed event by name; `None` when missing or dynamic.
    #[must_use]
    pub fn typed(&self, name: &str) -> Option<&TypedEvent> {
        self.typed.get(name)
    }

    /// Iterate typed events — used when seeding `Messages<T>` resources on
    /// the host World during VM bootstrap.
    pub fn typed_events(&self) -> impl Iterator<Item = (&str, &TypedEvent)> {
        self.typed.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// Per-VM event store: per-channel cursor + frame cache for typed events,
/// per-channel buffer for dynamic events.
///
/// Why the frame cache: multiple scripts in one VM may all call
/// `events("KeyboardInput")`. Bevy's [`MessageCursor`] is single-consumer —
/// once advanced past a message, that message is gone for that cursor. To
/// keep the "every script sees every event this tick" semantic, we lazily
/// drain the cursor into a per-channel `Vec<Value>` on first `refresh()`
/// per tick; subsequent reads in the same tick re-borrow that cache. The
/// cache clears at tick end.
#[derive(Default)]
pub struct EventStore {
    typed: HashMap<String, TypedChannelState>,
    dynamic: HashMap<String, DynEventBuffer>,
}

struct TypedChannelState {
    reader: Box<dyn TypedReader>,
    /// Snapshot of all events the cursor has seen since the last
    /// [`EventStore::end_tick_all`]. Filled lazily on first read this tick;
    /// re-used by every subsequent read in the same tick.
    frame_cache: Vec<Value>,
    /// Whether `frame_cache` has been populated yet this tick.
    primed: bool,
}

impl EventStore {
    /// Build a store with one reader / buffer per registered channel.
    ///
    /// Each typed-channel reader's cursor is seeded at the **current**
    /// `Messages<T>` tail on `world`, so a freshly built VM does not replay
    /// stale messages that were emitted before it existed. Important for
    /// world-switch scenarios where the host World outlives individual VMs.
    #[must_use]
    pub fn new(world: &World, registry: &EventRegistry) -> Self {
        let mut typed = HashMap::new();
        for (name, event) in &registry.typed {
            typed.insert(
                name.clone(),
                TypedChannelState {
                    reader: event.make_reader(world),
                    frame_cache: Vec::new(),
                    primed: false,
                },
            );
        }
        let mut dynamic = HashMap::new();
        for name in registry.dynamic.keys() {
            dynamic.insert(name.clone(), DynEventBuffer::default());
        }
        Self { typed, dynamic }
    }

    /// Snapshot all unread typed events for `name` into the per-tick frame
    /// cache (idempotent within a tick) and return a borrow of the cache.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::UnknownEvent`] / [`EventError::KindMismatch`] /
    /// [`EventError::MissingResource`] / [`EventError::Serialize`] as fitting.
    pub fn read_typed(&mut self, world: &World, name: &str) -> Result<&[Value], EventError> {
        let Some(state) = self.typed.get_mut(name) else {
            return Err(if self.dynamic.contains_key(name) {
                EventError::KindMismatch {
                    name: name.to_owned(),
                }
            } else {
                EventError::UnknownEvent {
                    name: name.to_owned(),
                }
            });
        };
        if !state.primed {
            state.reader.refresh(world, name, &mut state.frame_cache)?;
            state.primed = true;
        }
        Ok(&state.frame_cache)
    }

    /// Emit a typed event by `Value` payload. Goes through Bevy's
    /// `Messages<T>` directly — VM-side cursors will pick it up next read.
    ///
    /// # Errors
    ///
    /// Same as [`Self::read_typed`].
    pub fn emit_typed(
        &mut self,
        world: &mut World,
        name: &str,
        value: Value,
    ) -> Result<(), EventError> {
        let Some(state) = self.typed.get_mut(name) else {
            return Err(if self.dynamic.contains_key(name) {
                EventError::KindMismatch {
                    name: name.to_owned(),
                }
            } else {
                EventError::UnknownEvent {
                    name: name.to_owned(),
                }
            });
        };
        state.reader.emit(world, name, value)
    }

    /// Push a dynamic event payload onto the dynamic buffer.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::UnknownEvent`] if `name` is not registered, or
    /// [`EventError::KindMismatch`] if the channel is typed.
    pub fn push_dynamic(&mut self, name: &str, value: Value) -> Result<(), EventError> {
        if self.typed.contains_key(name) {
            return Err(EventError::KindMismatch {
                name: name.to_owned(),
            });
        }
        let Some(buffer) = self.dynamic.get_mut(name) else {
            return Err(EventError::UnknownEvent {
                name: name.to_owned(),
            });
        };
        buffer.push(value);
        Ok(())
    }

    /// Borrow the dynamic buffer for `name`, if any.
    #[must_use]
    pub fn dynamic_buffer(&self, name: &str) -> Option<&DynEventBuffer> {
        self.dynamic.get(name)
    }

    /// Drain a dynamic channel's buffer.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::UnknownEvent`] if `name` is unknown, or
    /// [`EventError::KindMismatch`] if the channel is typed.
    pub fn drain_dynamic(&mut self, name: &str) -> Result<Vec<Value>, EventError> {
        if self.typed.contains_key(name) {
            return Err(EventError::KindMismatch {
                name: name.to_owned(),
            });
        }
        let Some(buffer) = self.dynamic.get_mut(name) else {
            return Err(EventError::UnknownEvent {
                name: name.to_owned(),
            });
        };
        Ok(buffer.drain())
    }

    /// Tick-end housekeeping: clear dynamic buffers + reset the per-channel
    /// typed frame cache (cursor stays put so unread events from earlier
    /// frames are still visible next tick — but the cache must drop so we
    /// don't show the same events twice).
    pub fn end_tick_all(&mut self) {
        for buffer in self.dynamic.values_mut() {
            buffer.clear();
        }
        for state in self.typed.values_mut() {
            state.frame_cache.clear();
            state.primed = false;
        }
    }
}

fn ensure_messages_resource<T>(world: &mut World)
where
    T: Message + Send + Sync + 'static,
{
    if !world.contains_resource::<Messages<T>>() {
        world.insert_resource(Messages::<T>::default());
    }
}

fn make_typed_reader<T>(world: &World) -> Box<dyn TypedReader>
where
    T: Message + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    let mut cursor = MessageCursor::<T>::default();
    if let Some(messages) = world.get_resource::<Messages<T>>() {
        // 把 cursor 推到当前末尾——本 VM 不会读到 build 之前已经在 World 里
        // 的历史 message，避免 "stale entity" 类崩溃。
        for _ in cursor.read(messages) {}
    }
    Box::new(CursorReader::<T> { cursor })
}

/// Top-level merge `payload` into `default` (matches typed-component init
/// semantics): only object payloads against object defaults are merged; any
/// other shape returns `payload` unchanged.
#[must_use]
pub fn merge_with_default(payload: Value, default: Option<&Value>) -> Value {
    let Some(default) = default else {
        return payload;
    };
    let Value::Object(payload_map) = payload else {
        return payload;
    };
    let Value::Object(default_map) = default else {
        return Value::Object(payload_map);
    };
    let mut merged = default_map.clone();
    for (key, value) in payload_map {
        merged.insert(key, value);
    }
    Value::Object(merged)
}
