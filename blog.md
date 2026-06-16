# Torizon Cloud をコマンドラインから操作する

Toradex の [Torizon Cloud](https://www.toradex.com/torizon)（旧 Torizon OTA）には Web UI と REST API がありますが、コマンドラインからまとめて操作する公式 CLI はありません。これは特に**コーディングエージェント**にとって障壁でした。エージェントは Web UI をクリックできない一方、CLI の実行と JSON 出力の解釈は得意だからです。

`torizon-cloud-cli` は Torizon Cloud の REST API をラップした非公式 CLI です。**これにより、コーディングエージェントがコマンドラインからアップデートの管理や配信、ステータスのチェックができるようになりました。** もちろん人間が対話的に、あるいは CI から使うこともできます。

- crates.io: <https://crates.io/crates/torizon-cloud-cli>
- GitHub（MIT）: <https://github.com/signal-slot/torizon-cloud-cli>

> 非公式ツールであり、Toradex 社とは関係ありません。

## インストール

```bash
cargo install torizon-cloud-cli   # バイナリ名: torizon
```

## API client を作る

REST API を呼ぶには API client の `clientId` と `clientSecret` が要ります。Torizon Cloud（<https://app.torizon.io>）で発行します。

1. **Settings → Repository** を開く
2. **Create API Client** をクリック
3. 名前と説明を入力し、**API V2 Client Type** を選ぶ
4. 表示された `clientId` と `clientSecret` をコピーする（`clientSecret` はこの画面を閉じると二度と表示されません）

デバイスのプロビジョニングに使う `credentials.zip` では REST API は `403` になります。API client を使ってください。

## ログイン

対話的にログインすると、認証情報が `~/.config/torizon/credentials.toml` に保存され、以降のコマンドで自動的に使われます。

```bash
torizon login
#   Client ID:     <clientId>
#   Client secret:                ← 入力しても画面には表示されません
```

CI やエージェントなどの非対話環境では、ログインせずに環境変数だけで各コマンドを実行できます。

```bash
export TORIZON_CLIENT_ID=<ID>
export TORIZON_CLIENT_SECRET=<SECRET>
torizon devices list        # login 不要でそのまま動く
```

複数環境はプロファイルで切り替えられます。

```bash
torizon login --profile staging
torizon --profile staging devices list
```

## 基本操作

```bash
# デバイス
torizon devices list
torizon devices get <DEVICE_UUID>

# パッケージ
torizon packages list --hardware-id <HW_ID>
torizon packages upload --name myapp --version 1.0.0 \
  --hardware-id docker-compose --format BINARY --file ./docker-compose.yml

# フリート
torizon fleets list
torizon fleets add-devices <FLEET_ID> --device <DEVICE_UUID>

# メトリクス
torizon metrics names
```

`--json` を付けると機械可読な JSON になり、エージェントやスクリプトがそのまま解釈できます。

```bash
torizon --json devices list | jq '.values[].deviceName'
```

シェル補完も生成できます。

```bash
torizon completions zsh > "${fpath[1]}/_torizon"
```

## OTA を配信して完了まで見届ける

OTA は「配信して、デバイスが適用するのを待つ」までが一連です。`updates watch` が完了（または失敗）まで状態を表示します。

```bash
# 配信するパッケージ ID を確認
torizon packages list --hardware-id verdin-imx8mp

# OS とアプリをデバイスに配信
torizon updates launch \
  --package astra-os-20260615-04 \
  --package astra-demo-20260615-04 \
  --device <DEVICE_UUID>

# 完了まで監視（UpToDate で成功、INSTALL_FAILED 等で失敗終了）
torizon updates watch <DEVICE_UUID>

# 履歴と結果コードを一覧
torizon updates list <DEVICE_UUID>
```

`updates list` は各更新の状態と結果（`OK` / `verdin-imx8mp:INSTALL_FAILED` など）を表で返すので、適用が成功したか失敗したかがそのまま分かります。

## まとめ

`torizon-cloud-cli` によって、コーディングエージェント（や CI、人間）が Web UI を介さずに OTA の配信から完了確認までをコマンドラインで回せます。`launch` → `watch` → `list` と `--json` 出力が、エージェント主導の OTA 運用の土台になります。

```bash
cargo install torizon-cloud-cli
```
