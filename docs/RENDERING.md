# 描画エンジンの設計

原作のマニュアルにある機能をすべて Rust で実装する。画素単位の一致は目標にしない。原作の exe には実行時に依存しない。
外部プログラムや C ライブラリに頼らず、純 Rust のクレートと自前のコード（Windows のクリップボードと保存ダイアログのみ Win32 API）で実装する。

## 全体の流れ

```
バイト列 → data::parse → DataFile
Store → Settings::from_store → Settings
DataFile + Settings → field::build → Field
Field + Settings → draw::build_scene → Scene（描画命令の列）
Scene → raster::rasterize → Pixmap（画面、PNG、BMP、AVI、クリップボードの DIB）
Scene → emf / wmf（ベクトル形式の保存）
```

`src/engine/` は egui に依存しない。`Scene` を中間表現にすることで、ビットマップとメタファイルが同じ描画コードを共有する。

## モジュール

| ファイル | 役割 |
|---|---|
| `settings.rs` | `Settings`。設定項目を型付きで持つ。`from_store(&Store)` で作る |
| `data.rs` | データファイルの解析。`parse(bytes, &DataOptions) -> DataFile` |
| `field.rs` | 格子、三角メッシュ、パネルの統一表現と補間。範囲の計算 |
| `mesh.rs` | Delaunay 分割（`delaunator`） |
| `gradient.rs` | 9 種類のグラデーション（`CBtype` の 0〜2 は jet ではなく Google の Turbo で `colorous` を使う、3〜5 はグレー、6〜8 はスペクトル）、色数、反転、折り返し |
| `scene.rs` | `Scene`、`Op`（描画命令） |
| `raster.rs` | `Scene` を `tiny_skia::Pixmap` に描く。文字は `ab_glyph` |
| `draw/mod.rs` | `build_scene`。画像サイズの決定、描画順、余白 |
| `draw/plane.rs` | 平面図：分布図、散布図、パネル、ベクトル、等高線（カーテシアンと円筒） |
| `draw/land.rs` | 俯瞰図 |
| `draw/axes.rs` | 座標軸、グリッド、目盛、データ点 |
| `draw/legend.rs` | 凡例、ラベル |
| `export/bmp.rs` | 8bit BMP、RLE8 BMP、減色 |
| `export/emf.rs`、`export/wmf.rs` | メタファイル |
| `export/avi.rs` | 無圧縮 8bit AVI |
| `pipeline.rs` | `load` と `render`。GUI と CLI の入口 |

## Scene

```rust
pub struct Scene { pub width: u32, pub height: u32, pub bg: Rgb, pub ops: Vec<Op> }
pub enum Op {
    Image { x: i32, y: i32, w: u32, h: u32, rgba: Vec<u8> },   // 分布図の本体。透明な画素は描かない
    Polyline { pts: Vec<(f32, f32)>, color: Rgb, width: f32 },
    Polygon { pts: Vec<(f32, f32)>, fill: Option<Rgb>, stroke: Option<Rgb> },
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32, fill: Option<Rgb>, stroke: Option<Rgb> },
    Text { x: f32, y: f32, text: String, font: FontRef_, color: Rgb, anchor: Anchor },
}
```

色は `Rgb(u8, u8, u8)`。線と図形はアンチエイリアスなし（8bit 保存での色数を抑えるため）。文字はフォントを `C:\Windows\Fonts` から探す。全角の「ＭＳ ゴシック」等を実ファイルに対応づけ、無ければ代替フォント、それも無ければ文字を描かない。読み込んだフォントはキャッシュする。

## データファイル

- 区切りはスペース、タブ、コンマの混在可。E 型可。改行は CR+LF、LF、CR のいずれも可。文字コードは Shift-JIS（UTF-8 も可）。
- 読み飛ばす行：空行、先頭が `!"#$%&'()*` のいずれかの行、英字で始まる行、数値として解釈できない行。
- 数値行より前に書ける指定（大文字小文字を区別しない）：
  - `Labels x,y,文字列`：ラベル。座標は画像左上を原点とする画素。
  - `IncludeINI ファイル名`：設定ファイル。データファイルからの相対パス。
  - `[AVESE.EXE]` から次のセクションまで：設定の埋め込み（`キー=値`）。
  - `Mesh AutoCreate`：三角メッシュを自動生成する。
