// Sum the number of MouseButton events received this tick into Counter.clicks
// of every Counter-bearing entity.
let new_clicks = events("MouseButton").len();
if new_clicks > 0 {
    for e in query("Counter") {
        let prev = get(e, "Counter", "clicks");
        set(e, "Counter", "clicks", prev + new_clicks);
    }
}
