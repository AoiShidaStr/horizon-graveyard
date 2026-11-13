"""A small reader for gzipped structure NBT files (no dependencies).

Part of the Horizon port of The Graveyard (GPL-3.0-only). Original code.
"""
import gzip
import struct


class _Reader:
    def __init__(self, data):
        self.d = data
        self.i = 0

    def take(self, n):
        b = self.d[self.i:self.i + n]
        self.i += n
        return b

    def u8(self):
        return self.take(1)[0]

    def i16(self):
        return struct.unpack(">h", self.take(2))[0]

    def i32(self):
        return struct.unpack(">i", self.take(4))[0]

    def i64(self):
        return struct.unpack(">q", self.take(8))[0]

    def string(self):
        n = struct.unpack(">H", self.take(2))[0]
        return self.take(n).decode("utf-8", "replace")

    def payload(self, tag):
        if tag == 1:
            return struct.unpack(">b", self.take(1))[0]
        if tag == 2:
            return self.i16()
        if tag == 3:
            return self.i32()
        if tag == 4:
            return self.i64()
        if tag == 5:
            return struct.unpack(">f", self.take(4))[0]
        if tag == 6:
            return struct.unpack(">d", self.take(8))[0]
        if tag == 7:
            n = self.i32()
            return list(self.take(n))
        if tag == 8:
            return self.string()
        if tag == 9:
            inner = self.u8()
            n = self.i32()
            return [self.payload(inner) for _ in range(n)]
        if tag == 10:
            out = {}
            while True:
                t = self.u8()
                if t == 0:
                    return out
                name = self.string()
                out[name] = self.payload(t)
        if tag == 11:
            n = self.i32()
            return [self.i32() for _ in range(n)]
        if tag == 12:
            n = self.i32()
            return [self.i64() for _ in range(n)]
        raise ValueError("bad tag %d" % tag)


def read(path):
    """The root compound of a gzipped NBT file."""
    with open(path, "rb") as f:
        raw = f.read()
    try:
        raw = gzip.decompress(raw)
    except OSError:
        pass
    r = _Reader(raw)
    tag = r.u8()
    r.string()
    return r.payload(tag)


def palettes(root):
    """Every palette of a structure template (one, or `palettes` for several)."""
    if "palette" in root:
        return [root["palette"]]
    return root.get("palettes", [])
