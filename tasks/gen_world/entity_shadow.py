import math
import pathlib
import struct
import zlib

import flat_seam
from flat_seam import compound_payload, integer, list_tag, string


OUT = pathlib.Path("target/entity_shadow/world")
ENTITY = (136.5, 1.0, 136.5)


def entity_payload():
    return compound_payload(
        [
            string("id", "minecraft:pig"),
            list_tag("Pos", 6, [struct.pack(">d", value) for value in ENTITY]),
            list_tag("Rotation", 5, [struct.pack(">f", 0.0), struct.pack(">f", 0.0)]),
        ]
    )


def make_chunk(chunk_x, chunk_z):
    fields = [
        integer("xPos", chunk_x),
        integer("zPos", chunk_z),
        list_tag("sections", 10, [flat_seam.make_section()]),
    ]
    if (chunk_x, chunk_z) == (8, 8):
        fields.append(list_tag("entities", 10, [entity_payload()]))
    return b"\x0a\x00\x00" + compound_payload(fields)


def make_region():
    region = bytearray(8192)
    sector = 2
    for chunk_z in range(flat_seam.CHUNK_COUNT):
        for chunk_x in range(flat_seam.CHUNK_COUNT):
            compressed = zlib.compress(make_chunk(chunk_x, chunk_z))
            record = struct.pack(">I", len(compressed) + 1) + b"\x02" + compressed
            sectors = math.ceil(len(record) / 4096)
            local_index = chunk_x + chunk_z * 32
            struct.pack_into(">I", region, local_index * 4, (sector << 8) | sectors)
            region.extend(record)
            region.extend(bytes(sectors * 4096 - len(record)))
            sector += sectors
    return region


def main():
    region_dir = OUT / "region"
    region_dir.mkdir(parents=True, exist_ok=True)
    (region_dir / "r.0.0.mca").write_bytes(make_region())
    (OUT / "level.dat").write_bytes(flat_seam.make_level())
    print(f"entity shadow world: {OUT}")


if __name__ == "__main__":
    main()
