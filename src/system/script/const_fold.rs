//! Token-level const folding for designated host fns.
//!
//! 思路：标记一组"folding-eligible" host fn 名（如 `load_config`、未来的
//! `query` / `events` 等）。脚本里只要写成 `fn_name("literal")` 这种**直接
//! 字面量**形式，编译期 token mapper 就把字符串参数替换成 `IntegerConstant`，
//! 该整数是注册到 [`ConstFoldRegistry`] 的句柄。host fn 收到 int 直接返回，
//! 调用退化成"取常量整数 + 一次 noop call"。
//!
//! 参数为变量 / 表达式（如 `load_config(some_var)`）走原路径，host fn 仍
//! 需接受字符串参数——折叠是优化、不是强制。
//!
//! ## 当前覆盖
//!
//! - `load_config(path)` — 唯一已开通的 fold 入口。后续把
//!   `query("Component")` / `events("Channel")` 加入只需扩展
//!   [`ConstFoldRegistry::register`]。
//!
//! ## 不变式
//!
//! - 状态机重置：任何打破 `Identifier(<eligible>)` → `LeftParen` →
//!   `StringConstant` 序列的 token 立即把状态机回 `Idle`，避免误伤无关
//!   字符串。
//! - 返回的 handle id 不复用——同 path 多次出现各分配一个 id；运行时
//!   `load_config` 的 cache 在 [`crate::resource::config`] 那层做。

use rhai::{INT, Token};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// 一类 fold-eligible host fn 的注册信息：分配过的字面量列表。
///
/// `entries[i]` 是第 i 次出现的字面量；id 即下标。同一字面量重复出现
/// 各占一项——不在此层去重，让运行时按 path 在 [`crate::resource::config::ConfigCache`]
/// 这类 cache 里去重。
#[derive(Default)]
struct FoldedFn {
    entries: Vec<String>,
}

impl FoldedFn {
    fn alloc(&mut self, literal: String) -> i64 {
        let id = self.entries.len() as i64;
        self.entries.push(literal);
        id
    }
}

/// 编译期 token mapper 与运行时 host fn 共享的常量池。
///
/// `Rc<RefCell<...>>` 包装让它能：
/// 1. 被 `on_parse_token` 闭包捕获并写入（每帧只在 compile 时跑一次）
/// 2. 被运行时 host fn 闭包捕获并读取（用 id 反查 path）
///
/// 单线程 single-VM 不变式保证不会同时读写。
#[derive(Default)]
pub struct ConstFoldRegistry {
    fns: HashMap<String, FoldedFn>,
}

impl ConstFoldRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 把 `name` 加入 fold-eligible 列表——之后该 fn 的字面量字符串参数
    /// 会被 token mapper 自动替换成整数句柄。同名重复注册无害（idempotent）。
    pub fn register(&mut self, name: &str) {
        self.fns.entry(name.to_owned()).or_default();
    }

    /// 是否已注册过 fold 名 `name`。
    #[must_use]
    pub fn is_eligible(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }

    /// 给 `(fn_name, literal)` 分配 / 复用一个 handle id。
    ///
    /// 当前实现**每次出现都新分配**——对应运行时 cache 由调用方做（如
    /// `load_config` 内部按 path 复用同一 cache slot）。这样保留 fold 后
    /// 的整数语义"每次调用 = 一次取常量"，不需要 token mapper 自带 dedup。
    fn alloc(&mut self, fn_name: &str, literal: String) -> Option<i64> {
        self.fns.get_mut(fn_name).map(|f| f.alloc(literal))
    }

    /// 反查 `(fn_name, id)` 对应的字面量。
    #[must_use]
    pub fn lookup(&self, fn_name: &str, id: i64) -> Option<&str> {
        self.fns
            .get(fn_name)
            .and_then(|f| f.entries.get(id as usize))
            .map(String::as_str)
    }
}

/// Token mapper 的小状态机阶段。
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    /// 上一个 token 是已注册的 fold-eligible identifier；下一个期待 LeftParen。
    SeenIdent,
    /// 之前是 Ident + LeftParen；下一个期待 StringConstant 或任意 token。
    SeenParen,
}

/// 把 [`ConstFoldRegistry`] 串成一个 `on_parse_token` 闭包返回。
///
/// 闭包内部维护状态机 + 持有 registry 的 `Rc`——后续 host fn 用同一份
/// `Rc` 反查 path。
pub fn build_token_mapper(
    registry: Rc<RefCell<ConstFoldRegistry>>,
) -> impl Fn(Token, rhai::Position, &rhai::TokenizeState) -> Token + 'static {
    let stage = RefCell::new(Stage::Idle);
    let pending_fn = RefCell::new(String::new());

    move |token, _pos, _state| {
        let mut current = stage.borrow_mut();
        match (*current, &token) {
            (Stage::Idle, Token::Identifier(name)) => {
                let reg = registry.borrow();
                if reg.is_eligible(name) {
                    *current = Stage::SeenIdent;
                    *pending_fn.borrow_mut() = name.to_string();
                }
                token
            }
            (Stage::SeenIdent, Token::LeftParen) => {
                *current = Stage::SeenParen;
                token
            }
            (Stage::SeenParen, Token::StringConstant(s)) => {
                let fn_name = pending_fn.borrow().clone();
                *current = Stage::Idle;
                let mut reg = registry.borrow_mut();
                if let Some(id) = reg.alloc(&fn_name, s.to_string()) {
                    Token::IntegerConstant(id as INT)
                } else {
                    token
                }
            }
            _ => {
                *current = Stage::Idle;
                token
            }
        }
    }
}
