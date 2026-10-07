#!/usr/bin/env sh
# Copies the KayKit character models (CC0) into demo3d/assets, keeping only
# the animations the game uses so the browser download stays small.
#
#   GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1 https://github.com/KayKit-Game-Assets/kaykit-character-pack-adventures-1.0 /tmp/kk-adventurers
#   GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1 https://github.com/KayKit-Game-Assets/kaykit-character-pack-skeletons-1.0 /tmp/kk-skeletons
#   demo3d/tools/prepare_assets.sh /tmp/kk-adventurers /tmp/kk-skeletons
set -e
ADV="$1/addons/kaykit_character_pack_adventures"
SKE="$2/addons/kaykit_character_pack_skeletons"
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/../assets/models"
mkdir -p "$OUT"
prune() { python3 -I "$HERE/prune_glb.py" "$@"; }

GUARDIAN="Idle Running_A Running_Strafe_Left Running_Strafe_Right Walking_Backwards Jump_Idle
1H_Ranged_Shoot 1H_Melee_Attack_Chop 2H_Melee_Attack_Spin 2H_Melee_Attack_Chop Unarmed_Melee_Attack_Punch_A
Spellcast_Shoot Spellcast_Long Spellcast_Raise Throw Dodge_Forward Block Hit_A Death_A Cheer"
for c in Knight Rogue_Hooded Mage; do
  prune "$ADV/Characters/gltf/$c.glb" "$OUT/$c.glb" $GUARDIAN
done

SKELETON="Idle_Combat Running_A Walking_D_Skeletons Unarmed_Melee_Attack_Punch_A 1H_Melee_Attack_Chop
2H_Melee_Attack_Spin 1H_Ranged_Shoot Spellcast_Shoot Hit_A Death_C_Skeletons Spawn_Ground_Skeletons Taunt"
for c in Skeleton_Minion Skeleton_Rogue Skeleton_Mage Skeleton_Warrior; do
  prune "$SKE/Characters/gltf/$c.glb" "$OUT/$c.glb" $SKELETON
done

for w in Skeleton_Crossbow Skeleton_Blade Skeleton_Staff Skeleton_Axe Skeleton_Shield_Large_A; do
  cp "$SKE/Assets/gltf/$w.gltf" "$SKE/Assets/gltf/$w.bin" "$OUT/"
done
cp "$SKE/Assets/gltf/skeleton_texture.png" "$OUT/"
cp "$1/LICENSE.txt" "$OUT/KayKit-Adventurers-LICENSE.txt"
cp "$2/LICENSE.txt" "$OUT/KayKit-Skeletons-LICENSE.txt"
ls -la "$OUT"
