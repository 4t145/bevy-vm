//! VM instance — runs scripts against an external Bevy `World`.
//!
//! [`VmInstance`] holds its scripts, registries, RNG and event buffers,
//! but **not** a `World` of its own. Every tick takes `&mut World`; every
//! entity it spawns is tagged with [`VmTag(self.id)`] so multiple
//! instances coexist cleanly in one world.
//!
//! For multi-VM scenarios, [`VmRegistry`] is a NonSend resource that maps
//! [`VmId`] → instance. The renderer side (see [`crate::vm::plugin`]) iterates
//! the registry each frame to tick everything.

use crate::component::{ComponentKind, ComponentRegistry};
use crate::config::{ConfigFormat, EntityConfig, SystemConfig, WorldConfig};
use crate::error::VmError;
use crate::event::{EventError, EventKind, EventRegistry, EventStore, merge_with_default};
use crate::system::{Pause, ScriptSystem, System, TickContext};
use crate::vm::id::{VmId, VmTag};
use crate::world_access::{self, WorldAccessError};
use bevy_ecs::prelude::*;
use bevy_time::Time;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

/// Build a [`VmInstance`] with custom typed events registered up front.
#[derive(Default)]
pub struct VmInstanceBuilder {
    events: EventRegistry,
}

impl VmInstanceBuilder {
    /// Empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a typed event channel under `name` for Rust type `T`.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the registration fails.
    pub fn with_event<T>(mut self, name: &str) -> Result<Self, VmError>
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    {
        self.events.register_typed::<T>(name)?;
        Ok(self)
    }

    /// Like [`Self::with_event`] but with a default merge baseline.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] on registration failure.
    pub fn with_event_default<T>(mut self, name: &str) -> Result<Self, VmError>
    where
        T: Serialize + for<'de> Deserialize<'de> + Default + Send + 'static,
    {
        self.events.register_typed_with_default::<T>(name)?;
        Ok(self)
    }

    /// Load a world config from `config_path` (file or directory containing
    /// `world.ron`), build the instance against `world`.
    ///
    /// # Errors
    ///
    /// IO / parse / registration / compile errors as appropriate.
    pub fn load(
        self,
        world: &mut World,
        config_path: impl AsRef<Path>,
    ) -> Result<VmInstance, VmError> {
        let config_path = config_path.as_ref();
        let resolved = resolve_root_path(config_path);
        let plugins = crate::plugin_loader::load_root(&resolved)?;
        self.build_from_plugins(world, plugins)
    }

    /// Like [`Self::load`] but reads inline `text`. plugin list must be empty.
    ///
    /// # Errors
    ///
    /// Same as [`Self::load`].
    pub fn from_text(
        self,
        world: &mut World,
        text: &str,
        format: ConfigFormat,
        base_dir: impl AsRef<Path>,
    ) -> Result<VmInstance, VmError> {
        let config = WorldConfig::from_text(text, format)?;
        if !config.plugins.is_empty() {
            return Err(VmError::Parse(
                "inline `from_text` does not support `plugins:` field; use `load()` with a file path"
                    .to_owned(),
            ));
        }
        let plugins = vec![crate::plugin_loader::LoadedPlugin {
            name: crate::plugin_loader::ROOT_PLUGIN.to_owned(),
            config,
            base_dir: base_dir.as_ref().to_path_buf(),
        }];
        self.build_from_plugins(world, plugins)
    }

    /// JSON convenience.
    ///
    /// # Errors
    ///
    /// Same as [`Self::from_text`].
    #[cfg(feature = "config-json")]
    pub fn from_json(
        self,
        world: &mut World,
        text: &str,
        base_dir: impl AsRef<Path>,
    ) -> Result<VmInstance, VmError> {
        self.from_text(world, text, ConfigFormat::Json, base_dir)
    }

    /// RON convenience.
    ///
    /// # Errors
    ///
    /// Same as [`Self::from_text`].
    #[cfg(feature = "config-ron")]
    pub fn from_ron(
        self,
        world: &mut World,
        text: &str,
        base_dir: impl AsRef<Path>,
    ) -> Result<VmInstance, VmError> {
        self.from_text(world, text, ConfigFormat::Ron, base_dir)
    }

