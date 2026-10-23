import pathlib
import sys
import zipfile


version = pathlib.Path("mc-version").read_text().strip()
assets = pathlib.Path("target/downloads") / version / "assets" / "minecraft"
if not assets.is_dir():
    raise SystemExit(f"Minecraft assets not found: {assets}")

out = pathlib.Path(sys.argv[1])
out.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
    archive.writestr(
        "pack.mcmeta",
        '{"pack":{"description":"MCRE browser smoke pack","min_format":[100,1],"max_format":[100,1]}}',
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
    for name in files:
        file = assets / name
        archive.write(file, (pathlib.Path("assets/minecraft") / name).as_posix())
