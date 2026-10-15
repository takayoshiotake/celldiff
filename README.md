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
