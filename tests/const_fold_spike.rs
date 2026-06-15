//! Spike: validate Rhai's `on_parse_token` (internals feature) can rewrite
//! `load_config("path")` → `load_config(handle_int)` so that calls become
//! cheap integer lookups at runtime.
//!
//! 流程：
//! - mapper 维护小状态机：见到 `Identifier("load_config")` → `LeftParen`
//!   → `StringConstant(path)` 时把第三个 token 改写成 `IntegerConstant(id)`
//! - host fn `load_config(int) -> int` 是 identity
//! - 编译 + eval 一段脚本，验证替换后能跑、拿到的整数对得上预期 handle id

#[allow(deprecated)]
mod spike {
    use rhai::{Engine, INT, Token};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct ConstFoldState {
        stage: ParseStage,
        next_id: i64,
        // path → handle id
        paths: Vec<String>,
    }

    #[derive(Default, Clone, Copy, PartialEq, Eq)]
    enum ParseStage {
        #[default]
        Idle,
        SeenIdent,
        SeenParen,
    }

    #[test]
    fn rewrite_load_config_string_to_int() {
        let state = Rc::new(RefCell::new(ConstFoldState::default()));
        let mut engine = Engine::new();

        let state_for_mapper = Rc::clone(&state);
        engine.on_parse_token(move |token, _, _| {
            let mut s = state_for_mapper.borrow_mut();
            match (s.stage, &token) {
                (ParseStage::Idle, Token::Identifier(name)) if &**name == "load_config" => {
                    s.stage = ParseStage::SeenIdent;
                    token
                }
                (ParseStage::SeenIdent, Token::LeftParen) => {
                    s.stage = ParseStage::SeenParen;
                    token
                }
                (ParseStage::SeenParen, Token::StringConstant(path)) => {
                    let id = s.next_id;
                    s.next_id += 1;
                    s.paths.push(path.to_string());
                    s.stage = ParseStage::Idle;
                    Token::IntegerConstant(id as INT)
                }
                _ => {
                    // 任何打破序列的 token 把状态机重置——但 LeftParen 之后
                    // 出现非 StringConstant 也复位（如 `load_config(some_var)`
                    // 这种动态情况，让它走原路）。
                    s.stage = ParseStage::Idle;
                    token
                }
            }
        });

        // load_config 现在接受 int 直接返回——const fold 后调用即 noop。
        engine.register_fn("load_config", |id: i64| id);

        let script = r#"
            let a = load_config("levels/01.json");
            let b = load_config("levels/02.json");
            let c = load_config("levels/01.json"); // 同 path 不复用——本 spike 简单分配
            [a, b, c]
        "#;

        let result: rhai::Array = engine.eval(script).expect("eval");
        let nums: Vec<i64> = result
            .into_iter()
            .map(|d| d.as_int().expect("int"))
            .collect();
        assert_eq!(nums, vec![0, 1, 2]);

        let s = state.borrow();
        assert_eq!(
            s.paths,
            vec![
                "levels/01.json".to_string(),
                "levels/02.json".to_string(),
                "levels/01.json".to_string(),
            ]
        );
    }

    /// 验证不被替换的字符串（不出现在 load_config 参数位）原样保留。
    #[test]
    fn unrelated_strings_pass_through() {
        let state = Rc::new(RefCell::new(ConstFoldState::default()));
        let mut engine = Engine::new();

        let state_for_mapper = Rc::clone(&state);
        engine.on_parse_token(move |token, _, _| {
            let mut s = state_for_mapper.borrow_mut();
            match (s.stage, &token) {
                (ParseStage::Idle, Token::Identifier(name)) if &**name == "load_config" => {
                    s.stage = ParseStage::SeenIdent;
                    token
                }
                (ParseStage::SeenIdent, Token::LeftParen) => {
                    s.stage = ParseStage::SeenParen;
                    token
                }
                (ParseStage::SeenParen, Token::StringConstant(_)) => {
                    let id = s.next_id;
                    s.next_id += 1;
                    s.stage = ParseStage::Idle;
                    Token::IntegerConstant(id as INT)
                }
                _ => {
                    s.stage = ParseStage::Idle;
                    token
                }
            }
        });

        engine.register_fn("load_config", |id: i64| id);

        // `print` 收到的应该是字符串原样。
        let script = r#"
            let cfg = load_config("levels/01.json");
            let label = "hello world";
            label.len()
        "#;

        let result: i64 = engine.eval(script).expect("eval");
        assert_eq!(result, 11);
    }
}
