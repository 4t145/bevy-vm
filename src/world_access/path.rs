//! 点号路径导航：在 [`ron::Value`] 树上按 `a.b.0` 定位、读、增/改、删。
//!
//! 路径是点号分隔的段序列；每段对映射是字符串键，对序列是十进制下标。空路径
//! 指向根值本身。`path_set` 在缺失的中间映射键上按需创建空映射后继续下探。
//!
//! 能力以扩展 trait [`ValuePathExt`] 暴露，仅为 [`ron::Value`] 实现。

#[cfg(test)]
mod tests;

use ron::Value;

/// 为 [`ron::Value`] 提供点号路径的读、增/改、删能力。
pub trait ValuePathExt {
    /// 读取路径处值的引用。
    ///
    /// # Errors
    ///
    /// 路径中途的值不是可下探的容器，或键/下标不存在时返回描述性错误。
    fn path_get(&self, path: &str) -> Result<&Value, String>;

    /// 在路径处写入值；按需创建缺失的中间映射键。
    ///
    /// # Errors
    ///
    /// 路径中途的值不是映射，或试图按下标写入越界/非序列时返回描述性错误。
    fn path_set(&mut self, path: &str, value: Value) -> Result<(), String>;

    /// 删除路径处的值。
    ///
    /// # Errors
    ///
    /// 父路径不存在或不是容器、或试图删除根值时返回描述性错误。
    fn path_remove(&mut self, path: &str) -> Result<(), String>;
}

impl ValuePathExt for Value {
    fn path_get(&self, path: &str) -> Result<&Value, String> {
        let mut current = self;
        for segment in segments(path) {
            current = step(current, segment)?;
        }
        Ok(current)
    }

    fn path_set(&mut self, path: &str, value: Value) -> Result<(), String> {
        let parts: Vec<&str> = segments(path).collect();
        let Some((last, parents)) = parts.split_last() else {
            *self = value;
            return Ok(());
        };
        let mut current = self;
        for segment in parents {
            current = step_mut_or_create(current, segment)?;
        }
        assign(current, last, value)
    }

    fn path_remove(&mut self, path: &str) -> Result<(), String> {
        let parts: Vec<&str> = segments(path).collect();
        let Some((last, parents)) = parts.split_last() else {
            return Err("不能删除根值".to_owned());
        };
        let mut current = self;
        for segment in parents {
            current = step_mut(current, segment)?;
        }
        delete(current, last)
    }
}

/// 把路径拆成段；空路径产出空序列。
fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('.').filter(|segment| !segment.is_empty())
}

/// 只读下探一段。
fn step<'v>(value: &'v Value, segment: &str) -> Result<&'v Value, String> {
    match value {
        Value::Map(map) => map
            .iter()
            .find(|(key, _)| key_matches(key, segment))
            .map(|(_, v)| v)
            .ok_or_else(|| format!("映射中不存在键 `{segment}`")),
        Value::Seq(seq) => {
            let index = parse_index(segment)?;
            seq.get(index)
                .ok_or_else(|| format!("序列下标 {index} 越界"))
        }
        _ => Err(format!("无法在非容器值上下探段 `{segment}`")),
    }
}

/// 可变下探一段（要求已存在）。
fn step_mut<'v>(value: &'v mut Value, segment: &str) -> Result<&'v mut Value, String> {
    match value {
        Value::Map(map) => map
            .iter_mut()
            .find(|(key, _)| key_matches(key, segment))
            .map(|(_, v)| v)
            .ok_or_else(|| format!("映射中不存在键 `{segment}`")),
        Value::Seq(seq) => {
            let index = parse_index(segment)?;
            let len = seq.len();
            seq.get_mut(index)
                .ok_or_else(|| format!("序列下标 {index} 越界（长度 {len}）"))
        }
        _ => Err(format!("无法在非容器值上下探段 `{segment}`")),
    }
}

/// 可变下探一段；段在映射中缺失时创建空映射占位。
fn step_mut_or_create<'v>(value: &'v mut Value, segment: &str) -> Result<&'v mut Value, String> {
    if let Value::Seq(_) = value {
        return step_mut(value, segment);
    }
    let Value::Map(map) = value else {
        return Err(format!("无法在非容器值上下探段 `{segment}`"));
    };
    let key = Value::String(segment.to_owned());
    if !map.iter().any(|(k, _)| key_matches(k, segment)) {
        map.insert(key.clone(), Value::Map(ron::Map::new()));
    }
    Ok(&mut map[&key])
}

/// 在容器的末段位置写入值。
fn assign(container: &mut Value, segment: &str, value: Value) -> Result<(), String> {
    match container {
        Value::Map(map) => {
            map.insert(Value::String(segment.to_owned()), value);
            Ok(())
        }
        Value::Seq(seq) => {
            let index = parse_index(segment)?;
            if index == seq.len() {
                seq.push(value);
            } else if index < seq.len() {
                seq[index] = value;
            } else {
                return Err(format!("序列下标 {index} 越界（长度 {}）", seq.len()));
            }
            Ok(())
        }
        _ => Err(format!("无法在非容器值上写入段 `{segment}`")),
    }
}

/// 删除容器末段位置的值。
fn delete(container: &mut Value, segment: &str) -> Result<(), String> {
    match container {
        Value::Map(map) => {
            let key = Value::String(segment.to_owned());
            map.remove(&key)
                .map(|_| ())
                .ok_or_else(|| format!("映射中不存在键 `{segment}`"))
        }
        Value::Seq(seq) => {
            let index = parse_index(segment)?;
            if index >= seq.len() {
                return Err(format!("序列下标 {index} 越界（长度 {}）", seq.len()));
            }
            seq.remove(index);
            Ok(())
        }
        _ => Err(format!("无法在非容器值上删除段 `{segment}`")),
    }
}

/// 映射键是否等于路径段（键须为字符串）。
fn key_matches(key: &Value, segment: &str) -> bool {
    matches!(key, Value::String(s) if s == segment)
}

/// 把路径段解析为序列下标。
fn parse_index(segment: &str) -> Result<usize, String> {
    segment
        .parse::<usize>()
        .map_err(|_| format!("序列下标必须是非负整数，得到 `{segment}`"))
}
