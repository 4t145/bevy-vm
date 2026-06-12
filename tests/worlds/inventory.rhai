// 乙：物品是独立实体，背包存其 id 列表。
// 创建两个物品实体，把 id 存进背包，再销毁其中坏掉的那个。
for owner in query("Inventory") {
    let sword = spawn_entity();
    set(sword, "Item", "kind", "sword");
    set(sword, "Item", "durability", 0.8);

    let potion = spawn_entity();
    set(potion, "Item", "kind", "potion");
    set(potion, "Item", "durability", 0.0);

    set(owner, "Inventory", "slots", [sword, potion]);

    // 遍历背包，销毁耐久耗尽的物品实体。
    let slots = get(owner, "Inventory", "slots");
    for item_id in slots {
        if get(item_id, "Item", "durability") <= 0.0 {
            despawn(item_id);
        }
    }
}
