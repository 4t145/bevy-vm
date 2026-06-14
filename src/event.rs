//! Event layer — two flavors of channels share one buffering model.
//!
//! - **Typed events**: Rust types `#[derive(Serialize, Deserialize)]` registered
//!   at build time via [`crate::vm::VmWorldBuilder::with_event`]. The buffer
//!   is a type-erased `Vec<T>` (see [`AnyVec`]); pump_in / pump_out / external
//!   `send_event` / `drain_events` pass values **by ownership**, paying zero
//!   serialization on the Bevy ↔ VM hot path. Only when scripts read the
//!   channel via `events(name)` do we serialize each `T` into a JSON value
//!   on the fly.
//! - **Dynamic events**: declared by the world config as `events: [...]`,
//!   stored as [`serde_json::Value`] — AI-authored worlds can invent their
//!   own event names without recompiling.
//!
//! # Buffering（typed 双缓冲、dynamic 单缓冲）
//!
//! 两种事件分别用不同模型：
//!
//! ## typed events：strict double buffer
//!
//! - host `send_event` / pump_in → 写 back
//! - 脚本 `events("X")` → 读 front
//! - tick 末 swap：`front <- back, back.clear()`
//! - host pump_out / `drain_events` 在 tick 之后立即拿走 front 的事件
//!
//! 这条路径是 Bevy ↔ VM 的 hot path：让 host 端的事件和脚本生命周期解耦，
//! pump_in 在 tick 前一刻 push、pump_out 在 tick 后一刻 drain，pipeline 稳定。
//! 代价：host 推的事件**延迟 1 tick** 才被脚本看到——但跨进程边界，这点
//! 延迟无法避免。
//!
//! ## dynamic events：single buffer + same-tick consumption
//!
//! - 脚本 `emit("X", ...)` → 直接写 buffer
//! - 脚本 `events("X")` → 直接读 buffer
//! - tick 末 clear——dynamic 事件不跨帧存活
//!
//! 配合 plugin 拓扑排序，下游 plugin 在同 tick 能消费上游刚 emit 的事件——
//! Bevy `MessageReader/MessageWriter` 同 schedule 内的语义。**事件链零延迟**，
//! 多 plugin 的扫雷点击不再积累 N×2 帧延迟。
//!
//! ## 自我消费
//!
//! 同一 ScriptSystem 在 emit 后再 `events()` **会读到自己刚 emit 的**——
//! 单 buffer 的代价。按 Bevy 习惯，一个通道只在一处消费——作者自觉。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::{Any, TypeId};
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
    /// `send_event::<T>` / `drain_events::<T>` was called with `T` that does
    /// not match the type the event was registered with — or a typed-only
    /// API was called on a dynamic channel.
    #[error("event `{name}` was registered with a different Rust type")]
    TypeMismatch {
        /// Event name.
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
}

/// A type-erased `Vec<T>`: the storage trait used by typed channels.
///
/// Lets [`TypedEventBuffer`] hold `Box<Vec<T>>` without naming `T` in the
/// outer struct. Concrete `T` is recovered through [`Self::as_any_mut`] +
/// `downcast_mut::<Vec<T>>()` when push/drain need it.
pub trait AnyVec: Any + Send {
    /// Borrow the inner vec as `&dyn Any` so callers can downcast to `Vec<T>`.
    fn as_any(&self) -> &dyn Any;
    /// Mutable variant of [`Self::as_any`].
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Empty the vec without knowing `T` — used by tick-end buffer swap.
    fn clear(&mut self);
    /// Length of the underlying vec.
    fn len(&self) -> usize;
    /// Whether the underlying vec has zero elements.
    fn is_empty(&self) -> bool;
}

impl<T: Send + 'static> AnyVec for Vec<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn clear(&mut self) {
        Vec::clear(self);
    }
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn is_empty(&self) -> bool {
        Vec::is_empty(self)
    }
}

/// Per-channel **double** buffer for typed events: 跨 host/VM 边界的 channel
/// 仍走 strict double buffer，让 host pump_in / pump_out 与脚本生命周期解耦。
///
/// - host send / pump_in → 写 back
/// - tick 末 swap：front ← back, back.clear()
/// - 脚本 events() 读 front
/// - host drain / pump_out 读 front
///
/// 这是 Bevy ↔ VM 的 hot path，零序列化（[`AnyVec`]）；为了让 pump_out 在
/// tick 后能稳定 drain 到本帧 emit 的内容，typed 通道**不**走"同帧消费"。
pub struct TypedEventBuffer {
    front: Box<dyn AnyVec>,
    back: Box<dyn AnyVec>,
    type_id: TypeId,
}

