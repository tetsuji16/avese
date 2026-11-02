//! Small built-in Japanese/English UI catalog. Data and setting keys stay locale-neutral.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    Japanese,
    English,
}

impl Language {
    pub fn detect() -> Self {
        static DETECTED: std::sync::OnceLock<Language> = std::sync::OnceLock::new();
        *DETECTED.get_or_init(|| {
            let locale = system_locale().unwrap_or_default().to_ascii_lowercase();
            if locale.starts_with("en") {
                Self::English
            } else {
                Self::Japanese
            }
        })
    }

    pub fn is_english(self) -> bool {
        self == Self::English
    }
}

#[cfg(windows)]
fn system_locale() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    (len > 1).then(|| String::from_utf16_lossy(&buffer[..(len - 1) as usize]))
}

#[cfg(not(windows))]
fn system_locale() -> Option<String> {
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
}

/// Translate a Japanese source string. Unknown strings are preserved to avoid losing data.
pub fn text(source: &str) -> String {
    text_for(Language::detect(), source)
}

pub fn text_for(language: Language, source: &str) -> String {
    if !language.is_english() {
        return source.to_owned();
    }
    if let Some(translated) = english(source) { return translated.to_owned(); }
    // Errors often prefix a translated engine message with a path or record number.
    let mut translated = source.to_owned();
    for (japanese, english) in [
        ("データがありません", "No data is available"),
        ("格子が小さすぎます (2x2 以上が必要)", "The grid is too small (at least 2 × 2 is required)"),
        ("三角形分割には 3 点以上が必要です", "At least three points are required for triangulation"),
        ("座標が数値ではない点があります", "Some points have non-numeric coordinates"),
        ("点が一直線上に並んでいるため、三角形を作れません", "The points are collinear and cannot form triangles"),
        ("設定ファイルを読み込めません", "Could not load the settings file"),
        ("設定を保存できません", "Could not save settings"),
        ("ヘルプを書き出せません", "Could not write the help file"),
        ("ブラウザを起動できません", "Could not launch the browser"),
        ("クリップボードを開けません（他のアプリが使用中です）", "Could not open the clipboard (another app may be using it)"),
        ("クリップボードを空にできません", "Could not clear the clipboard"),
        ("クリップボードにコピーできません", "Could not copy to the clipboard"),
        ("AVI にするフレームがありません", "There are no frames to write to the AVI"),
        ("フレームレートが小さすぎます", "Frame rate is too low"),
        ("フレームレートが範囲外です（0 より大きく 1000 以下）", "Frame rate is out of range (must be greater than 0 and at most 1000)"),
        ("WMF に書き出せない数の頂点を持つ多角形があります", "A polygon has too many vertices to export to WMF"),
        ("画像が大きすぎて WMF に書き出せません（上限 32766 ピクセル）", "The image is too large to export to WMF (limit: 32,766 pixels)"),
        ("処理中にエラーが発生しました", "An error occurred during processing"),
        ("描画中にエラーが発生しました", "An error occurred while rendering"),
    ] {
        translated = translated.replace(japanese, english);
    }
    translated
}

pub fn english_mode() -> bool {
    Language::detect().is_english()
}

