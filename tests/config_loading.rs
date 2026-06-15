//! End-to-end config loading：脚本里 `load_config("file")` 走 const-fold +
//! ConfigCache，运行时拿到的 handle 可被 `config_get` / `config_len` /
//! `config_keys` 路径访问。

#![cfg(feature = "bevy-bridge")]

use bevy_ecs::world::World;
use bevy_vm::VmInstance;
use std::fs;
use std::path::PathBuf;

fn fixture_world(world_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("bevy_vm_config_e2e")
        .join(world_name);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_world_files(dir: &std::path::Path, world_ron: &str, scripts: &[(&str, &str)]) {
    fs::write(dir.join("world.ron"), world_ron).unwrap();
    for (name, body) in scripts {
        fs::write(dir.join(name), body).unwrap();
    }
}

#[test]
fn script_loads_json_and_reads_nested_paths() {
    let dir = fixture_world("simple_json");

    fs::write(
        dir.join("data.json"),
        r#"{
            "name": "level-one",
            "size": 16,
            "tiles": ["wall", "floor", "exit"]
        }"#,
    )
    .unwrap();

    write_world_files(
        &dir,
        r#"(
            components: [
                (name: "Probe", default: (
                    name: "",
                    size: 0,
                    third_tile: "",
                    n_tiles: 0,
                )),
            ],
            entities: [
                (components: { "Probe": () }),
            ],
            systems: [
                Script(path: "probe.rhai"),
            ],
        )"#,
        &[(
            "probe.rhai",
            r#"
                let cfg = load_config("data.json");
                for e in query("Probe") {
                    set(e, "Probe", "name", config_get(cfg, "name"));
                    set(e, "Probe", "size", config_get(cfg, "size"));
                    set(e, "Probe", "third_tile", config_get(cfg, "tiles.2"));
                    set(e, "Probe", "n_tiles", config_len(cfg, "tiles"));
                }
            "#,
        )],
    );

    let mut world = World::new();
    let mut vm = VmInstance::load(&mut world, &dir).expect("loads");
    vm.tick(&mut world).expect("tick");

    let probes = vm.query(&mut world, "Probe");
    assert_eq!(probes.len(), 1);
    let probe = probes[0];
    let name = vm.get(&world, probe, "Probe", "name").unwrap();
    let size = vm.get(&world, probe, "Probe", "size").unwrap();
    let third = vm.get(&world, probe, "Probe", "third_tile").unwrap();
    let n = vm.get(&world, probe, "Probe", "n_tiles").unwrap();

    assert_eq!(name.as_str().unwrap(), "level-one");
    assert_eq!(size.as_i64().unwrap(), 16);
    assert_eq!(third.as_str().unwrap(), "exit");
    assert_eq!(n.as_i64().unwrap(), 3);
}

#[test]
fn same_path_dedupes_to_same_handle() {
    let dir = fixture_world("dedup");
    fs::write(dir.join("d.json"), r#"{"x": 7}"#).unwrap();

    write_world_files(
        &dir,
        r#"(
            components: [
                (name: "Hits", default: (h1: 0, h2: 0)),
            ],
            entities: [
                (components: { "Hits": () }),
            ],
            systems: [
                Script(path: "p.rhai"),
            ],
        )"#,
        &[(
            "p.rhai",
            r#"
                let h1 = load_config("d.json");
                let h2 = load_config("d.json");
                for e in query("Hits") {
                    set(e, "Hits", "h1", h1);
                    set(e, "Hits", "h2", h2);
                }
            "#,
        )],
    );

    let mut world = World::new();
    let mut vm = VmInstance::load(&mut world, &dir).expect("loads");
    vm.tick(&mut world).expect("tick");

    let hits = vm.query(&mut world, "Hits");
    let e = hits[0];
    let h1 = vm.get(&world, e, "Hits", "h1").unwrap().as_i64().unwrap();
    let h2 = vm.get(&world, e, "Hits", "h2").unwrap().as_i64().unwrap();
    assert_eq!(h1, h2, "same path resolves to same cache handle");
}

#[test]
fn unknown_extension_errors() {
    let dir = fixture_world("bad_ext");
    fs::write(dir.join("data.txt"), "raw text").unwrap();

    write_world_files(
        &dir,
        r#"(
            components: [(name: "X", default: ())],
            entities: [(components: { "X": () })],
            systems: [Script(path: "p.rhai")],
        )"#,
        &[("p.rhai", r#"let cfg = load_config("data.txt");"#)],
    );

    let mut world = World::new();
    let mut vm = VmInstance::load(&mut world, &dir).expect("loads");
    let err = vm.tick(&mut world).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unsupported config extension")
            || msg.contains("only .json / .ron"),
        "unexpected error: {msg}"
    );
}
