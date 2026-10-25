import pathlib
import struct
import zlib


def read_png(path):
    png = path.read_bytes()
    if png[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"not a PNG: {path}")
    offset = 8
    compressed = bytearray()
    while offset < len(png):
        length = struct.unpack_from(">I", png, offset)[0]
        kind = png[offset + 4 : offset + 8]
        payload = png[offset + 8 : offset + 8 + length]
        offset += length + 12
        if kind == b"IHDR":
            width, height, depth, color, _, _, _ = struct.unpack(">IIBBBBB", payload)
            if (depth, color) != (8, 6):
                raise ValueError(f"expected RGBA8 PNG: {path}")
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            break
    stride = width * 4
    raw = zlib.decompress(compressed)
    previous = bytearray(stride)
    rows = []
    index = 0
    for _ in range(height):
        filter_kind = raw[index]
        index += 1
        row = bytearray(raw[index : index + stride])
        index += stride
        for position in range(stride):
            left = row[position - 4] if position >= 4 else 0
            above = previous[position]
            upper_left = previous[position - 4] if position >= 4 else 0
            if filter_kind == 1:
                predictor = left
            elif filter_kind == 2:
                predictor = above
            elif filter_kind == 3:
                predictor = (left + above) // 2
            elif filter_kind == 4:
                p = left + above - upper_left
                distances = (abs(p - left), abs(p - above), abs(p - upper_left))
                predictor = (left, above, upper_left)[distances.index(min(distances))]
            elif filter_kind == 0:
                predictor = 0
            else:
                raise ValueError(f"unsupported PNG filter {filter_kind}: {path}")
            row[position] = (row[position] + predictor) & 0xFF
        rows.append(row)
        previous = row
    return width, height, rows


def main():
    for direction in ("south", "west", "north", "east"):
        path = pathlib.Path(f"target/shots/flat-seam-{direction}.mcre.png")
        width, height, rows = read_png(path)
        if (width, height) != (960, 540):
            raise ValueError(f"unexpected dimensions: {path}")
        sky = rows[0][:3]
        if rows[-1][:3] == sky:
            raise ValueError(f"floor is missing: {path}")
        leaks = [
            (x, y)
            for y in range(height // 2 + 4, height)
            for x in range(width)
            if rows[y][x * 4 : x * 4 + 3] == sky
        ]
        print(f"{direction}: {len(leaks)} sky pixels below the floor horizon")
        if leaks:
            raise ValueError(f"floor pinholes in {path}: {leaks[:10]}")
        warm = [
            (x, y)
            for y in range(height // 2 + 4, height)
            for x in range(width)
            if rows[y][x * 4] > rows[y][x * 4 + 2] + 10
        ]
        print(f"{direction}: {len(warm)} off-sprite pixels on the stone floor")
        if warm:
            raise ValueError(f"texture atlas seams in {path}: {warm[:10]}")


if __name__ == "__main__":
    main()
