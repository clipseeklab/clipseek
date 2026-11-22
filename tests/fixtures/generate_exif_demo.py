"""Generate a small synthetic PNG and its TIFF EXIF payload, without dependencies.

All device names and GPS coordinates are test values, not a real photo's metadata.
Usage: python3 tests/fixtures/generate_exif_demo.py [output-directory]
"""
import pathlib
import struct
import sys
import zlib


def exif_payload():
    data = bytearray(b"II\x2a\x00\x08\x00\x00\x00")

    def ifd(fields):
        start = len(data)
        data.extend(struct.pack("<H", len(fields)))
        data.extend(bytes(12 * len(fields) + 4))
        for index, (tag, kind, count, value) in enumerate(sorted(fields)):
            if len(value) > 4:
                stored = struct.pack("<I", len(data))
                data.extend(value)
                if len(data) % 2:
                    data.append(0)
            else:
                stored = value.ljust(4, b"\0")
            struct.pack_into("<HHI4s", data, start + 2 + 12 * index, tag, kind, count, stored)
        return start

    def ascii_tag(tag, text):
        value = text.encode("ascii") + b"\0"
        return tag, 2, len(value), value

    def rational(tag, *values):
        return tag, 5, len(values), b"".join(struct.pack("<II", *v) for v in values)

    root = ifd([
        ascii_tag(0x010E, "Synthetic EXIF demo - not a real photograph"),
        ascii_tag(0x010F, "ClipSeek Demo"),
        ascii_tag(0x0110, "Demo Camera"),
        (0x0112, 3, 1, struct.pack("<H", 1)),
        (0x8769, 4, 1, bytes(4)),
        (0x8825, 4, 1, bytes(4)),
    ])
    shooting = ifd([
        rational(0x829A, (1, 125)), rational(0x829D, (28, 10)),
        (0x8827, 3, 1, struct.pack("<H", 200)),
        (0x9000, 7, 4, b"0232"),
        (0x927C, 7, 8, b"DEMONOTE"),
        ascii_tag(0x9003, "2026:11:07 10:30:00"),
        ascii_tag(0x9011, "+08:00"), rational(0x920A, (50, 1)),
        ascii_tag(0xA433, "ClipSeek Demo"), ascii_tag(0xA434, "Demo 50mm F2.8"),
        (0xA405, 3, 1, struct.pack("<H", 75)),
    ])
    gps = ifd([
        (0x0000, 1, 4, bytes([2, 3, 0, 0])),
        ascii_tag(0x0001, "N"), rational(0x0002, (31, 1), (13, 1), (48, 1)),
        ascii_tag(0x0003, "E"), rational(0x0004, (121, 1), (28, 1), (12, 1)),
        (0x0005, 1, 1, bytes([0])), rational(0x0006, (125, 10)),
    ])
    for index, offset in [(4, shooting), (5, gps)]:
        struct.pack_into("<I", data, root + 2 + index * 12 + 8, offset)
    return bytes(data)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def main():
    destination = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).parent
    destination.mkdir(parents=True, exist_ok=True)
    payload = exif_payload()
    # A visibly artificial checker card; pixels do not depict the example GPS location.
    width, height = 320, 200
    pixels = bytearray()
    for y in range(height):
        pixels.append(0)
        for x in range(width):
            pixels.extend((45 + x // 2, 65 + y // 2, 170 if (x // 40 + y // 40) % 2 else 90))
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    png += chunk(b"eXIf", payload) + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")
    (destination / "exif-demo.png").write_bytes(png)
    (destination / "exif-demo.tiff").write_bytes(payload)


if __name__ == "__main__":
    main()
