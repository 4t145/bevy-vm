//! 独占解释器：单个世界的驱动核心。
//!
//! [`VmWorld`] 持有一个独立的 `bevy_ecs::World`，按 [`WorldConfig`] 构建初始
//! 实体，并逐 tick 执行该世界的一组 system。它独占其 World，没有任何并发对手，
//! 因此 system 的访问集合是否动态都无所谓——上层管理者负责在世界之间并行。
//!
//! 一次 tick 分两个阶段：
//! 1. **system 阶段**：按加载顺序依次运行各 [`System`]（如脚本），它们通过宿主
//!    函数 query/get/set 等读写世界。这是「AI 行为」的落点。
//! 2. **静态计算阶段**：用普通类型化查询把速度积分到位置。这是「重活下沉」的
//!    落点，全速运行、无需反射。

use crate::component::{ComponentKind, ComponentRegistry};
use crate::component::{Position, Velocity};
use crate::config::{EntityConfig, SystemConfig, WorldConfig};
use crate::error::VmError;
use crate::system::{ScriptSystem, System};
use crate::world_access;
use bevy_ecs::prelude::*;
use ron::Value;
use std::path::Path;
use std::rc::Rc;

/// 每个世界自带的、独立可 tick 的模拟单元。
pub struct VmWorld {
    world: World,
    registry: Rc<ComponentRegistry>,
    systems: Vec<Box<dyn System>>,
}

impl VmWorld {
    /// 从世界配置文件加载并构建一个世界。
    ///
    /// 配置中声明的 system 脚本路径相对该配置文件所在目录解析。
    ///
    /// # Errors
    ///
    /// - 配置文件无法读取时返回 [`VmError::Io`]。
    /// - 配置文本无法解析时返回 [`VmError::Parse`]。
    /// - 配置引用了未登记的组件名时返回 [`VmError::UnknownComponent`]。
    /// - 反射写入组件初值失败时返回 [`VmError::SetProperty`]。
    /// - system 脚本文件无法读取或无法编译时返回 [`VmError::Io`] /
    ///   [`VmError::ScriptCompile`]。
    pub fn load(config_path: impl AsRef<Path>) -> Result<Self, VmError> {
        let config_path = config_path.as_ref();
        let text = std::fs::read_to_string(config_path).map_err(|e| VmError::Io {
            path: config_path.display().to_string(),
            reason: e.to_string(),
        })?;
        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_ron(&text, base_dir)
    }

    /// 从配置文本与基准目录构建世界。
    ///
    /// `base_dir` 用于解析 system 脚本的相对路径。
    ///
    /// # Errors
    ///
    /// 见 [`VmWorld::load`]。
    pub fn from_ron(text: &str, base_dir: impl AsRef<Path>) -> Result<Self, VmError> {
        let config = WorldConfig::from_ron(text)?;
        let mut registry = ComponentRegistry::with_builtins();
        let mut world = World::new();

        register_dynamic_components(&mut world, &mut registry, &config);
        let registry = Rc::new(registry);
        spawn_entities(&mut world, &registry, &config)?;

        let systems = Self::load_systems(&config, base_dir.as_ref(), &registry)?;
        Ok(Self {
            world,
            registry,
            systems,
        })
    }

    /// 按配置加载全部 system。
    fn load_systems(
        config: &WorldConfig,
        base_dir: &Path,
        registry: &Rc<ComponentRegistry>,
    ) -> Result<Vec<Box<dyn System>>, VmError> {
        config
            .systems
            .iter()
            .map(|system_config| Self::load_system(system_config, base_dir, registry))
            .collect()
    }

    /// 加载单个 system。
    fn load_system(
        system_config: &SystemConfig,
        base_dir: &Path,
        registry: &Rc<ComponentRegistry>,
    ) -> Result<Box<dyn System>, VmError> {
        let SystemConfig::Script { path } = system_config;
        let script_path = base_dir.join(path);
        let source = std::fs::read_to_string(&script_path).map_err(|e| VmError::Io {
            path: script_path.display().to_string(),
            reason: e.to_string(),
        })?;
        let system = ScriptSystem::compile(&source, Rc::clone(registry))?;
        Ok(Box::new(system))
    }

    /// 推进该世界一个 tick：先依次运行各 system，再做静态移动积分。
    ///
    /// # Errors
    ///
    /// 任一 system 运行期抛错时返回对应 [`VmError`]。静态阶段不会失败。
    pub fn tick(&mut self) -> Result<(), VmError> {
        for system in &self.systems {
            system.run(&mut self.world)?;
        }
        integrate_movement(&mut self.world);
        Ok(())
    }

    /// 返回底层 World 的只读引用，便于检视世界状态。
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// 查询挂有指定组件的全部实体。
    #[must_use]
    pub fn query(&mut self, component: &str) -> Vec<Entity> {
        let registry = Rc::clone(&self.registry);
        world_access::query_with_component(&mut self.world, &registry, component)
    }

    /// 读取实体上某组件给定点号路径处的值。
    ///
    /// # Errors
    ///
    /// 实体或组件不存在、路径无效时返回描述性错误。
    pub fn get(&self, entity: Entity, component: &str, path: &str) -> Result<Value, String> {
        world_access::get(&self.world, &self.registry, entity, component, path)
    }

