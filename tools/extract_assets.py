"""Extract the original's images into assets/local/ (git-ignored; never commit).

Writes (all PNG, RGBA):
  imagelist.png  toolbar/menu ImageList1, 4 columns x N rows of 20x20 cells
  app.png        form icon (32x32)
  about.png      About box picture (64x64)

usage: python tools/extract_assets.py ["original avese/avese.exe"] [assets/local]
Standard library only.  The images remain the property of the original author.
"""
import os
import struct
import sys
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dfmlib


def write_png(path, w, h, rgba):
    def chunk(t, d):
        c = struct.pack('>I', len(d)) + t + d
        return c + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    raw = b''.join(b'\x00' + bytes(rgba[y * w * 4:(y + 1) * w * 4]) for y in range(h))
    data = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b''))
    with open(path, 'wb') as f:
        f.write(data)


def dib(data, off, height_div=1):
    """Decode a DIB (BITMAPINFOHEADER at data[off:]) -> (w, h, bpp, rows top-down of pixel tuples)."""
    hs, w, h, planes, bpp, comp, isz, _, _, clr, _ = struct.unpack_from('<IiiHHIIiiII', data, off)
    h = abs(h) // height_div
    ncol = clr or ((1 << bpp) if bpp <= 8 else 0)
    pal = []
    p = off + hs
    for i in range(ncol if bpp <= 8 else 0):
        b, g, r, _ = data[p + 4 * i:p + 4 * i + 4]
        pal.append((r, g, b))
    p += 4 * len(pal)
    stride = ((w * bpp + 31) // 32) * 4
    rows = []
    for y in range(h):
        ro = p + (h - 1 - y) * stride
        row = []
        for x in range(w):
            if bpp == 32:
                b, g, r, a = data[ro + 4 * x:ro + 4 * x + 4]
                row.append((r, g, b))
            elif bpp == 24:
                b, g, r = data[ro + 3 * x:ro + 3 * x + 3]
                row.append((r, g, b))
            else:
                bit = x * bpp
                v = (data[ro + bit // 8] >> (8 - bpp - bit % 8)) & ((1 << bpp) - 1)
                row.append(pal[v] if pal else (v, v, v))
        rows.append(row)
    return w, h, bpp, rows, p + stride * h


def mask_rows(data, off, w, h):
    stride = ((w + 31) // 32) * 4
    rows = []
    for y in range(h):
        ro = off + (h - 1 - y) * stride
        rows.append([(data[ro + x // 8] >> (7 - x % 8)) & 1 for x in range(w)])
    return rows


def to_rgba(rows, mask):
    out = bytearray()
    for y, row in enumerate(rows):
        for x, (r, g, b) in enumerate(row):
            out += bytes((r, g, b, 0 if mask and mask[y][x] else 255))
    return out


def from_ico(d):
    cnt = struct.unpack_from('<H', d, 4)[0]
    size, off = struct.unpack_from('<II', d, 6 + 8)
    w, h, bpp, rows, mpos = dib(d, off, 2)
    mk = mask_rows(d, mpos, w, h)
    return w, h, to_rgba(rows, mk)


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    exe = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, 'original avese', 'avese.exe')
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(root, 'assets', 'local')
    os.makedirs(out, exist_ok=True)
    forms = dict(dfmlib.Exe(exe).forms(keep_binary=True))
    f1 = forms['TFORM1']
    # ImageList: 'IL' header followed by colour BMP (file header) and mask BMP
    il = dfmlib.find(f1, 'ImageList1')['props']['Bitmap']['_data']
    offs = [i for i in range(len(il) - 1) if il[i:i + 2] == b'BM' and il[i + 14:i + 18] == b'\x28\x00\x00\x00']
    cw, ch, bpp, rows, _ = dib(il, offs[0] + 14)
    mw, mh, _, _, _ = dib(il, offs[1] + 14)
    mk = mask_rows(il, offs[1] + 14 + 40 + 8, mw, mh)
    write_png(os.path.join(out, 'imagelist.png'), cw, ch, to_rgba(rows, mk))
    print('imagelist.png', cw, ch)
    w, h, px = from_ico(f1['props']['Icon.Data']['_data'])
    write_png(os.path.join(out, 'app.png'), w, h, px)
    print('app.png', w, h)
    ab = dfmlib.find(forms['TABOUTBOX'], 'Image1')['props']['Picture.Data']['_data']
    bm = ab.index(b'BM')
    w, h, bpp, rows, _ = dib(ab, bm + 14)
    write_png(os.path.join(out, 'about.png'), w, h, to_rgba(rows, None))
    print('about.png', w, h)


if __name__ == '__main__':
    main()