    fn build_from_plugins(
        mut self,
        world: &mut World,
        plugins: Vec<crate::plugin_loader::LoadedPlugin>,
    ) -> Result<VmInstance, VmError> {
        let id = VmId::next();
        let mut components = ComponentRegistry::with_builtins(world);
        components.register_field_type::<VmTag>();
        world.register_component::<VmTag>();
        world.register_component::<bevy_ecs::hierarchy::ChildOf>();

        let root_seed = plugins
            .iter()
            .find(|p| p.name == crate::plugin_loader::ROOT_PLUGIN)
            .and_then(|p| p.config.seed);
        let rng = match root_seed {
            Some(seed) => crate::random::VmRng::from_seed(seed),
            None => crate::random::VmRng::from_entropy(),
        };

        for plugin in &plugins {
            register_plugin_components(world, &mut components, plugin)?;
            register_plugin_events(&mut self.events, plugin)?;
        }
        let components = Rc::new(components);
        let events = Rc::new(self.events);

        for plugin in &plugins {
            spawn_plugin_entities(world, &components, plugin, id)?;
        }

        let systems = schedule_scripts(&plugins, &components, &events, id)?;
        let store = EventStore::new(&events);
        Ok(VmInstance {
            id,
            components,
            events,
            store,
            systems,
            rng,
            time: Time::<()>::default(),
            pause: Pause::default(),
        })
    }
}

/// One running VM instance — scripts + registries + per-instance state.
///
/// Does not own a `World`. Every tick takes `&mut World`.
pub struct VmInstance {
    id: VmId,
    components: Rc<ComponentRegistry>,
    events: Rc<EventRegistry>,
    store: EventStore,
    systems: Vec<Box<dyn System>>,
    rng: crate::random::VmRng,
    time: Time<()>,
    pause: Pause,
}

impl VmInstance {
    /// Identifier of this instance.
    #[must_use]
    pub fn id(&self) -> VmId {
        self.id
    }

    /// Convenience: build with no custom typed events.
    ///
    /// # Errors
    ///
    /// Same as [`VmInstanceBuilder::load`].
    pub fn load(world: &mut World, config_path: impl AsRef<Path>) -> Result<Self, VmError> {
        VmInstanceBuilder::new().load(world, config_path)
    }

    /// Convenience: load from inline text.
    ///
    /// # Errors
    ///
    /// Same as [`VmInstanceBuilder::from_text`].
    pub fn from_text(
        world: &mut World,
        text: &str,
        format: ConfigFormat,
        base_dir: impl AsRef<Path>,
    ) -> Result<Self, VmError> {
        VmInstanceBuilder::new().from_text(world, text, format, base_dir)
    }

    /// Convenience: JSON.
    ///
    /// # Errors
    ///
    /// Same as [`Self::load`].
    #[cfg(feature = "config-json")]
    pub fn from_json(
        world: &mut World,
        text: &str,
        base_dir: impl AsRef<Path>,
    ) -> Result<Self, VmError> {
        VmInstanceBuilder::new().from_json(world, text, base_dir)
    }

    /// Convenience: RON.
    ///
    /// # Errors
    ///
    /// Same as [`Self::load`].
    #[cfg(feature = "config-ron")]
    pub fn from_ron(
        world: &mut World,
        text: &str,
        base_dir: impl AsRef<Path>,
    ) -> Result<Self, VmError> {
        VmInstanceBuilder::new().from_ron(world, text, base_dir)
    }

    /// Advance the per-instance clock by `delta`. Pause inserts zero.
    pub fn advance_time(&mut self, delta: std::time::Duration) {
        let effective = if self.pause.paused {
            std::time::Duration::ZERO
        } else {
            delta
        };
        self.time.advance_by(effective);
    }

    /// Pause / unpause the per-instance clock.
    pub fn set_paused(&mut self, paused: bool) {
        self.pause.paused = paused;
    }

