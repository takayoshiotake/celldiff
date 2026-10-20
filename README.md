# celldiff

CSVをセル単位で比較し、文字単位の変更を強調したExcelレポートや、差分セルのテキスト一覧を出力するRust製CLIです。

## 使用方法

```sh
celldiff left.csv right.csv
celldiff left.csv right.csv -o diff.xlsx
celldiff left.csv right.csv -o diff.xlsx --force
celldiff --help
celldiff --list-encodings
```

- 入力文字コードは左右別々に指定できます。省略した側はUTF-8です。自動検出は行いません。
- 入力はleft／rightと呼び、leftを基準としたrightへの変更を表示します。
- 同じレコード番号・列位置で比較し、先頭レコードも比較対象です。
- 値は文字列として扱い、先頭ゼロ・空白を保持します。
- 途中へのレコード挿入や並べ替えでは、以降も位置に従って比較されます。
- `-o`／`--output`は省略可能で、既定の出力先はカレントディレクトリの`diff.xlsx`です。
- 既存の出力は既定では上書きしません。`--force`でも入力ファイルとその別名は保護します。
- GUI、キー照合、マージ、TSV・Excel入力はありません。

## テキスト出力

```sh
celldiff left.csv right.csv --text
celldiff left.csv right.csv --text --oneline
celldiff left.csv right.csv --text --oneline --address rc
```

`--text`は差分があるセルだけを標準出力へ表示し、Excelファイルを生成しません。
既存の`diff.xlsx`にも触れません。通常はセルごとに左右の値を複数行で表示し、
`--oneline`を付けると差分セル1つを1行にします。

```text
B2: "東京本社" -> "大阪本社"
D8: <missing> -> ""

Changed cells: 2 (modified=1, added=1, removed=0)
```

`--address a1|rc`で座標表記を切り替えます。既定は`a1`（例: B2）、
`rc`では`row=2 column=2`と表示します。先頭レコード・列はA1、またはrow=1 column=1です。
rowは物理行ではなく、CSVの論理レコード番号です。
値は引用符で囲み、欠落は`<missing>`、空文字列は`""`で区別します。
改行・タブ・引用符・バックスラッシュ・制御文字などはRustの文字列リテラルに準じてエスケープします。
成功時は末尾に件数を表示し、差分がなければ`No differences.`と表示します。

`--oneline`と明示的な`--address`は`--text`専用です。
`--text`と明示的な`-o`／`--output`、`--force`、`--list-encodings`の併用は引数エラーです。
`--list-encodings`と`--oneline`／明示的な`--address`の併用も引数エラーです。
左右の文字コード指定は利用できます。`--format`・`--mode`・`--summary`は設けていません。

テキスト出力にはExcel固有の行数・列数・セル文字数上限を適用しません。
終了コードはExcel出力と同じ0／1／2です。標準出力への書き込みやflushの失敗もエラーとします。
一覧は逐次出力するため、途中で入力エラーが起きると部分的な一覧が残る場合があります。
その場合は最終集計を表示せず、標準エラーへ原因を出して終了コード2で終了します。

## 引用符の有無も比較する

既定ではCSVの`1`と`"1"`は同じ値として扱います。
`--compare-quotes`を指定すると、解析後の値に加え、フィールドが引用符で囲まれていたかも比較します。

```sh
celldiff left.csv right.csv --compare-quotes
celldiff left.csv right.csv --compare-quotes --text --oneline
```

```text
A1: "1" [unquoted] -> "1" [quoted]

Changed cells: 1 (modified=1, added=0, removed=0)
```

引用符だけの追加・削除はModifiedとして1セルに数えます。
空フィールドと`""`も区別します。欠落セルとの区別も維持します。
テキスト出力では、存在するセルに`[quoted]`／`[unquoted]`を注記します。

Excelでは、指定時は変更のないセルも含め、引用符付きフィールドをCSV表現で表示します。
外側の引用符と内部の二重引用符エスケープを再構成し、変更した引用符も赤字・取り消し線／青字・太字の対象にします。
これは元のバイト列そのものの表示ではなく、解析後の値と引用符情報から作る表示です。
元の行・列位置は維持し、Summaryに比較設定を記録します。
引用符・エスケープを含めた表示文字列にもExcelのセル文字数上限が適用されます。

CSVの空行・引用符内改行・二重引用符・可変列数の扱いは従来どおりです。
左右の文字コード指定とも併用できます。BOMや改行コード、文字コードだけの違いは差分にしません。
`--list-encodings`との併用は引数エラーです。

## 入力文字コード

```sh
celldiff left.csv right.csv --encoding-left cp932 --encoding-right utf-8 -o diff.xlsx
celldiff left.csv right.csv --el cp932 --er utf-8 -o diff.xlsx
celldiff --list-encodings
```

`--el`／`--er`は`--encoding-left`／`--encoding-right`の別名です。
共通設定は設けず、指定しなかった側はUTF-8として読みます。
`--list-encodings`は入力ファイル・出力先なしで、対応する38種類の文字コード名と別名を表示します。
名前は大文字・小文字を区別しません。一覧にない名前はエラーです。
主な対応対象はUTF-8、UTF-16LE／BE、Shift_JIS系、EUC-JP、ISO-2022-JP、GBK／GB18030、Big5、EUC-KR、Windows-1250〜1258です。

変換は`encoding_rs`のWHATWGマッピングに従います。
`cp932`／`windows-31j`はShift_JISへ対応付けますが、Windows CP932とすべてのバイト列で完全同一ではありません。
`ascii`／`iso-8859-1`／`latin1`はWindows-1252として扱います。厳密なASCII検証ではありません。
UTF-32、エンディアンを明示しない`utf-16`、`auto`は受け付けません。

