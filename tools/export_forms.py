"""Write layout/caption-only form trees (no binary payloads) to assets/forms/.

usage: python tools/export_forms.py ["original avese/avese.exe"] [assets/forms]
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dfmlib


def rename(path):
    """This program is called avese; the original's name must not appear on its screens."""
    with open(path, encoding='utf-8') as f:
        text = f.read()
    for old, new in (('AV似非（えーぶい・えせ）', 'avese'), ('ＡＶ似非', 'avese'), ('AV似非', 'avese')):
        text = text.replace(old, new)
    if os.path.basename(path) == 'TABOUTBOX.json':
        # this is a reimplementation: own version, no original copyright line, author credit kept
        text = text.replace('"Caption": "Version  0.93"', '"Caption": "Version  0.1.0"')
        text = text.replace('"Caption": "Copyright  (c) 1997-2002"', '"Caption": "Rust 再実装版（非公式）"')
        text = re.sub(r'("name": "Label2",\s*"props": \{\s*"Left": )128(,\s*"Top": 80,\s*"Width": )45(,\s*"Height": 13,\s*"Caption": )"おりいる"',
                      r'\g<1>100\g<2>144\g<3>"原作: おりいる"', text)
    if os.path.basename(path) == 'TMESHPROGRESS.json':
        text = text.replace('"Caption": "Version  0.01"', '"Caption": "Version  0.1.0"')
    with open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(text)


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    exe = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, 'original avese', 'avese.exe')
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(root, 'assets', 'forms')
    os.makedirs(out, exist_ok=True)
    for name, comp in dfmlib.Exe(exe).forms(keep_binary=False):
        path = os.path.join(out, name + '.json')
        dfmlib.dump_json(comp, path)
        rename(path)
        print('wrote', name)


if __name__ == '__main__':
    main()
