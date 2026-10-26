import gzip
import math
import pathlib
import struct
import zlib

from flat_seam import (
    CHUNK_COUNT,
    SPAWN,
    compound,
    compound_payload,
    integer,
    int_array,
    list_tag,
    long_array,
    palette_entry,
    string,
    string_payload,
)


OUT = pathlib.Path("target/spawner_gallery/world")
SPAWNERS = (
    ("minecraft:zombie", 136, 1, 140),
    ("minecraft:skeleton", 132, 1, 136),
    ("minecraft:pig", 136, 1, 132),
    ("minecraft:creeper", 140, 1, 136),
)


def short(name, value):
    encoded = name.encode()
    return bytes((2,)) + struct.pack(">H", len(encoded)) + encoded + struct.pack(">h", value)


def spawner_block_entity(entity_id, x, y, z):
    return compound_payload(
        [
            string("id", "minecraft:mob_spawner"),
            integer("x", x),
            integer("y", y),
            integer("z", z),
            compound("SpawnData", [compound("entity", [string("id", entity_id)])]),
            short("Delay", 200),
        ]
    )


def section_for(spawners):
    indices = [0] * 4096
    for z in range(16):
        for x in range(16):
            indices[z * 16 + x] = 1
    for _, x, _, z in spawners:
        indices[256 + (z % 16) * 16 + x % 16] = 2
    packed = [0] * 256
    for index, value in enumerate(indices):
        packed[index // 16] |= value << ((index % 16) * 4)
    return compound_payload(
        [
            integer("Y", 0),
            compound(
                "block_states",
                [
                    list_tag(
                        "palette",
                        10,
                        [
                            palette_entry("minecraft:air"),
                            palette_entry("minecraft:stone"),
                            palette_entry("minecraft:spawner"),
                        ],
                    ),
                    long_array("data", packed),
                ],
            ),
            compound(
                "biomes",
                [list_tag("palette", 8, [string_payload("minecraft:plains")])],
            ),
        ]
    )


def make_chunk(chunk_x, chunk_z):
    spawners = [
        item
        for item in SPAWNERS
        if item[1] // 16 == chunk_x and item[3] // 16 == chunk_z
    ]
    fields = [
        integer("xPos", chunk_x),
        integer("zPos", chunk_z),
        list_tag("sections", 10, [section_for(spawners)]),
    ]
    if spawners:
        fields.append(
            list_tag(
                "block_entities",
                10,
                [spawner_block_entity(*spawner) for spawner in spawners],
            )
        )
    return b"\x0a\x00\x00" + compound_payload(fields)


def make_region():
    region = bytearray(8192)
    sector = 2
    for chunk_z in range(CHUNK_COUNT):
        for chunk_x in range(CHUNK_COUNT):
            compressed = zlib.compress(make_chunk(chunk_x, chunk_z))
            record = struct.pack(">I", len(compressed) + 1) + b"\x02" + compressed
            sectors = math.ceil(len(record) / 4096)
            local_index = chunk_x + chunk_z * 32
            struct.pack_into(">I", region, local_index * 4, (sector << 8) | sectors)
            region.extend(record)
            region.extend(bytes(sectors * 4096 - len(record)))
            sector += sectors
    return region


def make_level():
    return b"\x0a\x00\x00" + compound_payload(
        [
            compound(
                "Data",
                [
                    compound(
                        "spawn",
                        [
                            int_array("pos", SPAWN),
                            string("dimension", "minecraft:overworld"),
                        ],
                    )
                ],
            )
        ]
    )


def main():
    region_dir = OUT / "region"
    region_dir.mkdir(parents=True, exist_ok=True)
    (region_dir / "r.0.0.mca").write_bytes(make_region())
    (OUT / "level.dat").write_bytes(gzip.compress(make_level(), mtime=0))
    print(f"spawner gallery world: {OUT}")


if __name__ == "__main__":
    main()
