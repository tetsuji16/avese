# avese（非公式 Rust 再実装）

Delphi 製のフリーウェア「AV似非」（作者: おりいる）の機能を、Rust と egui（eframe 0.37）で再実装したものです。
**非公式**であり、原作者・原作とは関係ありません。原作の exe には実行時に依存しません。

## 機能

- **データ**: 格子、三角メッシュ（Delaunay 自動生成）、パネル。設定の埋め込み、`IncludeINI`、`Labels`、Shift-JIS と UTF-8。
- **平面図**: 分布図（線形、3 次スプライン）、散布図、パネル、ベクトル、等高線。カーテシアン座標と円筒座標。
- **俯瞰図**: 面と格子線。視点の指定。
- **装飾**: 座標軸、グリッド、目盛、データ点、凡例、ラベル、9 種類のカラースケール（既定は Google の Turbo）。
- **出力**: BMP（8bit、RLE8）、PNG、EMF、WMF、無圧縮 8bit AVI、クリップボード（DIB、拡張メタファイル）。連続処理での BMP と AVI の作成。
- **GUI**: メニュー、ツールバー、設定ダイアログ、連続処理、ラベルの追加、進捗表示、ステータスバーの座標表示、ドラッグ＆ドロップ、INI の読み書き、内蔵ヘルプ。

設計は [docs/RENDERING.md](docs/RENDERING.md) にあります。原作との画素単位の一致は目標にしていません。

## 使い方

```
avese データ.txt
avese --render データ.txt 出力.png [--rle] [--ini 設定.ini] [--set キー=値]...
avese --avi 出力.avi --fps 10 [--frame-size 320x240] [--ini 設定.ini] [--set キー=値]... データ1.txt データ2.txt ...
```

`--render` の出力形式は拡張子（`.png`、`.bmp`、`.emf`、`.wmf`）で決まります。

## ビルド

```
cargo build --release
cargo test
```

## アセットの取り出し（任意）

アプリのアイコンは同梱されているため、原作の exe は実行に不要です。原作の資料を調べる場合は、手元の exe から次のコマンドで `assets/local/`（git 管理外）に画像を取り出せます。

```
python -I tools/extract_assets.py
python -I tools/export_forms.py
```

ツールバーの一般操作には Phosphor Icons、グラフ操作とアプリのロゴには生成した独自画像を使います。`assets/forms/` のフォーム JSON は、原作の DFM から取り出したラベルや座標などのレイアウトで、画像などのバイナリは含みません。

## ライセンス

[MIT License](LICENSE) です。

- 原作の資産は MIT の対象外です。原作の exe、ヘルプ、サンプル、取り出した画像は、このリポジトリに含めないでください（`.gitignore` 済み）。
- MS ゴシック等のフォントは同梱しません。実行時に `C:\Windows\Fonts` から読み込みます。
- 一般操作アイコンは [Phosphor Icons](https://phosphoricons.com/)（MIT）を使っています。
- 等高線の追跡は [contour](https://crates.io/crates/contour)、Turbo の色は [colorous](https://crates.io/crates/colorous) を使っています。

## 既知の差異

- 描画は画素単位で原作と一致しません。文字の字形も GDI と異なります。
- `pixels_per_point` は OS の値のままで、倍率の高い画面では原作と物理サイズが異なります。
- 連続処理中は、Windows では進捗画面を表示してからメインウィンドウを隠します。他 OS ではメインウィンドウを表示したままです。
- 設定ダイアログの `TBeta`/`TBphi` と `SEeta`/`SEphi` は同じ値で連動させています（原作の変換式は未確認）。角度プレビュー（PBangle）は近似の枠線です。
- ダイアログは OS ウィンドウで、Windows ではオーナー設定とメイン無効化でモーダルにしています。他 OS ではメインの入力を無視する方式で、色・フォントの選択にはアプリ内の画面を使います。
- 太字は二重打ちで近似し、斜体は描きません。
- GUI で追加したラベルは INI に保存し、同じ設定を使うデータに表示します。データファイルには書きません。ラベルは Unicode を保持するため、INI 内で UTF-8 の 16 進表記にしています。
- 描画は進捗ウィンドウから中止できます。一部の等高線・ベクトル処理では、中止まで時間がかかる場合があります。
- クリップボードは Windows のみです。
