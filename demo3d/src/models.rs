//! Loads the KayKit characters, builds one animation graph per model, and
//! drives animations on spawned characters ("rigs").

use std::collections::HashMap;
use std::time::Duration;

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use crate::common::AppState;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ModelKind {
    Knight,
    Rogue,
    Mage,
    Minion,
    SkeletonRogue,
    SkeletonMage,
    SkeletonWarrior,
}

impl ModelKind {
    const ALL: [ModelKind; 7] = [
        ModelKind::Knight,
        ModelKind::Rogue,
        ModelKind::Mage,
        ModelKind::Minion,
        ModelKind::SkeletonRogue,
        ModelKind::SkeletonMage,
        ModelKind::SkeletonWarrior,
    ];

    fn path(self) -> &'static str {
        match self {
            ModelKind::Knight => "models/Knight.glb",
            ModelKind::Rogue => "models/Rogue_Hooded.glb",
            ModelKind::Mage => "models/Mage.glb",
            ModelKind::Minion => "models/Skeleton_Minion.glb",
            ModelKind::SkeletonRogue => "models/Skeleton_Rogue.glb",
            ModelKind::SkeletonMage => "models/Skeleton_Mage.glb",
            ModelKind::SkeletonWarrior => "models/Skeleton_Warrior.glb",
        }
    }

    /// Accessory meshes to hide (the packs ship every prop on every model).
    fn hidden(self) -> &'static [&'static str] {
        match self {
            ModelKind::Knight => {
                &["1H_Sword_Offhand", "Badge_Shield", "Rectangle_Shield", "Spike_Shield", "1H_Sword", "2H_Sword"]
            }
            ModelKind::Rogue => &["Knife_Offhand", "1H_Crossbow", "2H_Crossbow", "Knife", "Throwable"],
            ModelKind::Mage => &["Spellbook", "Spellbook_open", "2H_Staff"],
            _ => &[],
        }
    }
}

/// Weapon props for the skeletons, attached to their hand bones.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Prop {
    Crossbow,
    Blade,
    Staff,
    Axe,
    Shield,
}

impl Prop {
    const ALL: [Prop; 5] = [Prop::Crossbow, Prop::Blade, Prop::Staff, Prop::Axe, Prop::Shield];

    fn path(self) -> &'static str {
        match self {
            Prop::Crossbow => "models/Skeleton_Crossbow.gltf",
            Prop::Blade => "models/Skeleton_Blade.gltf",
            Prop::Staff => "models/Skeleton_Staff.gltf",
            Prop::Axe => "models/Skeleton_Axe.gltf",
            Prop::Shield => "models/Skeleton_Shield_Large_A.gltf",
        }
    }
}

pub struct AnimSet {
    pub graph: Handle<AnimationGraph>,
    pub nodes: HashMap<String, AnimationNodeIndex>,
}

#[derive(Resource)]
pub struct Models {
    gltf: HashMap<ModelKind, Handle<Gltf>>,
    props: HashMap<Prop, Handle<Gltf>>,
    pub anims: HashMap<ModelKind, AnimSet>,
    scenes: HashMap<ModelKind, Handle<bevy::world_serialization::WorldAsset>>,
    prop_scenes: HashMap<Prop, Handle<bevy::world_serialization::WorldAsset>>,
}

/// A spawned animated character.
#[derive(Component)]
pub struct Rig {
    pub kind: ModelKind,
    /// The entity holding the AnimationPlayer, once the model has spawned.
    pub player: Option<Entity>,
    pub current: String,
    pub props: Vec<(Prop, bool)>,
    /// One-shot animation in progress and seconds left on it.
    pub oneshot: Option<(String, f32)>,
}

impl Rig {
    pub fn new(kind: ModelKind) -> Self {
        Self { kind, player: None, current: String::new(), props: Vec::new(), oneshot: None }
    }

    /// Attaches a prop to the right hand (`left == false`) or left hand.
    pub fn with_prop(mut self, prop: Prop, left: bool) -> Self {
        self.props.push((prop, left));
        self
    }

    pub fn busy(&self) -> bool {
        self.oneshot.is_some()
    }
}

pub struct ModelsPlugin;

impl Plugin for ModelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_loading)
            .add_systems(Update, finish_loading.run_if(in_state(AppState::Loading)))
            .add_systems(Update, tick_oneshots);
    }
}

fn start_loading(mut commands: Commands, assets: Res<AssetServer>) {
    let gltf = ModelKind::ALL.iter().map(|&k| (k, assets.load(k.path()))).collect();
    let props = Prop::ALL.iter().map(|&p| (p, assets.load(p.path()))).collect();
    commands.insert_resource(Models {
        gltf,
        props,
        anims: HashMap::new(),
        scenes: HashMap::new(),
        prop_scenes: HashMap::new(),
    });
}

