# celldiff

CSVをセル単位で比較し、文字単位の変更を強調したExcelレポートを出力するRust製CLIです。

## 使用方法

```sh
celldiff before.csv after.csv -o diff.xlsx
celldiff before.csv after.csv -o diff.xlsx --force
celldiff --help
```

- UTF-8のCSVに対応します（先頭BOMは許容）。Shift_JISは事前にUTF-8へ変換してください。
- 同じレコード番号・列位置で比較し、先頭レコードも比較対象です。
- 値は文字列として扱い、先頭ゼロ・空白を保持します。
- 途中へのレコード挿入や並べ替えでは、以降も位置に従って比較されます。
- 既存の出力は既定では上書きしません。`--force`でも入力ファイルとその別名は保護します。
- GUI、キー照合、マージ、TSV・Excel入力はありません。

## Excelレポート

`Differences`シートは差分セルのみを一覧にします。
列はRecord、Column、Kind、Before present、After present、Before、Afterです。
番号は1始まり。KindはModified、Added、Removed。
Before/After presentが、空文字列と存在しないセルを区別します。

旧値の削除部分は赤字・取り消し線、新値の追加部分は青字・太字で表示します。
絵文字や結合文字を分断しない書記素クラスタ単位で比較し、元の値は正規化しません。
数式に見える値も文字列として出力します。

`Summary`シートに入力名、件数、凡例、比較条件を記載します。
差分がない場合も、見出しとSummaryを含むxlsxを生成します。

Excelの上限（差分1,048,575件、セル32,767 UTF-16単位）を超える場合はエラーで終了します。
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

[GitHub Actions](https://github.com/takayoshiotake/celldiff/actions)の成功したCI実行から、
対象の`celldiff-*`成果物をダウンロードして展開してください。
タグ`v*`、mainへのpush、手動実行で配布物を生成します。GitHub Releaseの自動公開は行いません。

| 配布対象 | 内容 |
| --- | --- |
| Windows x86_64 | ZIP、静的CRTでビルドしたexe |
| Linux x86_64 | tar.gz、musl向けの実行ファイル |
| macOS ARM64 | tar.gz、Apple Silicon向けの実行ファイル |

利用時にRust、Python、Excelのインストールは不要です（xlsxを閲覧するには対応アプリが必要）。
macOSではOS標準ライブラリを利用します。署名・公証は行っていません。
対応対象の実測サイズと動的依存は、各配布物の`build-info.json`に記録します。
アーカイブ自体はOSごとに異なります。全OSで動く単一の実行ファイルではありません。
一時ファイルを作成できるディスク領域が必要です。

## 開発

Rust stableを[公式の手順](https://rust-lang.org/tools/install/)で導入してください。

```sh
cargo build --release --locked
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- examples/before.csv examples/after.csv -o sample.xlsx
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
