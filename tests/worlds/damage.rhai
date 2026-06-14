// Consume incoming Hit events (typed): subtract amount from each entity's Health.
// Emit a Damaged event (dynamic) for every successful hit so external observers
// can react.
for hit in events("Hit") {
    let target = hit.target;
    let amount = hit.amount;
    let hp = get(target, "Health", "value") - amount;
    set(target, "Health", "value", hp);
    emit("Damaged", #{ entity: target, amount: amount });
}
