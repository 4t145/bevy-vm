// 每个实体若 Health.value > 5 则扣 2 血并进入 fighting，否则进入 fleeing。
// state 是运行时长出的新键，展示动态组件可自由扩展形状。
for e in query("Health") {
    let hp = get(e, "Health", "value");
    if hp > 5.0 {
        set(e, "Health", "value", hp - 2.0);
        set(e, "Health", "state", "fighting");
    } else {
        set(e, "Health", "state", "fleeing");
    }
}
