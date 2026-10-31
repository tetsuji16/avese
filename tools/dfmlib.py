"""Minimal PE resource + Delphi DFM (TPF0) reader.  Standard library only.

Used by extract_assets.py (keeps binary payloads, local use only) and
export_forms.py (drops them, commit-safe).
"""
import json
import struct


class Exe:
    def __init__(self, path):
        b = self.b = open(path, 'rb').read()
        pe = struct.unpack_from('<I', b, 0x3c)[0]
        nsec = struct.unpack_from('<H', b, pe + 6)[0]
        optsz = struct.unpack_from('<H', b, pe + 20)[0]
        sec0 = pe + 24 + optsz
        rsrc_rva = struct.unpack_from('<I', b, pe + 24 + 96 + 2 * 8)[0]
        self.secs = []
        for i in range(nsec):
            o = sec0 + 40 * i
            vs, va, rs, rp = struct.unpack_from('<IIII', b, o + 8)
            self.secs.append((va, max(vs, rs), rp))
        self.base = self.r2o(rsrc_rva)
        self.res = []
        self._walk(0, [])

    def r2o(self, rva):
        for va, sz, rp in self.secs:
            if va <= rva < va + sz:
                return rva - va + rp
        raise ValueError(hex(rva))

    def _name(self, idv):
        b = self.b
        if idv & 0x80000000:
            o = self.base + (idv & 0x7fffffff)
            n = struct.unpack_from('<H', b, o)[0]
            return b[o + 2:o + 2 + 2 * n].decode('utf-16le')
        return idv

    def _walk(self, off, path):
        b = self.b
        named, ids = struct.unpack_from('<HH', b, self.base + off + 12)
        for i in range(named + ids):
            idv, ptr = struct.unpack_from('<II', b, self.base + off + 16 + 8 * i)
            p = path + [self._name(idv)]
            if ptr & 0x80000000:
                self._walk(ptr & 0x7fffffff, p)
            else:
                rva, sz = struct.unpack_from('<II', b, self.base + ptr)
                o = self.r2o(rva)
                self.res.append((p, b[o:o + sz]))

    def forms(self, keep_binary=False):
        """Yield (resource name, component tree) for every TPF0 resource."""
        for p, d in self.res:
            if d[:4] == b'TPF0':
                r = Reader(d, keep_binary)
                r.i = 4
                yield str(p[1]), r.comp()


class Reader:
    def __init__(self, d, keep_binary):
        self.d, self.i, self.keep = d, 0, keep_binary

    def u8(self):
        v = self.d[self.i]
        self.i += 1
        return v

    def take(self, n):
        v = self.d[self.i:self.i + n]
        self.i += n
        return v

    def sstr(self):
        return self.take(self.u8()).decode('cp932', 'replace')

    def val(self):
        t = self.u8()
        if t == 0:
            return None
        if t == 1:
            out = []
            while self.d[self.i] != 0:
                out.append(self.val())
            self.i += 1
            return out
        if t == 2:
            return struct.unpack('<b', self.take(1))[0]
        if t == 3:
            return struct.unpack('<h', self.take(2))[0]
        if t == 4:
            return struct.unpack('<i', self.take(4))[0]
        if t == 5:
            raw = self.take(10)
            m = int.from_bytes(raw[:8], 'little')
            e = int.from_bytes(raw[8:], 'little')
            sg = -1 if e & 0x8000 else 1
            e &= 0x7fff
            return sg * m * 2.0 ** (e - 16383 - 63) if m else 0.0
        if t == 6:
            return self.sstr()
        if t == 7:
            return {'ident': self.sstr()}
        if t == 8:
            return False
        if t == 9:
            return True
        if t == 10:
            n = struct.unpack('<I', self.take(4))[0]
            raw = self.take(n)
            return {'binary': n, '_data': raw} if self.keep else {'binary': n}
        if t == 11:
            out = []
            while True:
                x = self.sstr()
                if not x:
                    break
                out.append(x)
            return {'set': out}
        if t == 12:
            n = struct.unpack('<I', self.take(4))[0]
            return self.take(n).decode('cp932', 'replace')
        if t == 13:
            return {'nil': True}
        if t == 14:
            items = []
            while self.d[self.i] != 0:
                if self.d[self.i] in (2, 3, 4):
                    self.val()
                else:
                    self.i += 1
                props = {}
                while self.d[self.i] != 0:
                    k = self.sstr()
                    props[k] = self.val()
                self.i += 1
                items.append(props)
            self.i += 1
            return {'collection': items}
        if t == 18:
            n = struct.unpack('<I', self.take(4))[0]
            return self.take(2 * n).decode('utf-16le')
        if t == 20:
            n = struct.unpack('<I', self.take(4))[0]
            return self.take(n).decode('utf-8', 'replace')
        raise ValueError('unknown value type %d' % t)

    def comp(self):
        if self.d[self.i] & 0xf0 == 0xf0:
            fl = self.u8()
            if fl & 2:
                self.val()
        cls = self.sstr()
        name = self.sstr()
        props = {}
        while self.d[self.i] != 0:
            k = self.sstr()
            props[k] = self.val()
        self.i += 1
        kids = []
        while self.d[self.i] != 0:
            kids.append(self.comp())
        self.i += 1
        return {'class': cls, 'name': name, 'props': props, 'kids': kids}


def find(comp, name):
    if comp['name'] == name:
        return comp
    for k in comp['kids']:
        r = find(k, name)
        if r:
            return r
    return None


def dump_json(comp, path):
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(comp, f, ensure_ascii=False, indent=0)