    /// Whether this instance's clock is paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.pause.paused
    }

    /// Run one tick: scripts → swap event buffers.
    ///
    /// 三件 per-instance 状态（RNG / Time / Pause）通过 [`TickContext`] 直接
    /// 借给系统——不再来回搬 World resource。多 VM 共享 World 时彼此完全隔离。
    ///
    /// # Errors
    ///
    /// Returns the corresponding [`VmError`] if any system raises one.
    pub fn tick(&mut self, world: &mut World) -> Result<(), VmError> {
        let mut ctx = TickContext {
            world,
            events: &mut self.store,
            rng: &mut self.rng,
            time: &mut self.time,
            pause: &mut self.pause,
        };
        let result = (|| -> Result<(), VmError> {
            for system in &self.systems {
                system.run(&mut ctx)?;
            }
            Ok(())
        })();
        self.store.end_tick_all();
        result
    }

    /// Despawn every entity tagged with this instance's [`VmTag`].
    pub fn unload(&mut self, world: &mut World) {
        let id = self.id;
        let mut q = world.query::<(Entity, &VmTag)>();
        let entities: Vec<Entity> = q
            .iter(world)
            .filter(|(_, tag)| tag.vm == id)
            .map(|(e, _)| e)
            .collect();
        for entity in entities {
            if let Ok(em) = world.get_entity_mut(entity) {
                em.despawn();
            }
        }
    }

    /// Shared handle to the [`ComponentRegistry`] driving this instance.
    #[must_use]
    pub fn components(&self) -> Rc<ComponentRegistry> {
        Rc::clone(&self.components)
    }

    /// Read-only borrow of the [`EventStore`].
    #[must_use]
    pub fn events(&self) -> &EventStore {
        &self.store
    }

    /// Mutable borrow of the [`EventStore`].
    pub fn events_mut(&mut self) -> &mut EventStore {
        &mut self.store
    }

    /// Send a typed event into the back buffer for `name`.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the channel is unknown / dynamic / wrong T.
    pub fn send_event<T: Send + 'static>(&mut self, name: &str, event: T) -> Result<(), VmError> {
        self.store.push_typed(name, event).map_err(VmError::from)
    }

    /// Send a dynamic event by name. Payload is merged with the declared default.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] when the channel is unknown or typed.
    pub fn send_event_dynamic(&mut self, name: &str, payload: Value) -> Result<(), VmError> {
        let merged = match self.events.resolve(name) {
            Some(EventKind::Typed(_)) => {
                return Err(EventError::KindMismatch {
                    name: name.to_owned(),
                }
                .into());
            }
            Some(EventKind::Dynamic(dyn_event)) => {
                merge_with_default(payload, Some(&dyn_event.default))
            }
            None => {
                return Err(EventError::UnknownEvent {
                    name: name.to_owned(),
                }
                .into());
            }
        };
        self.store.push_dynamic(name, merged).map_err(VmError::from)
    }

    /// Drain typed events from `name`'s front buffer.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] on channel mismatches.
    pub fn drain_events<T: Send + 'static>(&mut self, name: &str) -> Result<Vec<T>, VmError> {
        self.store.drain_typed::<T>(name).map_err(VmError::from)
    }

    /// Drain dynamic events from `name`'s front buffer.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::Event`] on channel mismatches.
    pub fn drain_events_dynamic(&mut self, name: &str) -> Result<Vec<Value>, VmError> {
        self.store.drain_dynamic(name).map_err(VmError::from)
    }

    /// Query every entity in `world` carrying both `component` and this
    /// instance's [`VmTag`].
    #[must_use]
    pub fn query(&self, world: &mut World, component: &str) -> Vec<Entity> {
        let registry = Rc::clone(&self.components);
        world_access::query_with_component_tagged(world, &registry, component, self.id)
    }

    /// Read a value at `path` on `entity`'s `component`.
    ///
    /// # Errors
    ///
    /// Returns [`WorldAccessError`] if the entity / component / path is invalid.
    pub fn get(
        &self,
        world: &World,
        entity: Entity,
        component: &str,
        path: &str,
    ) -> Result<Value, WorldAccessError> {
        world_access::get(world, &self.components, entity, component, path)
    }
}

/// NonSend resource holding every active [`VmInstance`] in this `World`.
///
/// Bevy NonSend resources are single-instance, so multi-VM support funnels
/// through this map. Insertion/lookup by [`VmId`].
#[derive(Default)]
pub struct VmRegistry {
    instances: HashMap<VmId, VmInstance>,
}

impl VmRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `vm`, returning its `VmId`. The instance lives in the
    /// registry from now on; tick / unload is done through the registry.
    pub fn insert(&mut self, vm: VmInstance) -> VmId {
        let id = vm.id();
        self.instances.insert(id, vm);
        id
    }

    /// Remove and return the instance with this id, if present.
    #[must_use]
    pub fn remove(&mut self, id: VmId) -> Option<VmInstance> {
        self.instances.remove(&id)
    }

    /// Borrow the instance with this id.
    #[must_use]
    pub fn get(&self, id: VmId) -> Option<&VmInstance> {
        self.instances.get(&id)
    }

    /// Mutably borrow the instance with this id.
    #[must_use]
    pub fn get_mut(&mut self, id: VmId) -> Option<&mut VmInstance> {
        self.instances.get_mut(&id)
    }

    /// Iterate over `(VmId, &VmInstance)`.
    pub fn iter(&self) -> impl Iterator<Item = (&VmId, &VmInstance)> {
        self.instances.iter()
    }

    /// Identifiers of all live instances.
    #[must_use]
    pub fn ids(&self) -> Vec<VmId> {
        self.instances.keys().copied().collect()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Number of live instances.
    #[must_use]
    pub fn len(&self) -> usize {
        self.instances.len()
    }
}

