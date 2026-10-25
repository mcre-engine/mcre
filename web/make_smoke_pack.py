import gzip
import pathlib
import struct
import sys
import zipfile
import zlib


def nbt_name(kind, name):
    raw = name.encode()
    return bytes([kind]) + struct.pack(">H", len(raw)) + raw


def nbt_int(name, value):
    return nbt_name(3, name) + struct.pack(">i", value)


def nbt_string_payload(value):
    raw = value.encode()
    return struct.pack(">H", len(raw)) + raw


def nbt_int_array(name, values):
    return (
        nbt_name(11, name)
        + struct.pack(">i", len(values))
        + b"".join(struct.pack(">i", value) for value in values)
    )


def nbt_list(name, kind, payloads):
    return (
        nbt_name(9, name)
        + bytes([kind])
        + struct.pack(">i", len(payloads))
        + b"".join(payloads)
    )


def nbt_compound(name, payload):
    return nbt_name(10, name) + payload + b"\x00"


def nbt_compound_payload(payload):
    return payload + b"\x00"


def make_region(payload):
    compressed = zlib.compress(payload)
    sectors = (5 + len(compressed) + 4095) // 4096
    region = bytearray(8192 + sectors * 4096)
    region[0:4] = struct.pack(">I", (2 << 8) | sectors)
    region[8192:8196] = struct.pack(">I", len(compressed) + 1)
    region[8196] = 2
    region[8197 : 8197 + len(compressed)] = compressed
    return bytes(region)


def make_smoke_save(path):
    spawn = nbt_compound(
        "spawn",
        nbt_int_array("pos", [8, 20, 8])
        + nbt_name(8, "dimension")
        + nbt_string_payload("minecraft:overworld"),
    )
    level = nbt_compound("", nbt_compound("Data", spawn))
    section = nbt_compound_payload(
        nbt_int("Y", 0)
        + nbt_compound(
            "block_states",
            nbt_list("palette", 8, [nbt_string_payload("minecraft:stone")]),
        )
        + nbt_compound(
            "biomes",
            nbt_list("palette", 8, [nbt_string_payload("minecraft:plains")]),
        )
    )
    chunk = nbt_compound(
        "",
        nbt_int("xPos", 0)
        + nbt_int("zPos", 0)
        + nbt_list("sections", 10, [section]),
    )
    entity = nbt_compound_payload(
        nbt_name(8, "id")
        + nbt_string_payload("minecraft:pig")
        + nbt_list(
            "Pos",
            6,
            [struct.pack(">d", 8.5), struct.pack(">d", 17.0), struct.pack(">d", 8.5)],
        )
    )
    entity_chunk = nbt_compound(
        "",
        nbt_int_array("Position", [0, 0]) + nbt_list("Entities", 10, [entity]),
    )

    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        archive.writestr("level.dat", gzip.compress(level, mtime=0))
        archive.writestr("region/r.0.0.mca", make_region(chunk))
        archive.writestr("entities/r.0.0.mca", make_region(entity_chunk))


version = pathlib.Path("mc-version").read_text().strip()
assets = pathlib.Path("target/downloads") / version / "assets" / "minecraft"
if not assets.is_dir():
    raise SystemExit(f"Minecraft assets not found: {assets}")

out = pathlib.Path(sys.argv[1])
save_out = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else out.with_name("smoke-save.zip")
out.parent.mkdir(parents=True, exist_ok=True)
save_out.parent.mkdir(parents=True, exist_ok=True)
make_smoke_save(save_out)
with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
    archive.writestr(
        "pack/pack.mcmeta",
        '{"pack":{"description":"MCRE browser smoke pack","min_format":[100,2],"max_format":[100,2]}}',
    )
    archive.writestr(
        "pack/assets/minecraft/textures/block/stone.png.mcmeta",
        '{"animation":{"frametime":1,"frames":[0,0]}}',
    )
    files = [
        "blockstates/grass_block.json",
        "blockstates/stone.json",
        "models/block/block.json",
        "models/block/cube.json",
        "models/block/cube_all.json",
        "models/block/grass_block.json",
        "models/block/stone.json",
    ]
    files.extend(
        f"textures/colormap/{name}.png"
        for name in ("grass", "foliage", "dry_foliage")
    )
    files.extend(
        f"textures/block/{name}.png"
        for name in (
            "dirt",
            "grass_block_top",
            "grass_block_side",
            "grass_block_side_overlay",
            "stone",
        )
    )
    files.append("textures/entity/pig/pig_temperate.png")
    for name in files:
        file = assets / name
        archive.write(file, (pathlib.Path("pack/assets/minecraft") / name).as_posix())
