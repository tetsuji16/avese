# avese

[English](README.md) | 日本語

avese は、数値計算の結果などの 3 次元データを、分布図、ベクトル図、等高線図、俯瞰図にするソフトです。テキストファイルをウィンドウに落とすだけで図になります。

**[使い方ガイド（動画つき）](http://o-server.main.jp/avese/ja/)** ・ **[ダウンロード](https://github.com/tetsuji16/avese/releases/latest)**

[![avese の画面。ベクトルと等高線を重ねた分布図。画像を押すと使い方ガイドを開きます](site/media/quick.png)](http://o-server.main.jp/avese/ja/#start)

| 分布図 | ベクトル | 等高線 |
|:---:|:---:|:---:|
| ![分布図](site/media/g-dist.png) | ![ベクトル](site/media/g-vector.png) | ![等高線](site/media/g-contour.png) |
| **俯瞰図** | **円筒座標** | **三角メッシュ** |
| ![俯瞰図](site/media/g-land.png) | ![円筒座標](site/media/g-cyl.png) | ![三角メッシュ](site/media/g-mesh.png) |

## はじめる

1. [最新のリリース](https://github.com/tetsuji16/avese/releases/latest)から Windows (x64) 用の ZIP をダウンロードして展開し、`avese.exe` を起動します。インストールは要りません。ほかの OS では[ソースからビルド](#ビルド)してください。
2. データファイルをウィンドウにドラッグ＆ドロップします。試すデータは[サンプルデータ](http://o-server.main.jp/avese/samples/samples.zip)にあります。
3. ツールバーで表示を切り替えて「再描画」を押します。設定ダイアログでの変更は OK で反映されます。
4. 「セーブ」で画像を保存します。複数のファイルをまとめてドロップすると「連続処理」が開き、BMP や AVI を作れます。

データはテキストで、1 行が 1 データ点です。値は空白、タブ、コンマで区切ります。

```
# X  Y  Z  U  V
0  0  1.5  0.2  0.1
1  0  1.8  0.3  0.1
```

列の割り当て、穴の書き方、ラベル、設定の埋め込みなどは[使い方ガイド](http://o-server.main.jp/avese/ja/#data)で説明しています。

## 機能

- **データ**: 格子、三角メッシュ（Delaunay 自動生成）、パネル。設定の埋め込み、`IncludeINI`、`Labels`、Shift-JIS と UTF-8。
- **平面図**: 分布図（線形、3 次スプライン）、散布図、パネル、ベクトル、等高線。カーテシアン座標と円筒座標。
- **俯瞰図**: 面と格子線。視点の指定。
- **装飾**: 座標軸、グリッド、目盛、データ点、凡例、ラベル、9 種類のカラースケール（既定は Google の Turbo）。
- **出力**: BMP（8bit、RLE8）、PNG、EMF、WMF、無圧縮 8bit AVI、クリップボード（DIB、拡張メタファイル）。連続処理での BMP と AVI の作成。
- **GUI**: メニュー、ツールバー、設定ダイアログ、連続処理、ラベルの追加、進捗表示、ステータスバーの座標表示、ドラッグ＆ドロップ、INI の読み書き、内蔵ヘルプ。

UI は OS の言語設定に合わせて日本語/英語を自動選択します。Windows はユーザーの地域設定、macOS は AppleLocale、Linux/Unix は `LC_ALL` / `LC_MESSAGES` / `LANG` を参照します。英語以外は日本語で表示します。

設計は [docs/RENDERING.md](docs/RENDERING.md) にあります。原作との画素単位の一致は目標にしていません。 画面の文字には、利用可能な日本語 sans フォントを優先します。描画画像の文字は設定で選択できます。

## コマンドライン

```
avese データ.txt
avese --render データ.txt 出力.png [--rle] [--ini 設定.ini] [--set キー=値]...
avese --avi 出力.avi --fps 10 [--frame-size 320x240] [--ini 設定.ini] [--set キー=値]... データ1.txt データ2.txt ...
```

`--render` の出力形式は拡張子（`.png`、`.bmp`、`.emf`、`.wmf`）で決まります。

読み込んだ点データは「再描画」→「三角メッシュを生成」で三角メッシュに変換できます。元データに `Mesh AutoCreate` を指定する方法も利用できます。

ラベルは「設定」→「詳細設定」→「ラベル」の「追加」から入力できます。GUI で追加したラベルは設定とともに保存されます。原作のヘルプではラベルをデータファイルに直接記述する方式で、この GUI 操作は本再実装の拡張です。

## ビルド

```
cargo build --release
cargo test
```

使い方ガイドは [site/](site/) にあります。

### 原作のサンプルを使う検証

原作のサンプルと ini を手元に置くと、パーサ・補間・描画・INI の往復を原作のデータで確認できます。`AVESE_ORIGINAL_DIR` に `sample1.txt`〜`sample6.txt` と `sample4.ini` のあるフォルダを指定してください（既定は `original avese`）。

```
AVESE_ORIGINAL_DIR=/path/to/samples cargo test -- --ignored
```

このフォルダは著作物のためリポジトリに含めません。指定しない場合、これらのテストは `ignored` のまま飛ばされます。

## アセットの取り出し（任意）

アプリのアイコンは同梱されているため、原作の exe は実行に不要です。原作の資料を調べる場合は、手元の exe から次のコマンドで `assets/local/`（git 管理外）に画像を取り出せます。

```
python -I tools/extract_assets.py
python -I tools/export_forms.py
```

ツールバーの一般操作には Phosphor Icons、グラフ操作には AI で生成した独自画像や描画した記号を使い、2D・3D は文字で表示します。アプリのロゴには生成した独自画像を使います。`assets/forms/` のフォーム JSON は、原作の DFM から取り出したラベルや座標などのレイアウトで、画像などのバイナリは含みません。

## ライセンス

[MIT License](LICENSE) です。

- 原作の資産は MIT の対象外です。原作の exe、ヘルプ、サンプル、取り出した画像は、このリポジトリに含めないでください（`.gitignore` 済み）。
- MS ゴシック等のフォントは同梱しません。実行時にインストール済みフォントを検索し、日本語フォントを利用します。
- 一般操作アイコンは [Phosphor Icons](https://phosphoricons.com/)（MIT）を使っています。
- 等高線の追跡は [contour](https://crates.io/crates/contour)、Turbo の色は [colorous](https://crates.io/crates/colorous) を使っています。

## 既知の差異

- 描画は画素単位で原作と一致しません。文字の字形も GDI と異なります。
- `pixels_per_point` は OS の値のままで、倍率の高い画面では原作と物理サイズが異なります。
- 連続処理中は、Windows では進捗画面を表示してからメインウィンドウを隠します。他 OS ではメインウィンドウを表示したままです。
- 設定ダイアログの `TBeta`/`TBphi` と `SEeta`/`SEphi` は 1 対 1 で連動します。DFM の範囲（-45～45、0～85）・ヒント・既定値（30）が一致するため、変換式は恒等としています。角度プレビュー（PBangle）は原作の実測に合わせた斜投影で、前面の大きさは変わらず、奥の面が η で横に、φ で縦にずれます（φ=60、70 で原作と 1 画素差）。
- ダイアログは OS ウィンドウで、Windows ではオーナー設定とメイン無効化でモーダルにしています。他 OS ではメインの入力を無視する方式で、色・フォントの選択にはアプリ内の画面を使います。
- 太字と斜体は、そのフォントに実書体があれば実書体を使い、ないものだけを二重打ちと傾きで合成します（詳しくは [docs/TEXT_RENDERING.md](docs/TEXT_RENDERING.md)）。ヒンティングはしないので、字形は GDI と完全には一致しません。
- GUI で追加したラベルは INI に保存し、同じ設定を使うデータに表示します。データファイルには書きません。ラベルは Unicode を保持するため、INI 内で UTF-8 の 16 進表記にしています。
- 描画は進捗ウィンドウから中止できます。一部の等高線・ベクトル処理では、中止まで時間がかかる場合があります。
- クリップボードは Windows のみです。

---

<sub>avese は、AV似非 (AVS を簡素化したソフト) を Rust で書き直したものです。AVS および AV似非 の開発元とは関係ありません。</sub>