    /// 全量检视：返回世界当前所有实体及其组件值的快照。
    ///
    /// 这是为调试器/检视器设计的——它克隆整棵世界状态，**不应**用于每帧渲染
    /// 热路径（渲染应只针对性读取可视字段并增量同步）。
    #[must_use]
    pub fn inspect(&mut self) -> WorldSnapshot {
        let entities = world_access::all_entities(&mut self.world);
        let registry = Rc::clone(&self.registry);
        let entities = entities
            .into_iter()
            .map(|entity| self.inspect_entity(&registry, entity))
            .collect();
        WorldSnapshot { entities }
    }

    /// 检视单个实体：收集其所有挂载组件的完整值。
    fn inspect_entity(&self, registry: &ComponentRegistry, entity: Entity) -> EntitySnapshot {
        let components = world_access::components_of(&self.world, registry, entity)
            .into_iter()
            .filter_map(|name| {
                let value = world_access::read_component(&self.world, registry, entity, &name)
                    .ok()
                    .flatten()?;
                Some((name, value))
            })
            .collect();
        EntitySnapshot { entity, components }
    }
}

/// 世界某一时刻的全量快照，用于检视/调试。
#[derive(Debug, Clone)]
pub struct WorldSnapshot {
    /// 世界中全部实体的快照。
    pub entities: Vec<EntitySnapshot>,
}

/// 单个实体的快照：实体 id + 其全部组件的完整值。
#[derive(Debug, Clone)]
pub struct EntitySnapshot {
    /// 实体标识。
    pub entity: Entity,
    /// 组件名 -> 组件完整值。
    pub components: Vec<(String, Value)>,
}

/// 按配置声明注册全部内容层动态组件。
fn register_dynamic_components(
    world: &mut World,
    registry: &mut ComponentRegistry,
    config: &WorldConfig,
) {
    for decl in &config.components {
        registry.register_dynamic(world, &decl.name, decl.default.clone());
    }
}

/// 按配置 spawn 全部实体，并写入各组件初值。
fn spawn_entities(
    world: &mut World,
    registry: &ComponentRegistry,
    config: &WorldConfig,
) -> Result<(), VmError> {
    for entity_config in &config.entities {
        spawn_entity(world, registry, entity_config)?;
    }
    Ok(())
}

/// spawn 单个实体，并按其配置初始化所挂的每个组件。
fn spawn_entity(
    world: &mut World,
    registry: &ComponentRegistry,
    entity_config: &EntityConfig,
) -> Result<(), VmError> {
    let entity = world.spawn_empty().id();
    for (component_name, overrides) in &entity_config.components {
        match registry.resolve(component_name) {
            Some(ComponentKind::Typed) => {
                init_typed_component(world, registry, entity, component_name, overrides)?;
            }
            Some(ComponentKind::Dynamic(_)) => {
                init_dynamic_component(world, registry, entity, component_name, overrides)?;
            }
            None => return Err(VmError::UnknownComponent(component_name.clone())),
        }
    }
    Ok(())
}

/// 初始化引擎层类型化组件：插入默认值，再按反射路径写入覆盖字段。
fn init_typed_component(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component_name: &str,
    overrides: &Value,
) -> Result<(), VmError> {
    let _ = registry.insert_typed_default(world, entity, component_name);
    let Value::Map(fields) = overrides else {
        return Ok(());
    };
    for (key, value) in fields.iter() {
        let Value::String(field) = key else {
            continue;
        };
        world_access::set(
            world,
            registry,
            entity,
            component_name,
            field,
            value.clone(),
        )
        .map_err(|reason| VmError::SetProperty {
            path: format!("{component_name}.{field}"),
            reason,
        })?;
    }
    Ok(())
}

/// 初始化内容层动态组件：插入默认值实例，再用配置覆盖做顶层字段合并。
fn init_dynamic_component(
    world: &mut World,
    registry: &ComponentRegistry,
    entity: Entity,
    component_name: &str,
    overrides: &Value,
) -> Result<(), VmError> {
    world_access::insert_dynamic_default(world, registry, entity, component_name).map_err(
        |reason| VmError::SetProperty {
            path: component_name.to_owned(),
            reason,
        },
    )?;
    let Value::Map(fields) = overrides else {
        return Ok(());
    };
    for (key, value) in fields.iter() {
        let Value::String(field) = key else {
            continue;
        };
        world_access::set(
            world,
            registry,
            entity,
            component_name,
            field,
            value.clone(),
        )
        .map_err(|reason| VmError::SetProperty {
            path: format!("{component_name}.{field}"),
            reason,
        })?;
    }
    Ok(())
}

/// 静态计算下沉：把速度按单位时间积分到位置。
fn integrate_movement(world: &mut World) {
    let mut query = world.query::<(&mut Position, &Velocity)>();
    for (mut position, velocity) in query.iter_mut(world) {
        position.x += velocity.x;
        position.y += velocity.y;
        position.z += velocity.z;
    }
}
