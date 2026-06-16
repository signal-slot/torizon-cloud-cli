# Torizon Cloud API をラップする CLI を Rust で作って crates.io に公開する

Toradex の [Torizon Cloud](https://www.toradex.com/torizon)（旧 Torizon OTA）は、組み込み Linux デバイスの OTA アップデートやフリート管理を提供するプラットフォームです。GUI と REST API は提供されていますが、デバイス一覧の取得や OTA 配信を **コマンドラインから一括操作する公式 CLI はありません**。

そこで本記事では、Torizon Cloud の REST API（Torizon OTA v2 API）をラップした CLI を Rust で実装し、GitHub と crates.io に公開するまでの手順を解説します。完成物は以下です。

- crates.io: <https://crates.io/crates/torizon-cloud-cli>
- GitHub: <https://github.com/signal-slot/torizon-cloud-cli>

```bash
cargo install torizon-cloud-cli   # バイナリ名: torizon
```

> 本ツールは非公式であり、Toradex 社とは関係ありません。

## 対象読者

- Rust で実用的な CLI（`clap` + `reqwest`）を書きたい方
- Torizon Cloud の操作を CI やスクリプトから自動化したい方

## Torizon Cloud API の基礎

| 項目 | 値 |
| --- | --- |
| API ベース URL | `https://app.torizon.io/api/v2` |
| 認証方式 | OAuth2 `client_credentials` |
| トークンエンドポイント | `https://kc.torizon.io/auth/realms/ota-users/protocol/openid-connect/token` |
| OpenAPI 仕様 | `https://app.torizon.io/api/docs-2.0/torizon-2.0-openapi.yaml` |

主なリソースは `devices` / `packages` / `updates` / `fleets` / `device-data`（metrics）/ `lockboxes` / `remote-access` です。

### 認証情報の準備（重要）

REST API を叩くには、Torizon Cloud の Web UI で発行する **API client**（`clientId` / `clientSecret`）が必要です。デバイスのプロビジョニングに使う `credentials.zip` とは別物で、`credentials.zip` 内のトークンで REST API を呼ぶと `HTTP 403`（認可スコープ不足）になります。

トークン取得は次のリクエストです。

```bash
curl -X POST \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials&client_id=<ID>&client_secret=<SECRET>" \
  https://kc.torizon.io/auth/realms/ota-users/protocol/openid-connect/token
```

## プロジェクト構成

```
torizon-cloud-cli/
├── Cargo.toml
├── src/
│   ├── main.rs          # clap による CLI 定義と dispatch
│   ├── config.rs        # 認証情報（プロファイル）の保存/読込
│   ├── auth.rs          # トークン取得とキャッシュ
│   ├── client.rs        # HTTP ラッパー（Bearer / クエリ / 420 リトライ）
│   ├── output.rs        # テーブル / JSON 整形
│   └── commands/        # サブコマンド実装
│       ├── devices.rs   packages.rs   updates.rs
│       ├── fleets.rs    metrics.rs    lockboxes.rs
│       ├── remote_access.rs   login.rs   mod.rs
```

### Cargo.toml

```toml
[package]
name = "torizon-cloud-cli"
version = "0.1.0"
edition = "2021"
rust-version = "1.74"
description = "Unofficial command-line interface for the Torizon Cloud (Torizon OTA v2) API"
license = "MIT"

[[bin]]
name = "torizon"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive", "env"] }
clap_complete = "4"
reqwest = { version = "0.12", features = ["blocking", "json", "rustls-tls"], default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
anyhow = "1"
dirs = "5"
```

`reqwest` は同期版で十分なので `blocking` を使い、`rustls-tls` で OpenSSL 依存を避けています。

## 認証情報の管理（config.rs）

認証情報は `~/.config/torizon/credentials.toml` に保存します。複数プロファイルに対応させ、`--profile` で切り替えられるようにします。

```toml
default = "default"

[profiles.default]
client_id = "..."
client_secret = "..."
# 任意で上書き可能:
# api_base = "https://app.torizon.io/api/v2"
```

ファイルは Unix で `0600` パーミッションにして秘密を保護します。

```rust
pub const DEFAULT_API_BASE: &str = "https://app.torizon.io/api/v2";
pub const DEFAULT_TOKEN_URL: &str =
    "https://kc.torizon.io/auth/realms/ota-users/protocol/openid-connect/token";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub client_id: String,
    pub client_secret: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub api_base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token_url: Option<String>,
}
```

## トークン取得とキャッシュ（auth.rs）

`client_credentials` グラントでトークンを取得し、`~/.config/torizon/token-cache.json` に有効期限付きでキャッシュします。期限の 30 秒前から再取得することで、毎回の API 呼び出しでトークンを取り直すのを避けます。

```rust
pub fn access_token(client: &reqwest::blocking::Client, profile: &Profile) -> Result<String> {
    let mut cache = load_cache();
    if let Some(tok) = cache.get(&profile.client_id) {
        if tok.expires_at > now_secs() + 30 {
            return Ok(tok.access_token.clone());
        }
    }
    let (token, expires_in) = request_token(client, profile)?;
    cache.insert(profile.client_id.clone(), CachedToken {
        access_token: token.clone(),
        expires_at: now_secs() + expires_in,
    });
    let _ = store_cache(&cache);
    Ok(token)
}
```

## HTTP ラッパー（client.rs）

ベース URL の付与、Bearer 認証、クエリ組み立て、エラー整形を 1 か所に集約します。Torizon API はレート制限時に `HTTP 420` と `Retry-After` ヘッダを返すため、ここで自動リトライします。

```rust
fn execute<F: Fn() -> RequestBuilder>(&self, path: &str, make: F) -> Result<Value> {
    let mut attempt = 0;
    loop {
        let resp = make().send().with_context(|| format!("requesting {path}"))?;
        if resp.status().as_u16() == 420 && attempt < MAX_RATE_LIMIT_RETRIES {
            let wait = resp.headers().get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(2);
            attempt += 1;
            std::thread::sleep(std::time::Duration::from_secs(wait));
            continue;
        }
        return Self::parse(resp, path);
    }
}
```

複数値クエリ（例: `deviceId` を複数指定）は、同じキーを複数回追加して表現します。

```rust
pub type Query<'a> = &'a [(&'a str, String)];
```

## サブコマンドの実装（clap derive）

`clap` の derive で、ネストしたサブコマンドを宣言的に定義します。

```rust
#[derive(Debug, Subcommand)]
enum Command {
    Login(LoginArgs),
    Devices  { #[command(subcommand)] cmd: DevicesCmd },
    Packages { #[command(subcommand)] cmd: PackagesCmd },
    Updates  { #[command(subcommand)] cmd: UpdatesCmd },
    Fleets   { #[command(subcommand)] cmd: FleetsCmd },
    Metrics  { #[command(subcommand)] cmd: MetricsCmd },
    Lockboxes{ #[command(subcommand)] cmd: LockboxesCmd },
    RemoteAccess { #[command(subcommand)] cmd: RemoteAccessCmd },
    Completions { shell: clap_complete::Shell },
}
```

たとえば `devices list` は、任意のフィルタをクエリに積んで `GET /devices` を呼ぶだけです。

```rust
let mut q: Vec<(&str, String)> = Vec::new();
if let Some(v) = args.limit { q.push(("limit", v.to_string())); }
if let Some(v) = args.name_contains { q.push(("nameContains", v)); }
for tag in &args.tags { q.push(("tags", tag.clone())); }
let resp = client.get("/devices", &q)?;
```

### enum パラメータは ValueEnum で型安全に

API が特定の enum 値（例: パッケージの `sortBy` は `Filename` / `CreatedAt`）しか受け付けない場合は、`ValueEnum` で受けて API 値にマッピングします。不正値は clap がパース時に弾きます。

```rust
#[derive(Clone, Copy, ValueEnum)]
pub enum SortBy { Filename, CreatedAt }

impl SortBy {
    fn api(self) -> &'static str {
        match self { SortBy::Filename => "Filename", SortBy::CreatedAt => "CreatedAt" }
    }
}
```

### OTA 配信の完了をポーリングする `updates watch`

OTA は「配信して、デバイスが適用するのを見届ける」までが一連の操作です。`updates watch` は `GET /updates/devices/{uuid}` と `GET /devices/{uuid}` をポーリングし、`deviceResult` が返る（成功・失敗が確定する）か `UpToDate` になるまで待ちます。

```rust
loop {
    let dev = client.get(&format!("/devices/{}", uuid), &[])?;
    let latest = latest_update(client, &uuid)?;
    // ... 状態を1行表示 ...
    if dev["deviceStatus"] == "UpToDate" { return Ok(()); }
    if has_result {
        if success { return Ok(()); } else { bail!("update failed: {result}"); }
    }
    std::thread::sleep(Duration::from_secs(interval));
}
```

## 出力形式（output.rs）

人間向けには固定幅テーブル、機械向けには `--json` で生 JSON を出します。`--json` 指定時は `"Update launched."` のようなステータス文言を出さず、**常にパース可能な JSON だけ**を標準出力に流すのがポイントです。

```rust
pub fn report_data(format: Format, human_msg: &str, data: &Value) {
    match format {
        Format::Json  => print_json(data),
        Format::Human => { println!("{human_msg}"); print_json(data); }
    }
}
```

ページネーションのレスポンスは `{ "values": [...], "total": N }` 形式なので、共通ヘルパーで `values` を取り出します。

## ユニットテストと CI

純粋ロジック（プロファイル解決、出力整形、レスポンスの平坦化、`key=value` パース等）は同一ファイル内の `#[cfg(test)]` モジュールでテストします。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolve_uses_default_then_sole_profile() {
        let mut cfg = Config::default();
        cfg.profiles.insert("only".into(), profile("o"));
        assert_eq!(cfg.resolve(None).unwrap().client_id, "o");
    }
}
```

GitHub Actions では fmt / clippy / test / build を回します。`.github/workflows/ci.yml`:

```yaml
name: CI
on:
  push: { branches: [main] }
  pull_request:
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: "rustfmt, clippy" }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all
      - run: cargo build --release
