# -*- coding: utf-8 -*-
"""説明サイトのサンプルデータを作る。外部ライブラリは使わない。

使い方: python -I make_samples.py [出力先フォルダ]
"""
import math
import os
import random
import sys
import zipfile

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__))


def write(name, settings, rows, head=""):
    path = os.path.join(OUT, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        if settings:
            f.write("[AVESE.EXE]\n")
            for k, v in settings.items():
                f.write(f"{k}={v}\n")
            f.write("[DATA]\n")
        f.write(head)
        for row in rows:
            if row is None:
                f.write("\n")
            else:
                f.write(" ".join(row) + "\n")


def num(v):
    return f"{v:.5f}"


def flow():
    """一様流の中の円柱まわりのポテンシャル流。Z は圧力係数、U, V は速度。"""
    rows, a = [], 1.0
    for j in range(61):
        y = -3 + j * 0.1
        for i in range(101):
            x = -5 + i * 0.1
            r2 = x * x + y * y
            if r2 < a * a:
                rows.append((num(x), num(y), "-", "0", "0"))
                continue
            u = 1 - a * a * (x * x - y * y) / (r2 * r2)
            v = -2 * a * a * x * y / (r2 * r2)
            rows.append((num(x), num(y), num(1 - u * u - v * v), num(u), num(v)))
        rows.append(None)
    return rows


def peaks():
    """山と谷のある面。俯瞰図の例。"""
    rows = []
    for j in range(61):
        y = -3 + j * 0.1
        for i in range(61):
            x = -3 + i * 0.1
            z = 3 * (1 - x) ** 2 * math.exp(-x * x - (y + 1) ** 2) - 10 * (x / 5 - x ** 3 - y ** 5) * math.exp(-x * x - y * y) - math.exp(-(x + 1) ** 2 - y * y) / 3
            rows.append((num(x), num(y), num(z)))
        rows.append(None)
    return rows


def polar():
    """円筒座標の例。θ は度、r は半径。U, V は θ 方向、r 方向の成分。"""
    rows = []
    for j in range(25):
        r = 1 + j * 0.125
        for i in range(73):
            t = i * 5.0
            th = math.radians(t)
            z = math.cos(3 * th) * math.sin(math.pi * (r - 1) / 3)
            rows.append((num(t), num(r), num(z), num(0.5 * math.sin(3 * th)), num(0.3 * math.cos(3 * th))))
        rows.append(None)
    return rows


def scatter():
    """格子に乗らない計測点。Mesh AutoCreate で三角メッシュにする。"""
    rng = random.Random(16)
    rows = []
    for _ in range(600):
        x, y = rng.uniform(0, 10), rng.uniform(0, 10)
        z = math.sin(x * 0.9) * math.cos(y * 0.7) + 0.15 * math.hypot(x - 5, y - 5)
        rows.append((num(x), num(y), num(z)))
    return rows


def panel():
    """Z だけを格子状に並べた例。1 行目が画像の一番上。"""
    rows = []
    for j in range(40):
        y = 3 - j * 0.15
        rows.append([num(math.sin(1.2 * (-4 + i * 0.13)) * math.cos(1.5 * y) * math.exp(-0.05 * ((-4 + i * 0.13) ** 2 + y * y))) for i in range(60)])
    return rows


def wave(k, n):
    """2 つの波源の干渉。連続処理と動画の例。"""
    t = 2 * math.pi * k / n
    rows = []
    for j in range(81):
        y = -4 + j * 0.1
        for i in range(121):
            x = -6 + i * 0.1
            a = math.hypot(x + 2, y)
            b = math.hypot(x - 2, y)
            z = math.cos(3 * a - t) * math.exp(-a * 0.18) + math.cos(3 * b - t) * math.exp(-b * 0.18)
            rows.append((num(x), num(y), num(z)))
        rows.append(None)
    return rows


if __name__ == "__main__":
    write("flow.txt", {}, flow())
    write("peaks.txt", {}, peaks())
    write("polar.txt", {}, polar())
    write("scatter.txt", {}, scatter(), head="Mesh AutoCreate\n")
    write("panel.txt", {}, panel())
    n = 24
    for k in range(n):
        write(f"wave/wave{k + 1:02d}.txt", {}, wave(k, n))
    with zipfile.ZipFile(os.path.join(OUT, "samples.zip"), "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name in ["flow.txt", "peaks.txt", "polar.txt", "scatter.txt", "panel.txt"] + [f"wave/wave{k + 1:02d}.txt" for k in range(n)]:
            z.write(os.path.join(OUT, name), name)
