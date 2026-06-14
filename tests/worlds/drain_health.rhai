for e in query("Health") {
    set(e, "Health", "value", get(e, "Health", "value") - 1.0);
}
