# avese GUI 仕様（原作 AV似非 v0.93 の実測にもとづく）

対象は GUI のみ。描画エンジン（データ解析、作図、画像/AVI 保存）は後続 Issue。
原作は Delphi VCL Win32。フォーム定義は `assets/forms/*.json`（DFM 由来、バイナリ除去済み）で確認する。

## 確定した設計
- egui/eframe 0.37（glow）。カスタム描画ウィジェットによる VCL 風レンダラー。絶対座標、feathering オフで 1px ベベルを鮮明に。
- モジュール: `main.rs` `forms.rs`（JSON 読み込み）`vcl`（ウィジェット）`state.rs`（コントロール名=INI キーのストア: checked/text/item_index/position/color/enabled/readonly）`ini.rs` `view.rs`（連動ルール）`fonts.rs` `app.rs` `platform_win32.rs`。
- TEdit = 枠なし TextEdit を凹みベベル内に置く（IME・コピペのため）。TSpinEdit = edit + 3D ボタン2個（SEdata* は EditorEnabled=false）。コンボのドロップダウンは Area。
- ツールチップ: Hint の `|` より前を、約500ms 後にクラシックな黄色で表示。タブ順は TabOrder。点線フォーカス枠。Esc=Cancel、Enter=Default ボタン。`&x` は下線とアクセラレータ。
- メニュー: Area によるカスタムポップアップ。アイコン欄あり、チェック付き画像項目はアイコンを凹ませ、画像なし項目はチェック記号、ショートカット文字を右側に。
- ダイアログ: `show_viewport_immediate` で実 OS ウィンドウ。Windows では windows-sys でオーナー設定（GWLP_HWNDPARENT）、`EnableWindow(main,false)` で真のモーダル、bsDialog は WS_EX_DLGMODALFRAME でアイコンなし。HWND はスレッドのウィンドウをタイトルで列挙して探す。他 OS はメイン入力を無視するだけ。
- 色/フォント選択: Win32 の ChooseColorW/ChooseFontW（他 OS は Issue）。ファイル選択は rfd。
- 色: clBtnFace (240,240,240)、shadow 160、dk shadow 105、highlight 255、3dlight 227。参照 PNG で調整。
- フォント: 実行時に `C:\Windows\Fonts\msgothic.ttc`（0=MS Gothic, 1=MS UI Gothic, 2=MS PGothic）、msmincho。無ければ Meiryo/Yu Gothic/Noto/IPA/Hiragino。MS フォントは同梱しない。GDI と同一にはならない（README に明記）。
- 無効ツールバーアイコン: エンボス（暗部→shadow、白を (1,1) ずらす）。
- キャンバス: GUI 段階は空白。ファイル読込でタイトル更新と再描画の有効化のみ。保存/クリップボード/AVI/BMP はダイアログ後に「未実装」と正直に表示（黙って失敗しない）。ヘルプ: exe 横の avese.htm を開く、無ければメッセージ。
- ControlBar のバンドはドラッグ可、RowSize 30 で行スナップ、位置を ToolBarNTop/Left に保存。ステータスバー 3 パネル 100,100,残り、サイズグリップ付き。

## ライセンス上の制約（必須）
原作はフリーウェアで全権利は作者おりいる氏。OSS ではない。
`original avese/`、exe、ヘルプ、サンプル、抽出した画像（ツールバーアイコン ImageList、アプリアイコン、About 画像）はコミットしない。
`tools/` の標準ライブラリのみの Python スクリプトが gitignore 済みの `assets/local/` へ抽出する。アプリは実行時にそれがあれば読み、無ければコード描画の代替アイコンを使う。コミットするのはレイアウト/キャプション由来データのみ。
About には原作クレジットに加えて小さく「非公式 Rust 再実装」を x100,y97 付近に追加（意図的な差分）。LICENSE は置かない（Issue で扱う）。

## フォーム要約（詳細は JSON）
- Form1 "AV似非 - nodata" クライアント 552x354、MS Pゴシック -12、StatusBar1 h22、ControlBar1 alTop h64 RowSize30、ScrollBox1 alClient + Image1。ツールバーは JSON 参照。
- Dlgconfig 「設定」 401x320; PageControl 5,5,391x276、マルチライン、TabHeight 18、初期 TSlegend。OK 240,290 74x24、ｷｬﾝｾﾙ 319,290。タブ行: 俯瞰図 分布図 ベクトル 等高線 座標系 グリッド / 凡例 ラベル 入出力 表示 フォント 特殊 暫定。選択タブの行が下に移動する。
- dlgAVI 連続処理 326x396（最小 300x200）Panel1 alBottom h100、savelist client。dlginputstr ラベルの追加 384x71。AboutBox ﾊﾞｰｼﾞｮﾝ情報 292x229。MeshProgress 三角メッシュ自動生成 280x81。
- ファイルフィルタ: 開く "AV似非データファイル(*.txt;*.dat)|テキストファイル(*.txt)|データファイル(*.dat)|すべてのファイル(*.*)" 複数選択; 保存 "ビットマップファイル(*.bmp)|RLE 圧縮 ビットマップファイル(*.bmp)|ウィンドウズメタファイル(*.wmf)|エンハンストメタファイル(*.emf)|PNGファイル(*.png)" 既定名 default; 設定 "設定ファイル(*.ini)" 既定名 avese 拡張子 ini; AVI "AVI-Files (*.avi)" 既定名 default。