#[allow(unreachable_patterns)]
pub fn english(source: &str) -> Option<&'static str> {
    Some(match source {
        "設定" => "Settings",
        "連続処理" => "Batch Processing",
        "ラベルの追加" => "Add Label",
        "バージョン情報" | "ﾊﾞｰｼﾞｮﾝ情報" => "About avese",
        "処理中" => "Working",
        "ファイル（&F）" => "&File",
        "設定（&O）" => "&Settings",
        "ヘルプ（&H）" => "&Help",
        "データロード(&L)..." => "Load Data(&L)...",
        "再読み込み" => "Reload",
        "再描画（&R）" => "Redraw(&R)",
        "画像のセーブ（&S）..." => "Save Image(&S)...",
        "データを閉じる(&X)" => "Close Data(&X)",
        "クリップボードにコピー(&C)" => "Copy to Clipboard(&C)",
        "アプリを閉じる(&Q)" => "Exit(&Q)",
        "設定の読込（&L）..." => "Load Settings(&L)...",
        "設定の保存（&S）" => "Save Settings(&S)",
        "設定に名前を付けて保存（&A）..." => "Save Settings As(&A)...",
        "詳細設定（&C）..." => "Preferences(&C)...",
        "バージョン情報（&A）..." => "About(&A)...",
        "ヘルプ(&H)" | "ヘルプ（&H）" => "Help(&H)",
        "設定" => "Settings",
        "平面図(&F)" => "2D Plot(&F)",
        "俯瞰図(&B)" => "3D View(&B)",
        "分布図(&D)" => "Distribution(&D)",
        "散布図(&S)" => "Scatter(&S)",
        "パネル状(&P)" => "Panel(&P)",
        "ベクトル(&V)" => "Vectors(&V)",
        "等高線(&C)" => "Contours(&C)",
        "座標軸(&A)" => "Axes(&A)",
        "グリッド(&G)" => "Grid(&G)",
        "ポイント(&T)" => "Points(&T)",
        "凡例表示(&L)" => "Legend(&L)",
        "ラベル表示(&E)" => "Labels(&E)",
        "分布図" => "Distribution",
        "散布図" => "Scatter Plot",
        "パネル状" => "Panel Plot",
        "俯瞰図" => "3D View",
        "ベクトル" => "Vectors",
        "等高線" => "Contours",
        "グリッド" => "Grid",
        "座標軸" => "Axes",
        "座標系" => "Coordinates",
        "凡例" => "Legend",
        "ラベル" => "Labels",
        "入出力" => "I/O",
        "表示" => "Display",
        "フォント" => "Fonts",
        "特殊" => "Advanced",
        "暫定" => "Other",
        "設定" => "Settings",
        "OK" => "OK",
        "キャンセル" | "ｷｬﾝｾﾙ" => "Cancel",
        "追加" => "Add",
        "削除" => "Remove",
        "変更(&B)" | "変更(&C)" | "変更(&F)" | "変更(&H)" | "変更(&T)" | "変更(&V)" => {
            "Change"
        }
        "ソート" => "Sort",
        "↑" => "↑",
        "↓" => "↓",
        "文字列" => "Text",
        "データ入力形式" => "Data Format",
        "行のデータを読み飛ばす" => "rows from start",
        "アプリケーションを終了する時に設定を保存する" => {
            "Save settings on exit"
        }
        "再描画" => "Redraw",
        "再読み込み" => "Reload",
        "データロード(&L)..." => "Load Data(&L)...",
        "三角メッシュを生成" => "Generate Triangular Mesh",
        "三角メッシュ自動生成" => "Automatic Mesh Generation",
        "円筒座標系" => "Cylindrical Coordinates",
        "カーテシアン座標系" => "Cartesian Coordinates",
        "円筒座標系" => "Cylindrical Coordinates",
        "設定をファイルから読みこむ" => "Load settings from file",
        "設定の読込" => "Load Settings",
        "設定の保存" => "Save Settings",
        "設定に名前を付けて保存" => "Save Settings As",
        "データファイルのロード" => "Load Data File",
        "設定ファイル(*.ini)" => "Settings File (*.ini)",
        "AV似非データファイル(*.txt;*.dat)" => "avese Data Files (*.txt;*.dat)",
        "テキストファイル(*.txt)" => "Text Files (*.txt)",
        "データファイル(*.dat)" => "Data Files (*.dat)",
        "すべてのファイル(*.*)" => "All Files (*.*)",
        "BMP を作成するフォルダ" => "Folder for BMP Files",
        "AVI作成" => "Create AVI",
        "AVI作成" => "Create AVI",
        "AVI-Files (*.avi)" => "AVI Files (*.avi)",
        "ファイルの先頭部分を読み飛ばす" => "Skip rows at start of file",
        "データのチェックを行う" => "Validate data",
        "スプラインで補間する" => "Use spline interpolation",
        "分布図" => "Distribution Plot",
        "座標軸を描く" => "Draw axes",
        "グリッドを描く" => "Draw grid",
        "ベクトル線を描く" => "Draw vectors",
        "等高線を描く" => "Draw contours",
        "面で表示" => "Show surface",
        "データ点を表示" => "Show data points",
        "ラベルを描く" => "Draw labels",
        "凡例を表示する" => "Show legend",
        "軸タイトルを付ける" => "Show axis titles",
        "軸目盛の数字" => "Axis Tick Labels",
        "軸目盛" => "Axis Ticks",
        "軸タイトル" => "Axis Titles",
        "X軸/R軸" => "X / R Axis",
        "Y軸/θ軸" => "Y / θ Axis",
        "Y軸" => "Y Axis",
        "X" => "X",
        "Y" => "Y",
        "Z" => "Z",
        "X  高さ" => "X Height",
        "Xの最大値" => "X Maximum",
        "Xの最小値" => "X Minimum",
        "Yの最大値" => "Y Maximum",
        "Yの最小値" => "Y Minimum",
        "Z軸" => "Z Axis",
        "Zの最大値を指定" => "Set Z Maximum",
        "Zの最小値を指定" => "Set Z Minimum",
        "Z値の範囲（最大）" => "Z Range (Maximum)",
        "Z値の範囲（最小）" => "Z Range (Minimum)",
        "前景色" => "Foreground Color",
        "背景色" => "Background Color",
        "色" => "Color",
        "サイズ" => "Size",
        "サイズ指定" => "Set Size",
        "形状" => "Shape",
        "点の形状" => "Point Shape",
        "横幅" => "Width",
        "長さ" => "Len.",
        "横倍率" => "Horizontal Scale",
        "縦倍率" => "Vertical Scale",
        "上下左右の余白" => "Margin",
        "角度" => "Angle",
        "度" => "degrees",
        "俯瞰図の高さ" => "3D View Height",
        "散布図の点の大きさ" => "Scatter Point Size",
        "使用する色の数" => "Number of Colors",
        "グラデーションのタイプ" => "Gradient Type",
        "折り返す" => "Wrap Colors",
        "反転する" => "Reverse Colors",
        "基準値を指定" => "Use Reference",
        "等高線の数" => "Contours",
        "等高線の間隔" => "Contour Interval",
        "等高線の色" => "Contour Color",
        "矢印の色" => "Arrow Color",
        "ベクトルの色" => "Vector Color",
        "ベクトルの色分け" => "Color Vectors by Magnitude",
        "ベクトルの長さ" => "Vector Length",
        "矢印をつける" => "Show Arrowheads",
        "矢印の大きさを統一する" => "Use Fixed Arrowhead Size",
        "線の幅" => "Width",
        "座標軸の矢印の大きさ" => "Axis Arrowhead Size",
        "軸の太さ" => "Axis Width",
        "フレームレート（fps）" => "Frame Rate (fps)",
        "ビットマップ （8bpp）" => "Bitmap (8 bpp)",
        "ウィンドウズメタファイル" => "Windows Metafile",
        "複数データロード" => "Load Multiple Data Files",
        "先頭から" => "Skip",
        "オフセット（X）" => "X Offset",
        "オフセット（Y）" => "Y Offset",
        "場所" => "Location",
        "文字列" => "Text",
        "ラベルの追加" => "Add Label",
        "選択したフォントのサンプル" => "Selected Font Preview",
        "フォント" => "Font",
        "フォントの変更" => "Change Font",
        "変更" => "Change",
        "メッシュ" => "Mesh",
        "連続処理" => "Batch Processing",
        "AVI作成" => "Create AVI",
        "BMP作成" => "Create BMP",
        "画像のセーブ" => "Save Image",
        "イメージの取り扱い" => "Image Options",
        "ビットマップのサイズが" => "Bitmap size is",
        "M byte を超える時警告を出す" => "Warn when exceeding MB",
        "設定を変更" => "Edit Settings",
        "保存" => "Save",
        "キャンセル" => "Cancel",
        "アプリケーションを終了する時に設定を保存する" => {
            "Save settings when exiting"
        }
        "設定をファイルから読みこむ" => "Load settings from file",
        "終了時に自動保存する" => "Save automatically on exit",
        "AV似非に影響を受けています" => "Inspired by AVese",
        "Rust 再実装版（非公式）" => "Unofficial Rust reimplementation",
        "MIT License" => "MIT License",
        "ライセンス全文はリポジトリを参照してください。" => {
            "See the repository for the full license."
        }
        "Version  0.1.0" => "Version 0.1.0",
        "Sample +0.123 avese" => "Sample +0.123 avese",
        "avese" => "avese",
        "Non" => "Non",
        "max" => "max",
        "min" => "min",
        "pixels" => "pixels",
        "r" => "r",
        "ｒ" => "r",
        "η=" => "η=",
        "φ=" => "φ=",
        "θ" => "θ",
        "-" => "-",
        "%" => "%",
        "表示間隔" => "Display Interval",
        "間引く" => "Decimate",
        "毎に描く" => "Draw Every",
        "転置して読みこむ" => "Transpose on Load",
        "簡易立体表示" => "Simple 3D View",
        "立体表示の形式" => "3D Display Mode",
        "軸の形式" => "Axis Mode",
        "データ点" => "Data Points",
        "ポイント(&T)" => "Points(&T)",
        "ポイント" => "Points",
        "ポイント(&T)" => "Points(&T)",
        "コンターのグラデーションタイプを指定（3x3通り）" => {
            "Choose contour gradient (3 × 3)"
        }
        "コンターの使用する色数（2～200）" => "Number of contour colors (2–200)",
        "画像に入れる余白" => "Image Margin",
        "描画倍率設定（円筒座標では無視される）" => {
            "Plot Scale (ignored in cylindrical coordinates)"
        }
        "軸タイトルを表示する" => "Show Axis Titles",
        "軸目盛の数字を付ける" => "Show Axis Tick Labels",
        "X と Y の最大値（指定不可）" => "Maximum X and Y (read only)",
        "Z値の範囲を指定する（範囲外は白抜きになる）" => {
            "Set Z range (values outside the range are blank)"
        }
        "Zの範囲値（最大）" => "Z Range (Maximum)",
        "Zの範囲値（最小）" => "Z Range (Minimum)",
        "現在の散布データから三角メッシュを生成します" => {
            "Generate a triangular mesh from the current scatter data"
        }
        "設定に名前を付けて保存" => "Save Settings As",
        "終了時に自動保存する" => "Save automatically on exit",
        "文字列表示" => "Show Text",
        "ラベル表示する座標と文字列" => "Label coordinates and text",
        "リストへの追加" => "Add to list",
        "リストからの削除" => "Remove from list",
        "等高線の本数を指定する" => "Set number of contours",
        "等高線の間隔を指定する" => "Set contour interval",
        "等高線を描くZ値の範囲を指定する" => "Set Z range for contours",
        "矢印を塗りつぶす（塗りつぶさない場合は鉤型）" => {
            "Fill arrowheads (otherwise open)"
        }
        "矢印の先端の開き角度" => "Arrowhead Angle",
        "線分の長さに対する先端部分の大きさ" => {
            "Arrowhead size relative to line length"
        }
        "ベクトルの基準値" => "Vector Reference Value",
        "ベクトルの長さ（代表長さの20分の1）" => {
            "Vector Length (1/20 of reference length)"
        }
        "ベクトル値の大きさによって線分の長さを変更しない" => {
            "Keep vector lengths fixed"
        }
        "中央から両脇に向かって色を変える" => "Vary colors outward from center",
        "最大値から最小値までの色の順番を反転する" => "Reverse color order",
        "天頂角（0～85度）" => "Zenith angle (0–85°)",
        "X軸とY軸の角度（-45～45度）" => "X/Y axis angle (-45–45°)",
        "描画倍率設定（円筒座標では無視される）" => {
            "Plot scale (ignored in cylindrical coordinates)"
        }
        "軸の矢印の大きさ" => "Axis Arrowhead Size",
        "目盛間隔" => "Tick Interval",
        "メモリの長さ" => "Tick Length",
        "グリッドの太さ" => "Grid Width",
        "グリッドの表示形式" => "Grid Style",
        "データを閉じる" => "Close Data",
        "再読み込みするデータがありません。" => "There is no data to reload.",
        "イメージがありません。先にデータを読み込んでください。" => {
            "No image is available. Load data first."
        }
        "設定を保存できません" => "Could not save settings",
        "描画中にエラーが発生しました" => "An error occurred while drawing",
        "処理中にエラーが発生しました" => "An error occurred while processing",
        "リストにファイルがありません。" => "The file list is empty.",
        "データをロード" => "Load Data",
        "設定の読込" => "Load Settings",
        "設定の保存" => "Save Settings",
        "画像のセーブ" => "Save Image",
        "ファイルが既にあります。上書きしますか？" => {
            "File(s) already exist. Overwrite?"
        }
        "フレームレートには 1 以上を指定してください。" => {
            "Frame rate must be at least 1."
        }
        "設定ファイルを読み込めません" => "Could not load settings file",
        "設定を読み込めません" => "Could not load settings",
        "ヘルプを書き出せません" => "Could not write help file",
        "ブラウザを起動できません" => "Could not launch browser",
        "処理を中止しました" => "Operation cancelled",
        "エラー" => "Error",
        "警告" => "Warning",
        _ => return form_word(source),
    })
}

