#!/usr/bin/env python3
"""Copy a .glb keeping only the named animations, dropping unused buffer data.

    prune_glb.py IN.glb OUT.glb Idle Running_A Death_A ...

Meshes, skins, nodes and images are kept as they are; only animations not
listed (and the accessor/buffer data only they used) are removed.
"""
import json
import struct
import sys


def read_glb(path):
    data = open(path, "rb").read()
    magic, version, _ = struct.unpack("<4sII", data[:12])
    assert magic == b"glTF" and version == 2, "not a glTF 2 binary"
    off, doc, binary = 12, None, b""
    while off < len(data):
        length, kind = struct.unpack("<I4s", data[off:off + 8])
        chunk = data[off + 8:off + 8 + length]
        if kind == b"JSON":
            doc = json.loads(chunk)
        elif kind == b"BIN\x00":
            binary = chunk
        off += 8 + length
    return doc, binary


def write_glb(path, doc, binary):
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    binary += b"\x00" * (-len(binary) % 4)
    total = 12 + 8 + len(js) + 8 + len(binary)
    with open(path, "wb") as f:
        f.write(struct.pack("<4sII", b"glTF", 2, total))
        f.write(struct.pack("<I4s", len(js), b"JSON") + js)
        f.write(struct.pack("<I4s", len(binary), b"BIN\x00") + binary)


def main():
    src, dst, keep = sys.argv[1], sys.argv[2], set(sys.argv[3:])
    doc, binary = read_glb(src)
    anims = [a for a in doc.get("animations", []) if a.get("name") in keep]
    missing = keep - {a["name"] for a in anims}
    if missing:
        sys.exit(f"{src}: no animations named {sorted(missing)}")
    doc["animations"] = anims

    used_acc = set()
    for mesh in doc.get("meshes", []):
        for prim in mesh["primitives"]:
            used_acc.update(prim["attributes"].values())
            if "indices" in prim:
                used_acc.add(prim["indices"])
            for target in prim.get("targets", []):
                used_acc.update(target.values())
    for skin in doc.get("skins", []):
        if "inverseBindMatrices" in skin:
            used_acc.add(skin["inverseBindMatrices"])
    for anim in anims:
        for s in anim["samplers"]:
            used_acc.update((s["input"], s["output"]))

    acc_order = sorted(used_acc)
    acc_map = {old: new for new, old in enumerate(acc_order)}
    accessors = [doc["accessors"][i] for i in acc_order]

    used_bv = {a["bufferView"] for a in accessors if "bufferView" in a}
    used_bv |= {img["bufferView"] for img in doc.get("images", []) if "bufferView" in img}
    bv_order = sorted(used_bv)
    bv_map = {old: new for new, old in enumerate(bv_order)}

    new_bin = bytearray()
    views = []
    for old in bv_order:
        view = dict(doc["bufferViews"][old])
        start = view.get("byteOffset", 0)
        chunk = binary[start:start + view["byteLength"]]
        new_bin += b"\x00" * (-len(new_bin) % 4)
        view["byteOffset"] = len(new_bin)
        view["buffer"] = 0
        new_bin += chunk
        views.append(view)

    for a in accessors:
        if "bufferView" in a:
            a["bufferView"] = bv_map[a["bufferView"]]
    for img in doc.get("images", []):
        if "bufferView" in img:
            img["bufferView"] = bv_map[img["bufferView"]]
    for mesh in doc.get("meshes", []):
        for prim in mesh["primitives"]:
            prim["attributes"] = {k: acc_map[v] for k, v in prim["attributes"].items()}
            if "indices" in prim:
                prim["indices"] = acc_map[prim["indices"]]
            prim["targets"] = [{k: acc_map[v] for k, v in t.items()} for t in prim.get("targets", [])]
            if not prim["targets"]:
                del prim["targets"]
    for skin in doc.get("skins", []):
        if "inverseBindMatrices" in skin:
            skin["inverseBindMatrices"] = acc_map[skin["inverseBindMatrices"]]
    for anim in anims:
        for s in anim["samplers"]:
            s["input"], s["output"] = acc_map[s["input"]], acc_map[s["output"]]

    doc["accessors"], doc["bufferViews"] = accessors, views
    doc["buffers"] = [{"byteLength": len(new_bin)}]
    write_glb(dst, doc, bytes(new_bin))


if __name__ == "__main__":
    main()