- 優先順位は 埋め込み > IncludeINI > 現在の設定。`CBLoadConf` がオンのときだけ反映し、Store に書き戻す。
- `SEskip` は先頭のデータ行を読み飛ばす行数。
- Z に `-` を与えた点は欠損。その点に接するセルを描かない。
- `SEdataX/Y/Z/U/V` は 1 始まりの列番号。U、V、Z の列が無い行は該当値が無いものとして扱う。
- パネル状のときは 1 行が 1 行分の Z の並びになる。`CBtenchi` で転置する。
- `CBDataCheck` がオンで、格子状に並んでいないときは警告を返す（読み込みは続行可能）。

## 画像サイズ

- `CBmode` 0 なら `SEsize` が幅、1 なら高さ。もう一方はデータの縦横比から決める。`EDh`（横倍率）、`EDv`（縦倍率）を掛ける。円筒座標では倍率を無視して正方形にする。
- `SEmar` を上下左右の余白に足す。凡例は下側に領域を追加する。
- `SEmaxbmp`（MB）を幅×高さのバイト数が超えるときは、描画前に警告する。

## 平面図

- 分布図：画素ごとにセルを求めて双線形補間する。`CBSpline` がオンなら行方向、列方向の順に 3 次スプラインで補間する。欠損を含む行を境に完全な行の連続区間を分け、欠損行に接する有効なセルは双線形に戻す。
- Z の範囲：`CBenZmin`、`CBenZmax` で指定。範囲外は端の色に丸める。
- 散布図：`SEdotsize`、`CBscattemode`（■●□○）。等高線は描かない。
- パネル：セルごとに一色。等高線とベクトルは描かない。行の先頭が画像の上。
- 三角メッシュ：三角形ごとに重心座標で補間する。
- ベクトル：基準長は `SEsize / 20`。`EDvector` はその百分率。最大の大きさ（`CBenVMax` なら `EDenVMax`）が基準長になる。方向は倍率の影響を受けず、成分に忠実に描く。間引きは `CBskipx/y`、`EDstepx/y`、`EDoffsetx/y`。矢印は `CBvectorhead`、`EDhead`（%）、`SEvectorheadangle`、`CBfix`（最大の矢印に揃える）、`CBpaint`（`SPvectorheadcolor` で塗る）。`CBcolorvector` なら大きさで色分けする。
- 等高線：補間した画素の値を縦横に走査し、隣接する値が等高線の値をまたぐ画素を塗る。欠損値の境界はまたがない。`RBctmode1` は `SEtoko` 本を等分、`RBctmode2` は `EDdt` の整数倍。範囲は `CBentokomin/max`。色は `SPtokocolor`。
- 円筒座標：通常データは列 1 がθ、列 2 が r。パネル状データでは横方向がθ、縦方向が r で、先頭行が外周、末尾行が中心に来る。θの最小から最大を 0〜2π に対応づけ、周方向は閉じる。通常データの r の最小値が中心。ベクトルはθ、r 成分を変換して描く。

円筒パネルは原作 v0.93 の `sample3.txt` をパネルとして読み込み、サイズ 300、余白 30、グレースケール、凡例ありの条件で比較した。原作画像との画素差（輝度差24超、画像全体、±4ピクセルで位置合わせ）は、行順と画素位置の修正前の 34.18% から 8.47% に減った。残差は細い同心円や文字周辺にあり、画素単位の一致は目標としていない。

## 俯瞰図

- 斜投影（原作を実測）。底面の Y 軸は上へ `cos φ` 倍に縮み、横へ `tan η`（|η| は 45° まで）の割合でずれる。X 軸と高さは縮まない。高さは Z の全範囲が `SElandheight` 画素になる。円筒座標では η を無視する。
- 面は奥から順に描く。色の段差が見えない細かさまで高さの差に応じてセルを分割する（命令数の上限を設ける）。
- `CBsurface` がオフなら、面に加えて、手前を向いた外周と欠損の縁から床まで高さの色の壁を立てる。円筒座標の外周は円弧に沿わせ、θ の継ぎ目には隣の列が欠損のときだけ立てる。壁は多くても 15,000 面（`MAX_POLYS / 4`）で、超える分は等間隔に間引く。パネルは壁を立てず、面と線の表示を切り替える（原作の俯瞰図はパネルを平面のまま描くが、本実装は #30 で立体にした）。
- `CBgrid` と `CBgridmesh` のメッシュ線はデータの格子点を通り面の上を走る。線の高さは面と同じ双線形で求める。三角形メッシュの辺が 14,400 本を超えるときは全体から等間隔に選び、線の分割は画像が大きいほど粗くして全体で約 30,000 区間に抑える。`CBgridbox` は箱を、`CBgridscale` は目盛を手前に重ねる。円筒座標ではメッシュ線を描かない。
- 等高線、ベクトルは描かない。散布図、データ点、座標軸、グリッド、凡例は描く。

