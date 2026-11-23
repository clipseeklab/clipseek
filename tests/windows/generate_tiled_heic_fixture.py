"""Generate an original HEIC grid fixture using independently coded HEVC tiles.

No downloaded photographs are used. Output: 2560x1440, 5x3 512px tiles;
the last row is cropped by the HEIF grid to exercise primary-image dimensions.
Grid layout reference: strukturag/libheif/wiki/Reading-and-Writing-Tiled-Images.
"""

import argparse
import hashlib
import json
import struct
import subprocess
import tempfile
from pathlib import Path


def box(kind, payload):
    return struct.pack(">I4s", len(payload) + 8, kind) + payload


def fullbox(kind, payload, version=0, flags=0):
    return box(kind, bytes([version]) + flags.to_bytes(3, "big") + payload)


def boxes(data, start=0, end=None):
    end = len(data) if end is None else end
    while start < end:
        size, kind = struct.unpack_from(">I4s", data, start)
        if size < 8 or start + size > end:
            raise ValueError("Invalid MP4 box extent")
        yield kind, start + 8, start + size
        start += size


def child(data, start, end, kind):
    return next((a, b) for name, a, b in boxes(data, start, end) if name == kind)


def extract_tiles(data):
    a, b = 0, len(data)
    for kind in [b"moov", b"trak", b"mdia", b"minf", b"stbl"]:
        a, b = child(data, a, b, kind)
    sa, sb = child(data, a, b, b"stsd")
    va, vb = child(data, sa + 8, sb, b"hvc1")
    ha, hb = child(data, va + 78, vb, b"hvcC")
    config = data[ha:hb]
    za, _ = child(data, a, b, b"stsz")
    size, count = struct.unpack_from(">II", data, za + 4)
    sizes = [size] * count if size else list(struct.unpack_from(f">{count}I", data, za + 12))
    ca, _ = child(data, a, b, b"stco")
    chunks = struct.unpack_from(">I", data, ca + 4)[0]
    offsets = struct.unpack_from(f">{chunks}I", data, ca + 8)
    ta, _ = child(data, a, b, b"stsc")
    entries = struct.unpack_from(">I", data, ta + 4)[0]
    table = [struct.unpack_from(">III", data, ta + 8 + 12 * i) for i in range(entries)]
    samples = []
    for chunk, offset in enumerate(offsets, 1):
        per_chunk = next(row[1] for row in reversed(table) if row[0] <= chunk)
        for _ in range(per_chunk):
            length = sizes[len(samples)]
            samples.append(data[offset:offset + length])
            offset += length
    assert len(samples) == count == 15
    return config, samples


def make_heic(config, samples, transforms=(), bit_depth=8, primary_size=(2560, 1440)):
    grid = struct.pack(">BBBBHH", 0, 0, 2, 4, 2560, 1440)
    payloads = [grid, *samples]
    ftyp = box(b"ftyp", b"heic" + bytes(4) + b"mif1heic")
    properties = box(b"ipco", b"".join([
        fullbox(b"ispe", struct.pack(">II", *primary_size)),
        fullbox(b"ispe", struct.pack(">II", 512, 512)),
        box(b"hvcC", config),
        fullbox(b"pixi", bytes([3, bit_depth, bit_depth, bit_depth])),
        *(box(kind, payload) for kind, payload in transforms),
    ]))
    associations = struct.pack(">I", 16)
    for item in range(1, 17):
        indices = [1, 4, *(0x85 + i for i in range(len(transforms)))] if item == 1 else [2, 0x83, 4]
        associations += struct.pack(">HB", item, len(indices)) + bytes(indices)
    iprp = box(b"iprp", properties + fullbox(b"ipma", associations))
    infos = b"".join(fullbox(b"infe", struct.pack(">HH4s", item, 0, b"grid" if item == 1 else b"hvc1")
                            + f"item-{item}".encode() + b"\0", version=2,
                            flags=0 if item == 1 else 1) for item in range(1, 17))
    iinf = fullbox(b"iinf", struct.pack(">H", 16) + infos)
    iref = fullbox(b"iref", box(b"dimg", struct.pack(">HH15H", 1, 15, *range(2, 17))))
    hdlr = fullbox(b"hdlr", bytes(4) + b"pict" + bytes(12) + b"Synthetic HEVC grid\0")

    def metadata(offset):
        locations = bytes([0x44, 0]) + struct.pack(">H", 16)
        for item, payload in enumerate(payloads, 1):
            locations += struct.pack(">HHHII", item, 0, 1, offset, len(payload))
            offset += len(payload)
        return fullbox(b"meta", hdlr + fullbox(b"pitm", struct.pack(">H", 1))
                       + fullbox(b"iloc", locations) + iinf + iref + iprp)

    meta = metadata(0)
    meta = metadata(len(ftyp) + len(meta) + 8)
    return ftyp + meta + box(b"mdat", b"".join(payloads))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ffmpeg", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--bit-depth", type=int, choices=[8, 10], default=8)
    args = parser.parse_args()
    if args.output.exists() or args.output.with_suffix(".json").exists():
        raise SystemExit("Refusing to replace an existing fixture or manifest")
    colors = [(48 + col * 40, 48 + row * 70, 64 + (row * 5 + col) * 10)
              for row in range(3) for col in range(5)]
    raw = bytearray()
    for color in colors:
        tile = bytearray(bytes(color) * (512 * 512))
        for y in range(16, 64):
            tile[3 * (y * 512 + 16):3 * (y * 512 + 64)] = bytes([240]) * (48 * 3)
        raw.extend(tile)
    with tempfile.TemporaryDirectory(prefix="heic-fixture-") as temporary:
        video = Path(temporary) / "tiles.mp4"
        command = [str(args.ffmpeg.resolve()), "-hide_banner", "-loglevel", "error",
                   "-f", "rawvideo", "-pixel_format", "rgb24", "-video_size", "512x512",
                   "-framerate", "1", "-i", "pipe:0", "-frames:v", "15", "-c:v", "libx265",
                   "-preset", "fast", "-crf", "18", "-pix_fmt", "yuv420p10le" if args.bit_depth == 10 else "yuv420p", "-tag:v", "hvc1",
                   "-x265-params", "keyint=1:min-keyint=1:scenecut=0:bframes=0:log-level=error:pools=1:frame-threads=1",
                   "-movflags", "+faststart", str(video)]
        subprocess.run(command, input=raw, check=True, timeout=120)
        config, samples = extract_tiles(video.read_bytes())
        data = make_heic(config, samples, bit_depth=args.bit_depth)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(data)
    manifest = {"file": args.output.name, "source": "Original synthetic fixture; no external media",
                "license": "CC0-1.0", "primary_item_type": "grid", "width": 2560, "height": 1440,
                "columns": 5, "rows": 3, "tile_width": 512, "tile_height": 512,
                "tile_count": 15, "bit_depth": args.bit_depth, "tile_colors_rgb": colors, "bytes": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
                "generator": "tests/windows/generate_tiled_heic_fixture.py",
                "format_reference": "https://github.com/strukturag/libheif/wiki/Reading-and-Writing-Tiled-Images"}
    args.output.with_suffix(".json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