```

ローカルでも同じ順序で確認できます。

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo build --release
```

## 公開する

### GitHub

`Cargo.toml` に公開メタデータ（`repository` / `readme` / `keywords` / `categories` など）と `LICENSE` を用意し、リポジトリを作成して push します。バイナリクレートなので `Cargo.lock` もコミットします。

```bash
git init -b main
git add -A
git commit -m "Initial release: Torizon Cloud CLI v0.1.0"
gh repo create signal-slot/torizon-cloud-cli --public --source . --push \
  --description "Unofficial CLI for the Torizon Cloud API"
```

### crates.io

公開前に `--dry-run` でパッケージ内容と隔離ビルドを検証します。

```bash
cargo publish --dry-run
cargo login        # crates.io の API トークンを入力
cargo publish
```

> `cargo publish` は**永続的で削除できません**（取り下げは `cargo yank` で「新規利用の停止」ができるのみ）。公開前にバージョンとパッケージ内容を必ず確認してください。

### リリースタグ

```bash
gh release create v0.1.0 --title "v0.1.0" --notes "First release."
```

`README.md` にバッジを付けておくと状態が一目で分かります。

```markdown
[![crates.io](https://img.shields.io/crates/v/torizon-cloud-cli.svg)](https://crates.io/crates/torizon-cloud-cli)
[![CI](https://github.com/signal-slot/torizon-cloud-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/signal-slot/torizon-cloud-cli/actions/workflows/ci.yml)
```