fn resolve_root_path(input: &Path) -> std::path::PathBuf {
    if input.is_dir() {
        input.join("world.ron")
    } else {
        input.to_path_buf()
    }
}

fn load_system(
    system_config: &SystemConfig,
    base_dir: &Path,
    plugin_name: &Rc<str>,
    components: &Rc<ComponentRegistry>,
    events: &Rc<EventRegistry>,
    vm_id: VmId,
) -> Result<Box<dyn System>, VmError> {
    let SystemConfig::Script { path, run_if, .. } = system_config;
    let script_path = base_dir.join(path);
    let source = std::fs::read_to_string(&script_path).map_err(|e| VmError::Io {
        path: script_path.display().to_string(),
        reason: e.to_string(),
    })?;
    let script_dir = script_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let system = ScriptSystem::compile(
        &source,
        Rc::clone(plugin_name),
        &script_dir,
        Rc::clone(components),
        Rc::clone(events),
        run_if,
        vm_id,
    )?;
    Ok(Box::new(system))
}

fn register_plugin_components(
    world: &mut World,
    registry: &mut ComponentRegistry,
    plugin: &crate::plugin_loader::LoadedPlugin,
) -> Result<(), VmError> {
    for decl in &plugin.config.components {
        let name = qualify_name(&plugin.name, &decl.name);
        registry.register_dynamic(world, &name, decl.default.clone())?;
    }
    Ok(())
}

fn register_plugin_events(
    registry: &mut EventRegistry,
    plugin: &crate::plugin_loader::LoadedPlugin,
) -> Result<(), VmError> {
    for decl in &plugin.config.events {
        let name = qualify_name(&plugin.name, &decl.name);
        registry.register_dynamic(&name, decl.default.clone())?;
    }
    Ok(())
}

fn qualify_name(plugin: &str, short: &str) -> String {
    if plugin == crate::plugin_loader::ROOT_PLUGIN || short.contains("::") {
        short.to_owned()
    } else {
        format!("{plugin}::{short}")
    }
}

fn spawn_plugin_entities(
    world: &mut World,
    registry: &ComponentRegistry,
    plugin: &crate::plugin_loader::LoadedPlugin,
    vm_id: VmId,
) -> Result<(), VmError> {
    for entity_config in &plugin.config.entities {
        spawn_entity(world, registry, plugin, entity_config, vm_id)?;
    }
    Ok(())
}

fn spawn_entity(
    world: &mut World,
    registry: &ComponentRegistry,
    plugin: &crate::plugin_loader::LoadedPlugin,
    entity_config: &EntityConfig,
    vm_id: VmId,
) -> Result<(), VmError> {
    let entity = world.spawn(VmTag::new(vm_id)).id();
    for (component_name, overrides) in &entity_config.components {
        let resolved = qualify_component_ref(registry, &plugin.name, component_name)?;
        match registry.resolve(&resolved) {
            Some(ComponentKind::Typed(_)) => {
                init_typed_component(world, registry, entity, &resolved, overrides)?;
            }
            Some(ComponentKind::Dynamic(_)) => {
                init_dynamic_component(world, registry, entity, &resolved, overrides)?;
            }
            None => return Err(VmError::UnknownComponent(resolved)),
        }
    }
    Ok(())
}

fn qualify_component_ref(
    registry: &ComponentRegistry,
    plugin_name: &str,
    name: &str,
) -> Result<String, VmError> {
    if name.contains("::") {
        return Ok(name.to_owned());
    }
    if registry.resolve(name).is_some() {
        return Ok(name.to_owned());
    }
    let qualified = format!("{plugin_name}::{name}");
    if registry.resolve(&qualified).is_some() {
        return Ok(qualified);
    }
    Err(VmError::UnknownComponent(name.to_owned()))
}

fn init_typed_component(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component_name: &str,
    overrides: &Value,
) -> Result<(), VmError> {
    let typed = registry
        .typed(component_name)
        .ok_or_else(|| VmError::UnknownComponent(component_name.to_owned()))?;
    typed
        .insert_from_value(world, entity, registry.type_registry(), overrides.clone())
        .map_err(|source| VmError::InitTypedComponent {
            component: component_name.to_owned(),
            source,
        })?;
    world_access::apply_required(world, registry, entity, component_name).map_err(|source| {
        VmError::InitTypedRequired {
            component: component_name.to_owned(),
            source,
        }
    })
}