### 大規模データの俯瞰図

上限を超える格子はブロック内の極値を残して縮約し、欠損をまたぐ面を作らないようにします。欠損が多い場合や縮約できない場合は元の有効セルを描くため、描画命令数が上限を超える場合があります。パネルには高さの異なる隣接セルの間と外周に側面を描きます。

上限を超える三角メッシュは、欠損がなければ境界とブロックごとの高さの極値を保って点を選び再三角形分割します。境界点だけで安全な上限を超える場合や、欠損値を含む場合は、元の有効な三角形をすべて描くため、命令数と処理時間が上限を超える場合があります。

パネルは間引くときに極値を残さないので、1 セルだけの山は消えることがあります。

欠損のないデータでの実測です（リリースビルド、3 回の中央値、`SEsize=800`、面表示オン）。

| データ | 描画ポリゴン | PNG 時間 | EMF | WMF |
|---|---:|---:|---:|---:|
| 格子 3 万セル | 29,933 | 0.16 秒 | 4.2 MB | 2.5 MB |
| 格子 5.9 万セル | 59,540 | 0.30 秒 | 8.3 MB | 5.0 MB |
| 格子 6.1 万セル（縮約） | 13,958 | 0.15 秒 | 1.9 MB | 1.1 MB |
| 格子 24 万セル（縮約） | 13,954 | 0.19 秒 | 1.9 MB | 1.1 MB |
| 格子 100 万セル（縮約） | 17,124 | 0.44 秒 | 2.3 MB | 1.4 MB |
| メッシュ 5 万三角形 | 49,302 | 0.28 秒 | 6.6 MB | 3.9 MB |
| メッシュ 12 万三角形（縮約） | 52,066 | 0.33 秒 | 7.0 MB | 4.1 MB |
| メッシュ 50 万三角形（縮約） | 55,442 | 0.64 秒 | 7.4 MB | 4.3 MB |
| パネル 6 万セル | 20,231 | 0.13 秒 | 2.9 MB | 1.7 MB |
| パネル 24 万セル | 28,887 | 0.19 秒 | 4.1 MB | 2.4 MB |

縮約に meshoptimizer は使いません。高さの誤差は小さくなりますが、C++ への依存が増え、z のスケーリングが必要で、裏返る三角形も出たためです。

## 原作サンプルを使う検証

原作のファイルは配布物に含めないため、サンプルを使うテストは通常はスキップします。手元に原作サンプルがある場合は、`AVESE_ORIGINAL_DIR` にそのディレクトリを指定して `cargo test original_ -- --ignored` を実行できます。指定したファイルが不足している場合はテストが失敗します。

## 座標軸、グリッド、凡例

- 座標軸：原点が範囲内のときだけ描く。`SEaxislinewidth`、`SEaxisheadsize`（0 で矢印なし）、`CBaxsiscross`（十字）、`CBaxistitle`、`EDXtitle/Y/Z`。十字でないときは原点から正の向きだけ描く。
- グリッド：`CBgridmesh`（全線）、`CBgridbox`（外枠）、`CBgridscale` と `SEgridlinelength`（目盛の長さ。負なら外向き）、`CBaxisscale`（数字）、`SEgridlinewidth`。間隔は `CBmemoryX/Y/Z`（自動、×10〜×0.1）。標準間隔は範囲に対する 1、2、5 系のきりのよい値。円筒座標では同心円と放射線になる。
- 凡例：`CBlegend`、`SElegendw`、`SElegendh`（最小 3）。前景色の枠と最小値、最大値を描く。
- ラベル：`CBLabel`。データファイルの `Labels` を描く。
- フォントは `LBfont` の 4 行（ラベル、凡例、軸タイトル、軸目盛）。色は `SPFGcolor`、背景は `SPBGcolor`。

## 出力

- BMP：8bit パレット。色数が 256 を超えるときは Median Cut で減色する。RLE8 も書ける。
- PNG：`image` クレート。
- EMF、WMF：`Scene` をレコードに変換する。`Op::Image` は `StretchDIBits`。WMF は Placeable ヘッダ付き。
- AVI：無圧縮 8bit。全フレーム共通のパレットを使う。サイズが異なるフレームは指定サイズに拡大縮小する。
- クリップボード：`RBusebmp` なら CF_DIB、`RBusewmf` なら CF_ENHMETAFILE（Windows のみ）。

## 色の表記

Store の `TColor` は BGR（`0x00BBGGRR`）。`Settings` で `Rgb` に変換する。

文字描画の差異と改善方針は [TEXT_RENDERING.md](TEXT_RENDERING.md) を参照。
