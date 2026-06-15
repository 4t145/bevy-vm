//! Convert original.txt → levels.json for the Robbo demo.
//!
//! 用法：`cargo run --bin robbo_levels` —— 把
//! `examples/assets/robbo/original.txt` 转成
//! `examples/worlds/robbo/levels.json`，结构：
//! ```json
//! { "name": "Original", "count": 58,
//!   "levels": [
//!     { "level": 1, "size": [16, 31], "colour": "996600",
//!       "rows": ["QQQQ...", "Q....Q...", ...] }
//!   ]
//! }
//! ```
//! 关卡数据保留为 row 字符串列表——脚本端 `config_get(cfg, "levels.0.rows.5")`
//! 取一行；逐字符 spawn tile / actor。

use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "no parent dir".to_owned())?
        .to_path_buf();
    let input = root.join("examples/assets/robbo/original.txt");
    let output_dir = root.join("examples/worlds/robbo");
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    let output = output_dir.join("levels.json");

    let text = fs::read_to_string(&input).map_err(|e| format!("read {input:?}: {e}"))?;

    let pack = parse_pack(&text)?;
    let json = serde_json::to_string_pretty(&pack).map_err(|e| e.to_string())?;
    fs::write(&output, json).map_err(|e| format!("write {output:?}: {e}"))?;

    println!(
        "wrote {} levels → {}",
        pack.levels.len(),
        output.display()
    );
    Ok(())
}

#[derive(serde::Serialize)]
struct LevelPack {
    name: String,
    count: usize,
    levels: Vec<Level>,
}

#[derive(serde::Serialize)]
struct Level {
    level: u32,
    size: [usize; 2],
    colour: String,
    rows: Vec<String>,
}

fn parse_pack(text: &str) -> Result<LevelPack, String> {
    let mut name = String::from("Original");
    let mut levels = Vec::<Level>::new();

    let mut iter = text.lines().peekable();
    while let Some(line) = iter.next() {
        let line = line.trim_end();
        match line {
            "[name]" => {
                if let Some(n) = iter.next() {
                    name = n.trim().to_owned();
                }
            }
            "[level]" => {
                let level_no: u32 = iter
                    .next()
                    .ok_or("missing level number")?
                    .trim()
                    .parse()
                    .map_err(|e| format!("bad level number: {e}"))?;
                let mut colour = String::from("608050");
                let mut size = [16usize, 31usize];
                let mut rows = Vec::<String>::new();

                loop {
                    let Some(next_line) = iter.peek() else {
                        break;
                    };
                    let trimmed = next_line.trim_end();
                    if trimmed == "[level]" {
                        break;
                    }
                    let Some(consumed) = iter.next() else {
                        break;
                    };
                    let trimmed = consumed.trim_end();
                    match trimmed {
                        "[colour]" => {
                            if let Some(c) = iter.next() {
                                colour = c.trim().to_owned();
                            }
                        }
                        "[size]" => {
                            if let Some(s) = iter.next() {
                                let parts: Vec<&str> = s.trim().split('.').collect();
                                if parts.len() == 2 {
                                    if let (Ok(w), Ok(h)) =
                                        (parts[0].parse(), parts[1].parse())
                                    {
                                        size = [w, h];
                                    }
                                }
                            }
                        }
                        "[data]" => {
                            // 之后连续的 width-长度 行都是棋盘行；遇到下一个
                            // [...] section header 或空行结束。
                            while let Some(peek) = iter.peek() {
                                let p = peek.trim_end();
                                if p.starts_with('[') {
                                    break;
                                }
                                let consumed = iter.next().unwrap().trim_end().to_owned();
                                if consumed.is_empty() {
                                    continue;
                                }
                                rows.push(consumed);
                            }
                        }
                        // 其他 section（[author]、[level_notes]、[forever]、
                        // [solution]）我们用不到——跳过它们的 body 直到下一个 header。
                        s if s.starts_with('[') && s.ends_with(']') => {
                            while let Some(peek) = iter.peek() {
                                if peek.trim_end().starts_with('[') {
                                    break;
                                }
                                iter.next();
                            }
                        }
                        _ => {}
                    }
                }

                if rows.len() != size[1] {
                    eprintln!(
                        "warn: level {level_no} declared height {} but got {} rows",
                        size[1],
                        rows.len()
                    );
                }
                levels.push(Level {
                    level: level_no,
                    size,
                    colour,
                    rows,
                });
            }
            _ => {}
        }
    }

    Ok(LevelPack {
        count: levels.len(),
        name,
        levels,
    })
}