## 連動ルール（メニュー/ツールバー。単体テスト必須）
初期: flat, cart, graph, legend, label。状態: land(bool)、cart/cyl、graph/scatter/panel は排他だが全オフ可。
- flat か land をクリック: land をトグル。land ON で vector, toko, panel を消す。OFF では何も消さない。
- cart/cyl: ラジオ。
- graph: トグル、ON で scatter, panel を消す。
- scatter: トグル、ON で graph, panel, toko を消す。
- panel: トグル、ON で graph, scatter, vector, toko, point, land を消す。
- vector: トグル、ON で panel, land を消す。
- toko: トグル、ON で scatter, panel, land を消す。
- point: トグル、ON で panel を消す。
- axis, grid, legend, label: 独立。

## ダイアログ内の連動（メニュールールとは別）
- ダイアログのチェックにも、上のメニュー連動ルールと同じ「ON で消す」関係がその場で効く（全 13 項目の組合せを実測済み）。例: パネル状 ON で 分布図・散布図・ベクトル・等高線・データ点・俯瞰図が OFF、俯瞰図 ON でベクトル・等高線・パネル状が OFF。ダイアログの 平面図/俯瞰図 は 俯瞰図 チェック 1 個。OK 後にメニューへ反映。
- CBenZmin/CBenZmax、CBentokomin/max、CBenVMax は対応 TEdit の ReadOnly と Color（読取専用時 clBtnFace）を切替。
- CBaxisscale ON で CBgridscale も ON。RBctmode 排他、RBcartesian/RBcylindrical 排他。表示タブの X/Y min/max は常に読取専用。
- Cancel は変更を戻す。再度開くとタブ位置を保持。

## 起動・終了
- 起動時 再描画無効、他は有効。1ファイル読込でタイトル "AV似非 - sample1.txt"、再描画有効。
- コマンドライン複数ファイル: 連続処理ダイアログにファイル一覧。原作はメイン非表示だが eframe のルートを隠すと更新が止まるため表示のまま（差分として明記）。
- CBautosave=0 なら終了時 ini を作らない。=1 のときのみ自動保存。設定の保存は exe 横の `avese.ini`、セクション `[avese.exe]`、Shift-JIS。

## INI キー順（厳密）
Form1Top, Form1Left, Form1height, Form1width, ToolBar1Top, ToolBar1Left, ToolBar2Top, ToolBar2Left, CBmode, SEsize, EDv, EDh, ToolBar3Top, ToolBar3Left, ToolBar4Top, ToolBar4Left, ToolBar5Top, ToolBar5Left, CBland, SElandheight, SEeta, SEphi, CBsurface, CBgraph, CBscatter, CBpanel, SEdotsize, CBscattemode, SPvectorcolor, SPvectorheadcolor, EDvector, EDhead, EDstepx, EDoffsetx, CBskipx, CBskipy, EDstepy, EDoffsety, CBfix, CBpaint, SEvectorheadangle, CBenVMax, EDenVMax, CBvectorhead, CBvector, CBtoko, SPtokocolor, EDdt, RBctmode1, RBctmode2, CBentokomin, CBentokomax, EDtokomin, EDtokomax, SEtoko, CBaxis, CBaxistitle, EDXtitle, EDYtitle, EDZtitle, SEaxisheadsize, SEaxislinewidth, CBaxsiscross, RBcartesian, RBcylindrical, CBgrid, SEgridlinewidth, SEgridlinelength, CBgridmesh, CBgridbox, CBgridscale, CBaxisscale, CBmemoryX, CBmemoryY, CBmemoryZ, CBpoint, CBLabel, LBlabel(-1), SEdataX, SEdataY, SEdataU, SEdataV, SEdataZ, CBtenchi, SEskip, SPBGcolor, SPFGcolor, EDenYmin, EDenYmax, EDenXmin, EDenXmax, SEmar, LBfontlabelfontname/size/style, LBfontlegendfontname/size/style, LBfontAxisfontname/size/style, LBfontMemfontname/size/style, LBfont(3), CBautosave, SEmaxbmp, RBusebmp, RBusewmf, CBDataCheck, CBLoadConf, CBSpline, CBcolorvector, CBlegend, CBreverse, CBtype, CBfold, EDenZmin, EDenZmax, CBenZmin, CBenZmax, SEcolor, SElegendw, SElegendh

色は 10 進 TColor（BGR）。既定値は DFM 値。フォント既定: ラベル ＭＳ Ｐゴシック 14、凡例 ＭＳ ゴシック 12、軸タイトル ＭＳ Ｐ明朝 12、軸目盛 terminal 6（LBfont 項目は `名\tフォント\tサイズ\tstyle`、列はベベル幅 103/150/114 のオーナー描画）。