#[allow(unreachable_patterns)]
fn form_word(source: &str) -> Option<&'static str> {
    Some(match source {
        "  横倍率" => " H scale",
        "  縦倍率" => " V scale",
        "Xの最大値" => "X Maximum",
        "Xの最小値" => "X Minimum",
        "Yの最大値" => "Y Maximum",
        "Yの最小値" => "Y Minimum",
        "Zの最大値を指定" => "Set Z Maximum",
        "Zの最小値を指定" => "Set Z Minimum",
        "X軸とY軸の角度（-45～45度）" => "Angle between X and Y axes (-45–45°)",
        "X（θ）方向に間引く" => "Decimate in X (θ) direction",
        "Y軸" => "Y Axis",
        "Y軸/θ軸" => "Y / θ Axis",
        "Y（ｒ）方向に間引く" => "Decimate in Y (r) direction",
        "Zの範囲値（最小）" => "Z Range (Minimum)",
        "Z値の範囲を指定する（範囲外は白抜きになる）" => {
            "Set Z range (out-of-range values are blank)"
        }
        "Z値の範囲（最小）" => "Z Range (Minimum)",
        "avese - nodata" => "avese - no data",
        "gray-1" => "Gray 1",
        "gray-2" => "Gray 2",
        "gray-3" => "Gray 3",
        "height" => "Height",
        "smooth colors-1" => "Smooth Colors 1",
        "smooth colors-2" => "Smooth Colors 2",
        "smooth colors-3" => "Smooth Colors 3",
        "spectral colors-1" => "Spectral Colors 1",
        "spectral colors-2" => "Spectral Colors 2",
        "spectral colors-3" => "Spectral Colors 3",
        "width" => "Width",
        "×0.1" => "×0.1",
        "×0.2" => "×0.2",
        "×0.5" => "×0.5",
        "×10" => "×10",
        "×2" => "×2",
        "×5" => "×5",
        "※チェックを外した場合の動作はサポート外" => {
            "Behavior when unchecked is not supported"
        }
        "※ラベルは設定とともに保存されます" => "Labels are saved with settings",
        "※色の変更は「表示」-「前景色/背景色」" => {
            "Change colors under Display > Foreground/Background"
        }
        "■" => "■",
        "□" => "□",
        "○" => "○",
        "●" => "●",
        "イメージの取り扱い" => "Image Options",
        "オフセット" => "Offset",
        "クリップボードにコピー" | "クリップボードにコピー(&C)" => {
            "Copy to Clipboard(&C)"
        }
        "グリッドの太さ" => "Grid Width",
        "グリッドの表示形式" => "Grid Style",
        "ファイル中のデータ位置" => "Data Position in File",
        "フォントの変更" => "Change Font",
        "ヘルプ" | "ヘルプ(&H)" | "ヘルプ（&H）" => "Help(&H)",
        "ベクトルの基準値" => "Vector Reference Value",
        "ベクトル線の色を変更" => "Change Vector Color",
        "ベクトル線を大きさで色分け" => "Color Vectors by Magnitude",
        "ポイント" | "ポイント(&T)" => "Points(&T)",
        "メモリの長さ" => "Tick Length",
        "ラベル\tＭＳ Ｐゴシック\t14\t0" => "Labels\tMS PGothic\t14\t0",
        "ラベル表示する座標と文字列" => "Label Coordinates and Text",
        "リストからの削除" => "Remove from List",
        "リストへの追加" => "Add to List",
        "代表長さ（ピクセル）" => "Reference Length (pixels)",
        "俯瞰図の高さ方向のピクセル数（1～1000）" => {
            "Bird's-eye height in pixels (1–1000)"
        }
        "俯瞰図(&B)" => "Bird's-eye View(&B)",
        "凡例\tＭＳ ゴシック\t12\t0" => "Legend\tMS Gothic\t12\t0",
        "凡例の横幅（3～）" => "Legend Width (3–)",
        "凡例の高さ（3～）" => "Legend Height (3–)",
        "凡例を表示する" | "凡例表示" | "凡例表示(&L)" => "Show Legend(&L)",
        "削除(&D)" => "Remove(&D)",
        "前景色の変更" => "Change Foreground Color",
        "十字" => "Crosshair",
        "厳密なデータチェックを行う" => "Strict Data Validation",
        "反転する" => "Reverse",
        "基準値を指定する" => "Set Reference Value",
        "大き過ぎるイメージを作らないよう警告を出す" => {
            "Warn before creating an image that is too large"
        }
        "平面図で表示" => "Show 2D Plot",
        "散布図の点の大きさのピクセル数（1～1000）" => {
            "Scatter point size in pixels (1–1000)"
        }
        "最大値から最小値までの色の順番を反転する" => {
            "Reverse color order from maximum to minimum"
        }
        "標準" => "Default",
        "点の形状" => "Point Shape",
        "画像のセーブ（&S）..." => "Save Image(&S)...",
        "矢印の先端の開き角度" => "Arrowhead Angle",
        "矢印の色を変更" => "Change Arrow Color",
        "矢印をつける（付けない場合は線分のみ）" => {
            "Show arrowheads (otherwise draw lines only)"
        }
        "立体表示の形式" => "3D Display Mode",
        "等高線(&C)" => "Contours(&C)",
        "等高線の範囲の最大値を指定する" => "Set maximum contour value",
        "等高線の範囲の最小値を指定する" => "Set minimum contour value",
        "等高線の色を変更" => "Change Contour Color",
        "等高線の間隔を指定する" => "Set contour interval",
        "等高線を描くZ値の範囲を指定する" => "Set Z range for contours",
        "終了時に自動保存する" => "Save automatically on exit",
        "背景色の変更" => "Change Background Color",
        "自動" => "Automatic",
        "行のデータを読み飛ばす" => "rows from start",
        "表示（&V）" => "Display(&V)",
        "角度をデフォルトに戻す" => "Reset Angles to Defaults",
        "設定に名前を付けて保存（&A）..." => "Save Settings As(&A)...",
        "設定の変更" => "Edit Settings",
        "設定の読込（&L）..." => "Load Settings(&L)...",
        "設定（&O）" => "Settings(&O)",
        "転置して読みこむ" => "Transpose on Load",
        "軸の太さ" => "Axis Width",
        "軸の形式" => "Axis Style",
        "軸の矢印の大きさ" => "Axis Arrowhead Size",
        "軸タイトル\tＭＳ Ｐ明朝\t12\t0" => "Axis Titles\tMS PMincho\t12\t0",
        "軸目盛\tterminal\t6\t0" => "Axis Ticks\tterminal\t6\t0",
        "軸目盛の数字を付ける" => "Show Axis Tick Labels",
        "追加(&A)" => "Add(&A)",
        "長さ" => "Len.",
        "間引く" => "Decimate",
        "面で表示" => "Show Surface",
        "ｒ" => "r",
        "ｷｬﾝｾﾙ" => "Cancel",
        "ﾊﾞｰｼﾞｮﾝ情報" => "About avese",
        "データロード" => "Load Data",
        "色の選択" => "Choose Color",
        "フォントの選択" => "Choose Font",
        "フォント名" => "Font Name",
        "候補" => "Preset",
        "フォント候補" => "Font Preset",
        "太字" => "Bold",
        "斜体" => "Italic",
        "下線" => "Underline",
        "取消線" => "Strikeout",
        "警告" => "Warning",
        "中止" => "Cancel",
        "はい(Y)" => "Yes (Y)",
        "中止(A)" => "Abort (A)",
        "無視(I)" => "Ignore (I)",
        "Box（外接）" => "Bounding Box",
        "クリップボードにコピー" => "Copy to Clipboard",
        "パネル状のとき転置する" => "Transpose Panel Data",
        "凡例表示" => "Show Legend",
        "変更(&B)" | "変更(&C)" | "変更(&F)" | "変更(&H)" | "変更(&T)" => "Change",
        "バージョン情報" => "About avese",
        "ヘルプ" | "ヘルプ(&H)" => "Help",
        "描画を中止しました" => "Rendering cancelled",
        "データ読み込みを中止しました" => "Data loading cancelled",
        "メッシュ生成を中止しました" => "Mesh generation cancelled",
        "中止しました" => "Cancelled",
        "データが格子状に並んでいません" => "The data is not arranged on a regular grid.",
        "設定を保存できません" => "Could not save settings",
        "再読み込みするデータがありません。" => "There is no data to reload.",
        "イメージがありません。先にデータを読み込んでください。" => {
            "No image is available. Load data first."
        }
        "フレームレートには 1 以上を指定してください。" => {
            "Frame rate must be at least 1."
        }
        "データロード" => "Load Data",
        "設定の読込" => "Load Settings",
        "設定ファイル(*.ini)" => "Settings Files (*.ini)",
        "設定に名前を付けて保存" => "Save Settings As",
        "色の選択" => "Choose Color",
        "フォントの選択" => "Choose Font",
        "フォント名" => "Font Name",
        "候補" => "Preset",
        "フォント候補" => "Font Preset",
        "太字" => "Bold",
        "斜体" => "Italic",
        "下線" => "Underline",
        "取消線" => "Strikeout",
        "AVI-Files (*.avi)" => "AVI Files (*.avi)",
        "BMP を作成するフォルダ" => "Folder for BMP Files",
        "リストにファイルがありません。" => "The file list is empty.",
        "aveseデータファイル(*.txt;*.dat)" => "avese Data Files (*.txt;*.dat)",
        "テキストファイル(*.txt)" => "Text Files (*.txt)",
        "データファイル(*.dat)" => "Data Files (*.dat)",
        "すべてのファイル(*.*)" => "All Files (*.*)",
        "ビットマップファイル(*.bmp)" => "Bitmap Files (*.bmp)",
        "RLE 圧縮 ビットマップファイル(*.bmp)" => "RLE Bitmap Files (*.bmp)",
        "PNGファイル(*.png)" => "PNG Files (*.png)",
        "エンハンストメタファイル(*.emf)" => "Enhanced Metafile (*.emf)",
        "ウィンドウズメタファイル(*.wmf)" => "Windows Metafile (*.wmf)",
        "画像のセーブ（拡張子で形式を指定）" => {
            "Save Image (choose format by extension)"
        }
        "画像ファイル (*.bmp;*.png;*.emf;*.wmf)" => "Image Files (*.bmp;*.png;*.emf;*.wmf)",
        "BMP の圧縮" => "BMP Compression",
        "RLE 圧縮を使用しますか？" => "Use RLE compression?",
        "文字列を入力してください" => "Enter text.",
        "クリップボードを開けません（他のアプリが使用中です）" => {
            "Could not open the clipboard (another app may be using it)."
        }
        "クリップボード用のメモリを確保できません" => {
            "Could not allocate clipboard memory."
        }
        "クリップボード用のメモリをロックできません" => {
            "Could not lock clipboard memory."
        }
        "クリップボードを空にできません" => "Could not clear the clipboard.",
        "クリップボードにコピーできません" => "Could not copy to the clipboard.",
        "メタファイルを作成できません" => "Could not create the metafile.",
        "このシステムでは未対応です" => {
            "This feature is not supported on this system."
        }
        "処理が中断されました" => "The operation was interrupted.",
        "処理中にエラーが発生しました" => "An error occurred during processing.",
        "描画中にエラーが発生しました" => "An error occurred while rendering.",
        "ファイルがありません" => "No files were provided.",
        "データがありません" => "No data is available.",
        "格子が小さすぎます (2x2 以上が必要)" => {
            "The grid is too small (at least 2 × 2 is required)."
        }
        "三角形分割には 3 点以上が必要です" => {
            "At least three points are required for triangulation."
        }
        "座標が数値ではない点があります" => {
            "Some points have non-numeric coordinates."
        }
        "点が一直線上に並んでいるため、三角形を作れません" => {
            "The points are collinear, so they cannot form triangles."
        }
        "フレームの画像データが不正です" => "A frame contains invalid image data.",
        "フレームレートが範囲外です（0 より大きく 1000 以下）" => {
            "Frame rate is out of range (must be greater than 0 and at most 1000)."
        }
        "フレームレートが小さすぎます" => "Frame rate is too low.",
        "AVI にするフレームがありません" => "There are no frames to write to the AVI.",
        "AVI のサイズが 2GB を超えます。フレーム数か画像サイズを減らしてください" => {
            "The AVI would exceed 2 GB. Reduce the frame count or image size."
        }
        "WMF に書き出せない数の頂点を持つ多角形があります" => {
            "A polygon has too many vertices to export to WMF."
        }
        "画像が大きすぎて WMF に書き出せません（上限 32766 ピクセル）" => {
            "The image is too large to export to WMF (limit: 32,766 pixels)."
        }
        "設定ファイルを読み込めません" => "Could not load the settings file",
        "設定を読み込めません" => "Could not load settings",
        "ヘルプを書き出せません" => "Could not write the help file",
        "ブラウザを起動できません" => "Could not launch the browser",
        "はい(Y)" => "Yes (Y)",
        "中止(A)" => "Abort (A)",
        "無視(I)" => "Ignore (I)",
        "非構造格子データです\n三角メッシュを自動生成しますか？\n（時間がかかる場合があります）" => {
            "The data is not on a regular grid.\nGenerate a triangular mesh automatically?\n(This may take some time.)"
        }
        "リストにファイルがありません。" => "The file list is empty.",
        _ => return None,
    })
}