fn finish_loading(
    mut models: ResMut<Models>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let all_loaded = models.gltf.values().all(|h| assets.is_loaded_with_dependencies(h))
        && models.props.values().all(|h| assets.is_loaded_with_dependencies(h));
    if !all_loaded {
        return;
    }
    let mut anims = HashMap::new();
    let mut scenes = HashMap::new();
    for (&kind, handle) in &models.gltf {
        let gltf = gltfs.get(handle).expect("loaded");
        let names: Vec<String> = gltf.named_animations.keys().map(|k| k.to_string()).collect();
        let (graph, indices) =
            AnimationGraph::from_clips(names.iter().map(|n| gltf.named_animations[n.as_str()].clone()));
        let nodes = names.into_iter().zip(indices).collect();
        anims.insert(kind, AnimSet { graph: graphs.add(graph), nodes });
        scenes.insert(kind, gltf.default_scene.clone().or_else(|| gltf.scenes.first().cloned()).expect("scene"));
    }
    let mut prop_scenes = HashMap::new();
    for (&p, handle) in &models.props {
        let gltf = gltfs.get(handle).expect("loaded");
        prop_scenes.insert(p, gltf.default_scene.clone().or_else(|| gltf.scenes.first().cloned()).expect("scene"));
    }
    models.anims = anims;
    models.scenes = scenes;
    models.prop_scenes = prop_scenes;
    next.set(AppState::Menu);
}

impl Models {
    /// Spawns a character model. Animations start once it has loaded.
    pub fn spawn(&self, commands: &mut Commands, rig: Rig, transform: Transform) -> Entity {
        let scene = self.scenes[&rig.kind].clone();
        commands.spawn((bevy::world_serialization::WorldAssetRoot(scene), transform, rig)).observe(on_rig_ready).id()
    }
}

fn on_rig_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    models: Res<Models>,
    children: Query<&Children>,
    names: Query<&Name>,
    players: Query<(), With<AnimationPlayer>>,
    mut rigs: Query<&mut Rig>,
) {
    let root = ready.entity;
    let Ok(mut rig) = rigs.get_mut(root) else { return };
    let hidden = rig.kind.hidden();
    for e in children.iter_descendants(root) {
        if players.contains(e) && rig.player.is_none() {
            let set = &models.anims[&rig.kind];
            commands.entity(e).insert((AnimationGraphHandle(set.graph.clone()), AnimationTransitions::new()));
            rig.player = Some(e);
        }
        if let Ok(name) = names.get(e) {
            if hidden.contains(&name.as_str()) {
                commands.entity(e).insert(Visibility::Hidden);
            }
            for &(prop, left) in &rig.props {
                let slot = if left { "handslot.l" } else { "handslot.r" };
                if name.as_str() == slot {
                    let child = commands
                        .spawn((
                            bevy::world_serialization::WorldAssetRoot(models.prop_scenes[&prop].clone()),
                            Transform::IDENTITY,
                        ))
                        .id();
                    commands.entity(e).add_child(child);
                }
            }
        }
    }
}

/// Plays an animation on a rig. Looping animations don't restart if already
/// playing; one-shots play once and the rig reports `busy()` until done.
pub fn play(
    rig: &mut Rig,
    models: &Models,
    players: &mut Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    name: &str,
    looping: bool,
    speed: f32,
) {
    let Some(pe) = rig.player else { return };
    let Ok((mut player, mut transitions)) = players.get_mut(pe) else { return };
    let Some(&node) = models.anims[&rig.kind].nodes.get(name) else {
        warn!("{:?} has no animation {name}", rig.kind);
        return;
    };
    if looping && rig.current == name && rig.oneshot.is_none() {
        if let Some(a) = player.animation_mut(node) {
            a.set_speed(speed);
        }
        return;
    }
    let fade = if looping { 180 } else { 90 };
    let active = transitions.play(&mut player, node, Duration::from_millis(fade));
    active.set_speed(speed);
    if looping {
        active.repeat();
        rig.oneshot = None;
    } else {
        active.replay();
        let len = clip_len(name) / speed.max(0.01);
        rig.oneshot = Some((name.to_string(), len));
    }
    rig.current = name.to_string();
}

/// Freezes or resumes a rig's animation (for Frozen / Suspended).
pub fn set_paused(rig: &Rig, players: &mut Query<(&mut AnimationPlayer, &mut AnimationTransitions)>, paused: bool) {
    let Some(pe) = rig.player else { return };
    let Ok((mut player, _)) = players.get_mut(pe) else { return };
    if paused {
        player.pause_all();
    } else {
        player.resume_all();
    }
}

/// Approximate clip lengths in seconds, for knowing when one-shots end.
fn clip_len(name: &str) -> f32 {
    match name {
        "Spawn_Ground_Skeletons" => 1.9,
        "Death_C_Skeletons" | "Death_A" => 1.6,
        "Hit_A" => 0.5,
        "Unarmed_Melee_Attack_Punch_A" => 0.7,
        "1H_Melee_Attack_Chop" | "2H_Melee_Attack_Chop" => 0.9,
        "2H_Melee_Attack_Spin" => 1.0,
        "1H_Ranged_Shoot" => 0.6,
        "Spellcast_Shoot" => 0.9,
        "Spellcast_Long" => 1.6,
        "Spellcast_Raise" => 1.0,
        "Throw" => 0.8,
        "Dodge_Forward" => 0.7,
        "Taunt" => 1.8,
        "Cheer" => 1.5,
        _ => 0.8,
    }
}

fn tick_oneshots(time: Res<Time>, mut rigs: Query<&mut Rig>) {
    for mut rig in &mut rigs {
        if let Some((_, t)) = &mut rig.oneshot {
            *t -= time.delta_secs();
            if *t <= 0.0 {
                rig.oneshot = None;
            }
        }
    }
}
