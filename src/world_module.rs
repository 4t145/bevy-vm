//! World module loader: 把根 [`WorldConfig`] 的 `modules:` 字段递归展开成
//! 一组命名 module，按 `dependencies:` 拓扑排序，再交给 [`crate::vm`] 装配。
//!
//! "module" 在这里指脚本侧的内容单元——配置 ron + 脚本 rhai 的集合。与
//! host-side 的 [`crate::plugin::VmPlugin`] 是完全不同的概念，名字不再共用。
//!
//! # module 命名约定
//!
//! - 文件名 stem 即 module 名（与 Rust mod 一致）。`tiles.ron` → `tiles`。
//! - 顶级 `world.ron` 的内容视作"全局空间"——它的 components/events 不带
//!   module 前缀；module 文件里声明的 components/events 在注册期变成
//!   `<module>::<short>`。
//! - 同名 module（路径不同但 stem 相同）以**首次加载**为准，后续被忽略并
//!   合并依赖关系——这避免了"module 写两次出歧义"的常见错误。
//!
//! # 加载流程
//!
//! 1. 从 [`load_root`] 开始递归读取每个 module 文件。
//! 2. 每个 module 解析为 `LoadedModule { name, config, base_dir }`。
//! 3. 拓扑排序，环则报 [`VmError::ModuleCycle`]，缺失依赖报
//!    [`VmError::ModuleMissingDependency`]。
//! 4. 输出 `Vec<LoadedModule>`，**根 world 总是最后一个**——它的实体可以
//!    引用任何 module 的组件。

use crate::config::{ConfigFormat, WorldConfig};
use crate::error::VmError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 顶级 world 的合成 module 名——它没有自己的文件名 stem，但需要在加载
/// 图里有一个稳定的 id 用于依赖与冲突检测。
pub const ROOT_MODULE: &str = "<root>";

/// 一个被加载到内存的 module（含根 world）。
///
/// 不持有解析后的 components/events 等业务结构——那是 [`crate::vm`] 的
/// 职责；这里只管"把 ron 文件读成 [`WorldConfig`] + 元数据"。
pub struct LoadedModule {
    /// module 名。根 world 取 [`ROOT_MODULE`]，其它取文件 stem。
    pub name: String,
    /// 解析后的 ron 配置。
    pub config: WorldConfig,
    /// 该文件所在目录——脚本路径、嵌套 module 路径相对它解析。
    pub base_dir: PathBuf,
}

/// 入口：从一个根 world 文件出发，按 `modules:` 递归收集所有 module，
/// 然后按依赖拓扑排序。
///
/// 根的 `LoadedModule.name` 是 [`ROOT_MODULE`]。
///
/// # Errors
///
/// 任何 IO / 解析 / 依赖图错误。
pub fn load_root(root_path: &Path) -> Result<Vec<LoadedModule>, VmError> {
    let (root_config, root_base_dir) = read_config(root_path)?;
    let mut graph = ModuleGraph::new();
    graph.insert(LoadedModule {
        name: ROOT_MODULE.to_owned(),
        config: root_config,
        base_dir: root_base_dir,
    });
    expand_recursive(&mut graph, ROOT_MODULE)?;
    graph.topo_sort()
}

/// module 收集状态：name → loaded。同时维护 declared 顺序作为拓扑稳定 tie-break。
struct ModuleGraph {
    modules: HashMap<String, LoadedModule>,
    /// 插入顺序——拓扑排序时同层节点按此排序，让加载顺序对作者可预测。
    insertion_order: Vec<String>,
}

