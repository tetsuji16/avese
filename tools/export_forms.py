"""Write layout/caption-only form trees (no binary payloads) to assets/forms/.

usage: python tools/export_forms.py ["original avese/avese.exe"] [assets/forms]
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dfmlib


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    exe = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, 'original avese', 'avese.exe')
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(root, 'assets', 'forms')
    os.makedirs(out, exist_ok=True)
    for name, comp in dfmlib.Exe(exe).forms(keep_binary=False):
        dfmlib.dump_json(comp, os.path.join(out, name + '.json'))
        print('wrote', name)


if __name__ == '__main__':
    main()