impl TypedEventBuffer {
    /// Construct buffers for type `T` (both empty).
    #[must_use]
    pub fn new<T: Send + 'static>() -> Self {
        Self {
            front: Box::new(Vec::<T>::new()),
            back: Box::new(Vec::<T>::new()),
            type_id: TypeId::of::<T>(),
        }
    }

    fn check_type<T: 'static>(&self, name: &str) -> Result<(), EventError> {
        if TypeId::of::<T>() == self.type_id {
            Ok(())
        } else {
            Err(EventError::TypeMismatch {
                name: name.to_owned(),
            })
        }
    }

    /// Push one event onto the back buffer (visible next tick).
    ///
    /// # Errors
    ///
    /// Returns [`EventError::TypeMismatch`] if `T` differs from the type
    /// the channel was registered with.
    pub fn push<T: Send + 'static>(&mut self, name: &str, event: T) -> Result<(), EventError> {
        self.check_type::<T>(name)?;
        let vec = self
            .back
            .as_any_mut()
            .downcast_mut::<Vec<T>>()
            .expect("type checked above");
        vec.push(event);
        Ok(())
    }

    /// Drain the front buffer.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::TypeMismatch`] if `T` differs from the registered
    /// type.
    pub fn drain<T: Send + 'static>(&mut self, name: &str) -> Result<Vec<T>, EventError> {
        self.check_type::<T>(name)?;
        let vec = self
            .front
            .as_any_mut()
            .downcast_mut::<Vec<T>>()
            .expect("type checked above");
        Ok(std::mem::take(vec))
    }

    /// Borrow the front buffer for read-only inspection.
    fn front_any(&self) -> &dyn AnyVec {
        &*self.front
    }

    /// Number of events currently readable.
    #[must_use]
    pub fn front_len(&self) -> usize {
        self.front.len()
    }

    /// Swap front/back and clear new back (called at tick end).
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.front, &mut self.back);
        self.back.clear();
    }
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

/// Per-channel storage tagged by layer.
pub enum ChannelStorage {
    /// Typed channel — a [`TypedEventBuffer`] holds `Vec<T>` directly.
    Typed(TypedEventBuffer),
    /// Dynamic channel — a [`DynEventBuffer`] holds [`Value`] payloads.
    Dynamic(DynEventBuffer),
}

impl ChannelStorage {
    /// Tick-end housekeeping: typed swap (front ← back, back clear) for the
    /// host bridge path; dynamic clear (脚本内部同帧消费完毕，丢掉旧的)。
    fn end_tick(&mut self) {
        match self {
            Self::Typed(b) => b.swap(),
            Self::Dynamic(b) => b.clear(),
        }
    }
}