impl ModuleGraph {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
            insertion_order: Vec::new(),
        }
    }

    fn contains(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }

    fn insert(&mut self, module: LoadedModule) {
        let name = module.name.clone();
        if self.modules.insert(name.clone(), module).is_none() {
            self.insertion_order.push(name);
        }
    }

    fn get(&self, name: &str) -> Option<&LoadedModule> {
        self.modules.get(name)
    }

    /// Kahn 拓扑：依赖在前，被依赖的后出现。
    /// 同层（in_degree=0）按 [`Self::insertion_order`] 选取——给作者可预测加载序。
    /// 根 module（[`ROOT_MODULE`]）隐式依赖所有别的 module，钉在最后。
    fn topo_sort(mut self) -> Result<Vec<LoadedModule>, VmError> {
        // effective_deps[name] = 该 module 的依赖名列表（含根的隐式依赖）。
        let mut effective_deps: HashMap<String, Vec<String>> = HashMap::new();
        for (name, module) in &self.modules {
            let mut deps = module.config.dependencies.clone();
            if name == ROOT_MODULE {
                for other in self.modules.keys() {
                    if other != ROOT_MODULE {
                        deps.push(other.clone());
                    }
                }
            }
            for dep in &deps {
                if !self.modules.contains_key(dep) {
                    return Err(VmError::ModuleMissingDependency {
                        module: name.clone(),
                        missing: dep.clone(),
                    });
                }
            }
            effective_deps.insert(name.clone(), deps);
        }

        // in_degree[name] = name 还剩多少未满足的依赖。
        let mut in_degree: HashMap<String, usize> = effective_deps
            .iter()
            .map(|(name, deps)| (name.clone(), deps.len()))
            .collect();
        // reverse[dep] = 依赖 dep 的所有 module（dep emit 后用来解锁它们）。
        let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
        for (name, deps) in &effective_deps {
            for dep in deps {
                reverse.entry(dep.clone()).or_default().push(name.clone());
            }
        }

        let mut output: Vec<LoadedModule> = Vec::with_capacity(self.modules.len());
        loop {
            // 每轮在 insertion_order 中找第一个 in_degree=0 的 module。
            // 注意 insertion_order 在 emit 后会清掉对应项，避免重复。
            let picked = self
                .insertion_order
                .iter()
                .find(|n| in_degree.get(n.as_str()).copied() == Some(0))
                .cloned();
            let Some(name) = picked else {
                break;
            };
            self.insertion_order.retain(|n| n != &name);
            in_degree.remove(&name);
            let module = self.modules.remove(&name).expect("checked above");
            output.push(module);
            if let Some(dependents) = reverse.get(&name) {
                for d in dependents {
                    if let Some(deg) = in_degree.get_mut(d) {
                        *deg = deg.saturating_sub(1);
                    }
                }
            }
        }

        // 还有 module 没 emit ⇒ 有环。
        if !self.modules.is_empty() {
            let chain = describe_cycle(&self.modules, &effective_deps);
            return Err(VmError::ModuleCycle { chain });
        }
        Ok(output)
    }
}

/// 递归展开：遍历 module 的 `modules:`，加载每个引用的文件。
/// 已加载（按 stem 名）的 module 跳过——同名 module 在加载图里只一份。
fn expand_recursive(graph: &mut ModuleGraph, current: &str) -> Result<(), VmError> {
    // 收集本轮要处理的引用——克隆出来避免对 graph 的双借用。
    let to_process: Vec<(String, PathBuf)> = {
        let module = graph.get(current).expect("module in graph by precondition");
        module
            .config
            .modules
            .iter()
            .map(|p| (p.clone(), module.base_dir.clone()))
            .collect()
    };
    for (rel_path, base_dir) in to_process {
        let resolved = resolve_module_path(&base_dir, &rel_path);
        let stem = module_stem(&resolved);
        if graph.contains(&stem) {
            continue;
        }
        let (config, module_base_dir) = read_config(&resolved)?;
        graph.insert(LoadedModule {
            name: stem.clone(),
            config,
            base_dir: module_base_dir,
        });
        expand_recursive(graph, &stem)?;
    }
    Ok(())
}

/// 把 module 在 `modules:` 字段里写的相对路径解析成绝对/可读路径。
/// 缺 `.ron` 扩展自动补全——和 Rust mod 一样让短名引用更自然。
fn resolve_module_path(base_dir: &Path, rel: &str) -> PathBuf {
    let mut path = base_dir.join(rel);
    if path.extension().is_none() {
        path.set_extension("ron");
    }
    path
}

/// `path/to/foo.ron` → `"foo"`；带保险：扩展名或 stem 缺失时退回完整文件名字符串。
fn module_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| {
            path.to_str()
                .expect("path is utf-8 because we constructed it from utf-8 strings")
        })
        .to_owned()
}

fn read_config(path: &Path) -> Result<(WorldConfig, PathBuf), VmError> {
    let format =
        ConfigFormat::from_extension(path.extension().and_then(|e| e.to_str()).unwrap_or(""))
            .ok_or_else(|| {
                VmError::Parse(format!(
                    "unsupported module file extension: `{}`",
                    path.display()
                ))
            })?;
    let text = std::fs::read_to_string(path).map_err(|e| VmError::Io {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    let config = WorldConfig::from_text(&text, format)?;
    let base_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok((config, base_dir))
}

/// 当拓扑剩下没 emit 的 module 时，从中选一个走依赖链回头查环。
fn describe_cycle(
    remaining: &HashMap<String, LoadedModule>,
    deps: &HashMap<String, Vec<String>>,
) -> String {
    // 任取一个未排出的节点，沿 deps 走直到回到访问过的——闭环路径。
    let Some(start) = remaining.keys().next() else {
        return "<empty>".to_owned();
    };
    let mut path: Vec<String> = vec![start.clone()];
    let mut current = start.clone();
    loop {
        let next = deps
            .get(&current)
            .and_then(|d| d.iter().find(|name| remaining.contains_key(name.as_str())))
            .cloned();
        match next {
            Some(n) => {
                if path.contains(&n) {
                    path.push(n);
                    return path.join(" -> ");
                }
                path.push(n.clone());
                current = n;
            }
            None => return path.join(" -> "),
        }
    }
}