先頭のBOMは指定した文字コードと一致する場合のみ除去し、矛盾するBOMはエラーにします。
UTF-32のBOMもエラーです。UTF-16LE／BEはBOMなしでも明示指定で読めます。
不正なバイト列・末尾で途切れた文字は置換せず、終了コード2で終了します。
エラー時は既存の出力を保持します。

内部では読みながらUTF-8へ変換し、CSV解析へ渡します。全入力をメモリへ保持しません。
エラー位置に表示されるバイト位置は、変換後のUTF-8上の位置です。
実際に使用した文字コード名はSummaryのLeft encoding／Right encodingに記録します。

## Excelレポート

`Differences`シートは元のCSVの行・列位置を保ちます。
CSVの1レコード目・1列目はA1です。変更のないセルも出力し、説明用の行・列は挿入しません。

変更セル内で共通部分を1回だけ表示し、削除部分は赤字・取り消し線、
追加部分は青字・太字で表示します。たとえば「東京本社→大阪本社」は
同じセルに「東京大阪本社」と入り、「東京」が削除、「大阪」が追加として強調されます。
これは差分を読むための表示であり、変更後データそのものではありません。

変更セルの背景は薄い黄色、追加セルは薄い青、削除セルは薄い赤です。
空文字列と存在しないセルも背景色で区別できます。
余剰レコード・列は元の位置に追加／削除として表示し、後続を詰めたり再対応したりしません。

絵文字や結合文字を分断しない書記素クラスタ単位で比較し、元の値は正規化しません。
数式に見える値も文字列として出力します。
`Summary`シートに入力名、件数、凡例、比較条件を記載します。
差分がない場合も、元の表とSummaryを含むxlsxを生成します。

Excelの上限（1,048,576行、16,384列、セル32,767 UTF-16単位）を超える場合はエラーで終了します。
1セルに旧・新の差分をまとめた結果も文字数上限の対象です。
無断の切り捨てやシート分割はしません。エラー時は完成ファイルを公開せず、既存出力を保持します。
CSVパーサーは空の物理行をスキップします。引用符内の改行はフィールド値です。

## 終了コード

| コード | 意味 |
| --- | --- |
| 0 | 正常終了、差分なし。ヘルプ・バージョン表示も0 |
| 1 | 正常終了、差分あり |
| 2 | 引数、入力、比較、保存のエラー |

差分ありの1は実行失敗ではありません。スクリプトでは0と1を正常終了として扱ってください。

## ダウンロードと実行要件

[Releases](https://github.com/takayoshiotake/celldiff/releases)から対象OSのアーカイブと
`checksums.txt`をダウンロードして展開してください。
`checksums.txt`はアーカイブのSHA-256一覧です。
アーカイブには実行ファイルと`celldiff-build-info.txt`だけを同梱します。
公開前の成果物は[GitHub Actions](https://github.com/takayoshiotake/celldiff/actions)の成功したCI実行でも取得できます。
CI成果物の保存期間は14日です。

### リリース作成（メンテナー向け）

1. mainのCI成功を確認し、実行URL末尾の実行IDを控えます。
2. `Cargo.toml`のバージョンに対応するタグ（例: `v0.1.0`）でDraft Releaseを作成します。対象はmainにします。
3. Actionsの「Release assets (draft only)」で「Run workflow」を開き、ブランチmain、タグ、CI実行IDを指定して実行します。
4. バイナリ3種と`checksums.txt`の添付を確認し、GitHub画面から公開します。

リリース用Actionsは手動実行のみです。成功したmainのCI成果物だけを使い、
Draftの対象をそのテスト済みコミットに固定します。既存タグがある場合は同じコミットを指す必要があります。
同名・同ハッシュの添付はスキップし、異なる内容の上書きは拒否します。
タグの作成やReleaseの公開は自動で行いません。
成果物が期限切れの場合はmainのCIを再実行してください。

| 配布対象 | 内容 |
| --- | --- |
| Windows x86_64 | ZIP、静的CRTでビルドしたexe |
| Linux x86_64 | tar.gz、musl向けの実行ファイル |
| macOS ARM64 | tar.gz、Apple Silicon向けの実行ファイル |

利用時にRust、Python、Excelのインストールは不要です（xlsxを閲覧するには対応アプリが必要）。
macOSではOS標準ライブラリを利用します。署名・公証は行っていません。
対応対象の実測サイズと動的依存は、各配布物の`celldiff-build-info.txt`に記録します。
アーカイブ自体はOSごとに異なります。全OSで動く単一の実行ファイルではありません。
一時ファイルを作成できるディスク領域が必要です。

## 開発

Rust stableを[公式の手順](https://rust-lang.org/tools/install/)で導入してください。

```sh
cargo build --release --locked
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- examples/left.csv examples/right.csv -o sample.xlsx
```

最後の例は差分があるので終了コード1になります。
入力は順次読み込み、出力はconstant-memoryモードで行います。
依存の正確なバージョンは`Cargo.lock`で管理します。

Linuxでの性能測定:

```sh
python3 scripts/benchmark.py --binary target/release/celldiff --rows 10000 100000
```

`/usr/bin/time`とPythonが必要です。これらは開発・検証用で、配布exeの実行要件ではありません。
CIの手動実行でも性能測定します。通常のpushでは大きなデータの性能測定を毎回実行しません。

詳しくは[設計書](docs/design.md)と[検証記録](docs/validation.md)を参照してください。