fn init_dynamic_component(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component_name: &str,
    overrides: &Value,
) -> Result<(), VmError> {
    world_access::insert_dynamic_default(world, registry, entity, component_name).map_err(
        |source| VmError::InsertDynamicDefault {
            component: component_name.to_owned(),
            source,
        },
    )?;
    let Value::Object(fields) = overrides else {
        return Ok(());
    };
    for (field, value) in fields.iter() {
        world_access::set(
            world,
            registry,
            entity,
            component_name,
            field,
            value.clone(),
        )
        .map_err(|source| VmError::InitDynamicField {
            component: component_name.to_owned(),
            field: field.clone(),
            source,
        })?;
    }
    Ok(())
}

fn schedule_scripts(
    plugins: &[crate::plugin_loader::LoadedPlugin],
    components: &Rc<ComponentRegistry>,
    events: &Rc<EventRegistry>,
    vm_id: VmId,
) -> Result<Vec<Box<dyn System>>, VmError> {
    use std::collections::{HashMap, HashSet};

    struct ScriptMeta<'a> {
        plugin: &'a crate::plugin_loader::LoadedPlugin,
        config: &'a SystemConfig,
        full_name: String,
        sets: Vec<String>,
    }

    let mut metas: Vec<ScriptMeta<'_>> = Vec::new();
    for plugin in plugins {
        for system_config in &plugin.config.systems {
            let SystemConfig::Script { path, in_set, .. } = system_config;
            let stem = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(path);
            let full_name = format!("{}::{}", plugin.name, stem);
            let mut sets = vec![plugin.name.clone()];
            sets.extend(in_set.iter().cloned());
            metas.push(ScriptMeta {
                plugin,
                config: system_config,
                full_name,
                sets,
            });
        }
    }

    let mut systems_in_set: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, meta) in metas.iter().enumerate() {
        for set in &meta.sets {
            systems_in_set.entry(set.clone()).or_default().push(i);
        }
    }
    let by_full_name: HashMap<&str, usize> = metas
        .iter()
        .enumerate()
        .map(|(i, m)| (m.full_name.as_str(), i))
        .collect();

    let resolve_ref = |name: &str| -> Vec<usize> {
        if let Some(targets) = systems_in_set.get(name) {
            return targets.clone();
        }
        if let Some(&idx) = by_full_name.get(name) {
            return vec![idx];
        }
        Vec::new()
    };

    let n = metas.len();
    let mut edges: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];
    let add_edge =
        |edges: &mut Vec<HashSet<usize>>, in_degree: &mut Vec<usize>, from: usize, to: usize| {
            if from == to {
                return;
            }
            if edges[from].insert(to) {
                in_degree[to] += 1;
            }
        };

    for i in 1..n {
        if metas[i - 1].plugin.name == metas[i].plugin.name {
            add_edge(&mut edges, &mut in_degree, i - 1, i);
        }
    }

    for (i, meta) in metas.iter().enumerate() {
        let SystemConfig::Script { before, after, .. } = meta.config;
        for target_name in before {
            let targets = resolve_ref(target_name);
            if targets.is_empty() {
                return Err(VmError::UnknownSystemRef {
                    system: meta.full_name.clone(),
                    missing: target_name.clone(),
                    kind: "before",
                });
            }
            for &j in &targets {
                add_edge(&mut edges, &mut in_degree, i, j);
            }
        }
        for source_name in after {
            let sources = resolve_ref(source_name);
            if sources.is_empty() {
                return Err(VmError::UnknownSystemRef {
                    system: meta.full_name.clone(),
                    missing: source_name.clone(),
                    kind: "after",
                });
            }
            for &j in &sources {
                add_edge(&mut edges, &mut in_degree, j, i);
            }
        }
    }

    let mut order: Vec<usize> = Vec::with_capacity(n);
    loop {
        let pick = (0..n).find(|&i| in_degree[i] == 0 && !order.contains(&i));
        let Some(idx) = pick else {
            break;
        };
        order.push(idx);
        for &j in &edges[idx] {
            in_degree[j] -= 1;
        }
        in_degree[idx] = usize::MAX;
    }

    if order.len() != n {
        let stuck: Vec<&str> = (0..n)
            .filter(|i| !order.contains(i))
            .map(|i| metas[i].full_name.as_str())
            .collect();
        return Err(VmError::SystemOrderCycle {
            chain: stuck.join(" <-> "),
        });
    }

    let mut systems: Vec<Box<dyn System>> = Vec::with_capacity(n);
    for idx in order {
        let meta = &metas[idx];
        let plugin_name: Rc<str> = Rc::from(meta.plugin.name.as_str());
        let system = load_system(
            meta.config,
            &meta.plugin.base_dir,
            &plugin_name,
            components,
            events,
            vm_id,
        )?;
        systems.push(system);
    }
    Ok(systems)
}
