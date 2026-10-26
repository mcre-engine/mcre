import gzip
import math
import pathlib
import struct
import zlib


CHUNK_COUNT = 17
SPAWN = (136, 2, 136)
OUT = pathlib.Path("target/flat_seam/world")


def tag(kind, name, payload):
    encoded = name.encode()
    return bytes((kind,)) + struct.pack(">H", len(encoded)) + encoded + payload


def integer(name, value):
    return tag(3, name, struct.pack(">i", value))


def string_payload(value):
    encoded = value.encode()
    return struct.pack(">H", len(encoded)) + encoded


def string(name, value):
    return tag(8, name, string_payload(value))


def compound_payload(fields):
    return b"".join(fields) + b"\x00"


def compound(name, fields):
    return tag(10, name, compound_payload(fields))


def list_tag(name, item_kind, payloads):
    return tag(
        9,
        name,
        bytes((item_kind,))
        + struct.pack(">i", len(payloads))
        + b"".join(payloads),
    )


def int_array(name, values):
    return tag(
        11,
        name,
        struct.pack(">i", len(values))
        + b"".join(struct.pack(">i", value) for value in values),
    )


def long_array(name, values):
    return tag(
        12,
        name,
        struct.pack(">i", len(values))
        + b"".join(struct.pack(">q", value) for value in values),
    )


def palette_entry(name):
    return compound_payload([string("Name", name)])


def make_section():
    packed = [0x1111111111111111 if index < 16 else 0 for index in range(256)]
    return compound_payload(
        [
            integer("Y", 0),
            compound(
                "block_states",
                [
                    list_tag(
                        "palette",
                        10,
                        [palette_entry("minecraft:air"), palette_entry("minecraft:stone")],
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


def make_chunk(chunk_x, chunk_z, section):
    return b"\x0a\x00\x00" + compound_payload(
        [
            integer("xPos", chunk_x),
            integer("zPos", chunk_z),
            list_tag("sections", 10, [section]),
        ]
    )


def make_region(section):
    region = bytearray(8192)
    sector = 2
    for chunk_z in range(CHUNK_COUNT):
        for chunk_x in range(CHUNK_COUNT):
            compressed = zlib.compress(make_chunk(chunk_x, chunk_z, section))
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
    (region_dir / "r.0.0.mca").write_bytes(make_region(make_section()))
    (OUT / "level.dat").write_bytes(gzip.compress(make_level(), mtime=0))
    print(f"flat seam test world: {OUT} ({CHUNK_COUNT * CHUNK_COUNT} chunks)")


if __name__ == "__main__":
    main()
