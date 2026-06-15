//! Script-readable config files.
//!
//! 脚本可调 `load_config("relative/path.json")` 拿到一个整数句柄；后续
//! `config_get(handle, "path.to.field")` 读取嵌套值。
//!
//! - **格式**：按扩展名分发——`.json` → `serde_json`；`.ron` →
//!   `ron::from_str`。其他扩展名拒绝（要求结构化数据）。
//! - **路径根**：脚本当前 module 的 `base_dir`（通常等于该 module 的目录）。
//! - **缓存**：[`ConfigCache`] 是 VM-scoped 资源，按 `(VmId, path)`
//!   去重；同一 path 第二次 `load_config` 复用同一 [`ConfigHandle`]，
//!   不重新读盘解析。
//!
//! 本模块**不依赖 Bevy**——纯 std + serde。即使 headless 跑也能用。

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// 加载 / 访问配置时可能出现的错误。
#[derive(Debug, Error)]
pub enum ConfigError {
    /// 不支持的扩展名（必须是 `.json` 或 `.ron`）。
    #[error("unsupported config extension `{ext}` for `{path}` — only .json / .ron allowed")]
    UnsupportedExtension {
        /// 失败的扩展名（不含点）。
        ext: String,
        /// 失败的路径。
        path: String,
    },

    /// 文件读取失败（不存在 / 权限等）。
    #[error("failed to read config `{path}`: {source}")]
    Io {
        /// 失败的路径。
        path: String,
        /// 底层 IO 错误。
        #[source]
        source: std::io::Error,
    },

    /// 解析失败（JSON / RON 语法错）。
    #[error("failed to parse config `{path}`: {reason}")]
    Parse {
        /// 失败的路径。
        path: String,
        /// 底层 parser 错误信息。
        reason: String,
    },

    /// 通过句柄查找时找不到——通常意味着脚本作弊地传了 const fold 之外的
    /// id（比如手算的整数）。
    #[error("config handle {handle} not found in cache")]
    UnknownHandle {
        /// 出错的句柄值。
        handle: i64,
    },

    /// 字段路径在 Value 树里走不通（不存在的 key、越界 index 等）。
    #[error("config path `{path}` cannot be resolved against `{root}`")]
    PathMiss {
        /// 失败的字段路径。
        path: String,
        /// 用作根的配置文件路径（用于诊断）。
        root: String,
    },
}

/// 一个已加载的配置 = (源路径，解析后的 [`Value`])。
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    source: PathBuf,
    root: Value,
}

impl LoadedConfig {
    /// 此条配置的来源路径（绝对，便于诊断）。
    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// 配置根 Value 的引用。
    #[must_use]
    pub fn root(&self) -> &Value {
        &self.root
    }

    /// 按 `path` 在 Value 树里取值。`path` 是 `.` 分隔的 key / index 序列，
    /// 空 path 返回根。例：`"levels.0.tiles"`。
    ///
    /// # Errors
    ///
    /// 路径走不通时返回 [`ConfigError::PathMiss`]。
    pub fn get(&self, path: &str) -> Result<&Value, ConfigError> {
        if path.is_empty() {
            return Ok(&self.root);
        }
        let mut current = &self.root;
        for segment in path.split('.') {
            current = match current {
                Value::Object(map) => map.get(segment).ok_or_else(|| ConfigError::PathMiss {
                    path: path.to_owned(),
                    root: self.source.display().to_string(),
                })?,
                Value::Array(arr) => {
                    let index: usize = segment.parse().map_err(|_| ConfigError::PathMiss {
                        path: path.to_owned(),
                        root: self.source.display().to_string(),
                    })?;
                    arr.get(index).ok_or_else(|| ConfigError::PathMiss {
                        path: path.to_owned(),
                        root: self.source.display().to_string(),
                    })?
                }
                _ => {
                    return Err(ConfigError::PathMiss {
                        path: path.to_owned(),
                        root: self.source.display().to_string(),
                    });
                }
            };
        }
        Ok(current)
    }
}

/// VM-scoped 资源：handle id → LoadedConfig。
///
/// id 从 0 开始单调递增，与 [`crate::system::script::const_fold`] 的
/// fold-eligible 调用 `load_config` 的字面量分配保持一致——脚本编译时
/// 第 i 次见到 `load_config("path")` 就分配 id i，运行时 host fn 拿到同
/// 一个 i 反查 cache。
#[derive(Default)]
pub struct ConfigCache {
    entries: Vec<LoadedConfig>,
    by_path: HashMap<PathBuf, usize>,
}