## 使い方

```bash
# ログイン（認証情報は ~/.config/torizon/credentials.toml に保存）
# シークレットは引数では受け取らない（履歴やプロセス一覧に残さない）
torizon login                       # 対話: secret は非エコー入力
# CI 等の非対話用:
#   TORIZON_CLIENT_ID=<ID> TORIZON_CLIENT_SECRET=<SECRET> torizon login
#   get-secret | torizon login --client-id <ID> --client-secret-stdin

# デバイス一覧
torizon devices list

# パッケージのアップロード
torizon packages upload --name myapp --version 1.0.0 \
  --hardware-id docker-compose --format BINARY --file ./docker-compose.yml

# OTA 配信 → 完了を見届ける
torizon updates launch --package <PKG_ID> --device <DEVICE_UUID>
torizon updates watch <DEVICE_UUID>

# 機械可読出力
torizon --json devices list | jq '.values[].deviceName'

# シェル補完
torizon completions zsh > "${fpath[1]}/_torizon"
```

## まとめ

- Torizon Cloud には公式 REST CLI がないため、OpenAPI 仕様をもとに `clap` + `reqwest` で自作しました。
- 認証は **API client** の `client_credentials`、トークンはキャッシュ、レート制限（HTTP 420）は自動リトライ。
- `--json` で機械可読出力に対応し、`updates watch` で OTA の完了まで CLI 内で完結させました。
- fmt / clippy / test / build を CI で回し、crates.io と GitHub に公開しました。

`cargo install torizon-cloud-cli` で試せます。Torizon Cloud の運用自動化に役立てば幸いです。