/// What layer an event name belongs to.
pub enum EventKind<'a> {
    /// Typed event: Rust type registered up front.
    Typed(&'a TypedEvent),
    /// Dynamic event: declared by config, payloads are raw [`Value`].
    Dynamic(&'a DynEvent),
}

/// Type-erased boxed `T` produced by deserializing a script payload.
type BoxedAny = Box<dyn Any + Send>;
/// Function pointer: deserialize a [`Value`] into the registered `T`,
/// returning a boxed instance ready for [`PushBoxedFn`].
type DeserializeIntoFn = fn(Value) -> Result<BoxedAny, EventError>;
/// Function pointer: push a previously-boxed `T` onto a typed channel's
/// back buffer.
type PushBoxedFn = fn(&mut TypedEventBuffer, &str, BoxedAny) -> Result<(), EventError>;
/// Function pointer: serialize one front-buffer event at the given index
/// into a [`Value`] (used when scripts read `events(name)`).
type SerializeAtFn = fn(&dyn AnyVec, usize) -> Result<Value, EventError>;

/// Metadata for a typed event channel.
pub struct TypedEvent {
    /// Rust type id, used to reject `send_event::<U>` when `U != T`.
    pub type_id: TypeId,
    /// Default payload as a [`Value`], when the registered type implements
    /// [`Default`]. Used as the merge baseline for partial payloads emitted
    /// from scripts. `None` when the type has no `Default`.
    pub default: Option<Value>,
    /// Construct a fresh `TypedEventBuffer` for the registered `T` — used
    /// by [`EventStore::new`] without naming `T` at the call site.
    make_buffer: fn() -> TypedEventBuffer,
    /// Validate-and-deserialize a script-supplied [`Value`] payload into a
    /// boxed `T`. Used when a script `emit`s on a typed channel.
    deserialize_into: DeserializeIntoFn,
    /// Push a previously-deserialized boxed `T` onto a typed channel's back
    /// buffer. Pairs with [`Self::deserialize_into`].
    push_boxed: PushBoxedFn,
    /// Serialize a single event from the front buffer into a [`Value`] (used
    /// when scripts read `events(name)`). Index is into the front buffer.
    serialize_at: SerializeAtFn,
    /// Diagnostic name (the string the event was registered under).
    name: String,
}

impl TypedEvent {
    /// Diagnostic name (the string the event was registered under).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Deserialize `value` into the registered Rust type and push it onto
    /// the channel's back buffer.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Deserialize`] if `value` is not shaped like
    /// the registered type, or [`EventError::KindMismatch`] if `buffer` is
    /// not the typed buffer for this channel.
    pub fn emit_from_value(
        &self,
        buffer: &mut TypedEventBuffer,
        name: &str,
        value: Value,
    ) -> Result<(), EventError> {
        let boxed = (self.deserialize_into)(value)?;
        (self.push_boxed)(buffer, name, boxed)
    }

    /// Serialize the front-buffer event at `index` into a [`Value`].
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Serialize`] on serialization failure (rare for
    /// normal struct shapes).
    pub fn serialize_front_at(
        &self,
        buffer: &TypedEventBuffer,
        index: usize,
    ) -> Result<Value, EventError> {
        (self.serialize_at)(buffer.front_any(), index)
    }
}

/// Metadata for a dynamic event channel.
#[derive(Debug, Clone)]
pub struct DynEvent {
    /// Declared default payload — top-level fields merged into every emit
    /// (matches dynamic-component init semantics).
    pub default: Value,
}

/// Registry of every event name a [`crate::VmWorld`] can emit or receive.
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

    /// Register a typed event under `name` for Rust type `T`.
    ///
    /// `T` does **not** need to implement [`Default`]; payload merging on
    /// script-side emit is skipped for such channels.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::TypeMismatch`] when the same name is already
    /// registered (typed or dynamic) — silent re-registration would mask
    /// config bugs.
    pub fn register_typed<T>(&mut self, name: &str) -> Result<(), EventError>
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + 'static,
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
        T: Serialize + for<'de> Deserialize<'de> + Default + Send + 'static,
    {
        let default = serialize_typed::<T>(&T::default(), name)?;
        self.insert_typed::<T>(name, Some(default))
    }

    fn insert_typed<T>(&mut self, name: &str, default: Option<Value>) -> Result<(), EventError>
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    {
        if self.typed.contains_key(name) || self.dynamic.contains_key(name) {
            return Err(EventError::TypeMismatch {
                name: name.to_owned(),
            });
        }
        self.typed.insert(
            name.to_owned(),
            TypedEvent {
                type_id: TypeId::of::<T>(),
                default,
                make_buffer: TypedEventBuffer::new::<T>,
                deserialize_into: deserialize_into_boxed::<T>,
                push_boxed: push_boxed::<T>,
                serialize_at: serialize_front_index::<T>,
                name: name.to_owned(),
            },
        );
        Ok(())
    }

    /// Register a dynamic event channel.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::TypeMismatch`] if the name collides with an
    /// already-registered event.
    pub fn register_dynamic(&mut self, name: &str, default: Value) -> Result<(), EventError> {
        if self.typed.contains_key(name) || self.dynamic.contains_key(name) {
            return Err(EventError::TypeMismatch {
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
}

/// Per-world event store: every channel's double buffer keyed by name.
#[derive(Default)]
pub struct EventStore {
    channels: HashMap<String, ChannelStorage>,
}

impl EventStore {
    /// Build a store with one storage per registered channel.
    #[must_use]
    pub fn new(registry: &EventRegistry) -> Self {
        let mut channels = HashMap::new();
        for (name, typed) in &registry.typed {
            channels.insert(name.clone(), ChannelStorage::Typed((typed.make_buffer)()));
        }
        for name in registry.dynamic.keys() {
            channels.insert(
                name.clone(),
                ChannelStorage::Dynamic(DynEventBuffer::default()),
            );
        }
        Self { channels }
    }

    /// Return a reference to the storage for `name`.
    #[must_use]
    pub fn storage(&self, name: &str) -> Option<&ChannelStorage> {
        self.channels.get(name)
    }

    /// Mutable variant of [`Self::storage`].
    pub fn storage_mut(&mut self, name: &str) -> Option<&mut ChannelStorage> {
        self.channels.get_mut(name)
    }

    /// Push a typed event by value, no serialization.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::UnknownEvent`] if `name` is not registered,
    /// [`EventError::KindMismatch`] if the channel is dynamic, or
    /// [`EventError::TypeMismatch`] if `T` differs from the registered type.
    pub fn push_typed<T: Send + 'static>(
        &mut self,
        name: &str,
        event: T,
    ) -> Result<(), EventError> {
        match self.channels.get_mut(name) {
            None => Err(EventError::UnknownEvent {
                name: name.to_owned(),
            }),
            Some(ChannelStorage::Dynamic(_)) => Err(EventError::KindMismatch {
                name: name.to_owned(),
            }),
            Some(ChannelStorage::Typed(buffer)) => buffer.push(name, event),
        }
    }

    /// Drain a typed channel's front buffer.
    ///
    /// # Errors
    ///
    /// Same as [`Self::push_typed`].
    pub fn drain_typed<T: Send + 'static>(&mut self, name: &str) -> Result<Vec<T>, EventError> {
        match self.channels.get_mut(name) {
            None => Err(EventError::UnknownEvent {
                name: name.to_owned(),
            }),
            Some(ChannelStorage::Dynamic(_)) => Err(EventError::KindMismatch {
                name: name.to_owned(),
            }),
            Some(ChannelStorage::Typed(buffer)) => buffer.drain(name),
        }
    }

    /// Push a dynamic event payload onto a dynamic channel's back buffer.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::UnknownEvent`] if `name` is not registered, or
    /// [`EventError::KindMismatch`] if the channel is typed.
    pub fn push_dynamic(&mut self, name: &str, value: Value) -> Result<(), EventError> {
        match self.channels.get_mut(name) {
            None => Err(EventError::UnknownEvent {
                name: name.to_owned(),
            }),
            Some(ChannelStorage::Typed(_)) => Err(EventError::KindMismatch {
                name: name.to_owned(),
            }),
            Some(ChannelStorage::Dynamic(buffer)) => {
                buffer.push(value);
                Ok(())
            }
        }
    }

    /// Drain a dynamic channel's front buffer.
    ///
    /// # Errors
    ///
    /// Same as [`Self::push_dynamic`].
    pub fn drain_dynamic(&mut self, name: &str) -> Result<Vec<Value>, EventError> {
        match self.channels.get_mut(name) {
            None => Err(EventError::UnknownEvent {
                name: name.to_owned(),
            }),
            Some(ChannelStorage::Typed(_)) => Err(EventError::KindMismatch {
                name: name.to_owned(),
            }),
            Some(ChannelStorage::Dynamic(buffer)) => Ok(buffer.drain()),
        }
    }

    /// Tick-end housekeeping: typed channels swap front/back（让 host pump_out
    /// 在 tick 后能 drain 本帧 emit 的事件，同时 host pump_in 推到 back 的
    /// 事件下个 tick 进 front 给脚本读）；dynamic channels 直接 clear（脚本
    /// 已在本 tick 内同帧消费完毕，旧的不该残留到下一 tick）。
    pub fn end_tick_all(&mut self) {
        for storage in self.channels.values_mut() {
            storage.end_tick();
        }
    }
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

/// `Value` -> boxed `T` deserializer fn-ptr factory.
fn deserialize_into_boxed<T>(value: Value) -> Result<BoxedAny, EventError>
where
    T: for<'de> Deserialize<'de> + Send + 'static,
{
    let instance: T = serde_json::from_value(value).map_err(|e| EventError::Deserialize {
        name: std::any::type_name::<T>().to_owned(),
        reason: e.to_string(),
    })?;
    Ok(Box::new(instance))
}

/// Push a previously-deserialized boxed `T` onto a typed channel's back
/// buffer. Recovers `T` by `Any::downcast` — assumes `boxed` came from
/// [`deserialize_into_boxed::<T>`] for the same `T`.
fn push_boxed<T>(
    buffer: &mut TypedEventBuffer,
    name: &str,
    boxed: BoxedAny,
) -> Result<(), EventError>
where
    T: Send + 'static,
{
    let value: T = *boxed
        .downcast::<T>()
        .map_err(|_| EventError::TypeMismatch {
            name: name.to_owned(),
        })?;
    buffer.push(name, value)
}

/// Serialize the front-buffer event at `index` (when scripts read events).
fn serialize_front_index<T>(front: &dyn AnyVec, index: usize) -> Result<Value, EventError>
where
    T: Serialize + 'static,
{
    let vec = front
        .as_any()
        .downcast_ref::<Vec<T>>()
        .ok_or_else(|| EventError::TypeMismatch {
            name: std::any::type_name::<T>().to_owned(),
        })?;
    let item = &vec[index];
    serde_json::to_value(item).map_err(|e| EventError::Serialize {
        name: std::any::type_name::<T>().to_owned(),
        reason: e.to_string(),
    })
}

fn serialize_typed<T: Serialize>(value: &T, name: &str) -> Result<Value, EventError> {
    serde_json::to_value(value).map_err(|e| EventError::Serialize {
        name: name.to_owned(),
        reason: e.to_string(),
    })
}