impl ConfigCache {
    /// 空 cache。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 加载 `relative_path`（相对 `base_dir`），返回它的句柄 id。同一
    /// 绝对路径第二次加载复用第一次的句柄。
    ///
    /// # Errors
    ///
    /// 见 [`ConfigError`] —— 扩展名 / IO / 解析失败都向上传播。
    pub fn ensure_loaded(
        &mut self,
        base_dir: &Path,
        relative_path: &str,
    ) -> Result<usize, ConfigError> {
        let absolute = canonicalize_for_cache(base_dir, relative_path);
        if let Some(&id) = self.by_path.get(&absolute) {
            return Ok(id);
        }
        let loaded = parse_file(&absolute, relative_path)?;
        let id = self.entries.len();
        self.entries.push(loaded);
        self.by_path.insert(absolute, id);
        Ok(id)
    }

    /// 按 id 取已加载的配置。`id` 来自 [`Self::ensure_loaded`] 或 const-fold
    /// 后注入的整数。
    ///
    /// # Errors
    ///
    /// 返回 [`ConfigError::UnknownHandle`] 当 id 越界。
    pub fn get(&self, id: i64) -> Result<&LoadedConfig, ConfigError> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.entries.get(i))
            .ok_or(ConfigError::UnknownHandle { handle: id })
    }

    /// 已加载的配置数量——主要供调试。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否还没加载任何配置。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn canonicalize_for_cache(base_dir: &Path, relative_path: &str) -> PathBuf {
    let absolute = base_dir.join(relative_path);
    // 不强制 canonicalize（fs 调用 + 文件可能不存在）；直接 normalize 路径
    // 字符串足以让相同写法命中缓存。后续真要 canonicalize 再说。
    absolute
}

fn parse_file(absolute: &Path, original: &str) -> Result<LoadedConfig, ConfigError> {
    let text = std::fs::read_to_string(absolute).map_err(|e| ConfigError::Io {
        path: original.to_owned(),
        source: e,
    })?;
    let ext = absolute
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let root: Value = match ext.as_str() {
        "json" => serde_json::from_str(&text).map_err(|e| ConfigError::Parse {
            path: original.to_owned(),
            reason: e.to_string(),
        })?,
        "ron" => {
            // RON parsing → serde_json::Value 走 ron::from_str + serde 桥接。
            ron::from_str(&text).map_err(|e| ConfigError::Parse {
                path: original.to_owned(),
                reason: e.to_string(),
            })?
        }
        _ => {
            return Err(ConfigError::UnsupportedExtension {
                ext,
                path: original.to_owned(),
            });
        }
    };
    Ok(LoadedConfig {
        source: absolute.to_path_buf(),
        root,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir() -> PathBuf {
        let dir = std::env::temp_dir().join("bevy_vm_config_test");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_json_and_dedups_paths() {
        let dir = fixture_dir();
        let path = dir.join("a.json");
        std::fs::write(&path, r#"{"name":"hi","scores":[1,2,3]}"#).unwrap();

        let mut cache = ConfigCache::new();
        let id1 = cache.ensure_loaded(&dir, "a.json").unwrap();
        let id2 = cache.ensure_loaded(&dir, "a.json").unwrap();
        assert_eq!(id1, id2, "second load reuses cached handle");

        let cfg = cache.get(id1 as i64).unwrap();
        assert_eq!(cfg.get("name").unwrap(), &Value::String("hi".to_owned()));
        assert_eq!(cfg.get("scores.1").unwrap(), &Value::Number(2.into()));
    }

    #[test]
    fn rejects_unknown_extension() {
        let dir = fixture_dir();
        let path = dir.join("a.txt");
        std::fs::write(&path, "raw").unwrap();

        let mut cache = ConfigCache::new();
        let err = cache.ensure_loaded(&dir, "a.txt").unwrap_err();
        assert!(matches!(err, ConfigError::UnsupportedExtension { .. }));
    }

    #[test]
    fn path_miss_returns_error() {
        let dir = fixture_dir();
        let path = dir.join("b.json");
        std::fs::write(&path, r#"{"a":1}"#).unwrap();

        let mut cache = ConfigCache::new();
        let id = cache.ensure_loaded(&dir, "b.json").unwrap();
        let cfg = cache.get(id as i64).unwrap();
        assert!(matches!(
            cfg.get("missing").unwrap_err(),
            ConfigError::PathMiss { .. }
        ));
    }
}
