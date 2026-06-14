// 共享 helper module。被 hud.rhai 通过 `import "helpers" as h;` 引入。
// 演示：helper 函数可被多个 plugin 脚本 import。

fn increment() {
    1
}
