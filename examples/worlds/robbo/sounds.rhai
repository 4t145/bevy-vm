// Robbo 音效路径——所有 ogg 在 examples/assets/robbo/sounds/。
// 直接走 host fn `play_sound(path, opts)`，opts 见 attach_audio。

fn snd_ammo()     { "robbo/sounds/ammo.ogg" }
fn snd_bomb()     { "robbo/sounds/bomb.ogg" }
fn snd_burn()     { "robbo/sounds/burn.ogg" }
fn snd_capsule()  { "robbo/sounds/capsule.ogg" }
fn snd_door()     { "robbo/sounds/door.ogg" }
fn snd_key()      { "robbo/sounds/key.ogg" }
fn snd_screw()    { "robbo/sounds/screw.ogg" }
fn snd_shot()     { "robbo/sounds/shot.ogg" }
fn snd_spawn()    { "robbo/sounds/spawn.ogg" }
fn snd_teleport() { "robbo/sounds/teleport.ogg" }
fn snd_walk()     { "robbo/sounds/walk.ogg" }

// 一次性短音效——`mode: "Despawn"` 让 Bevy 在播完后自动 despawn 那个音频
// entity，不留残骸。volume 默认线性 1.0，不需要传。
fn play(path) {
    play_sound(path, #{ mode: "Despawn" });
}
