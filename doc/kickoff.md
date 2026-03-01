# M0 キックオフ — 別セッションで実装を開始する人向け

別セッションでこの crate の実装を始める際の **コピペで動く手順** と **事前環境チェック結果**。

## 1. 環境チェック結果 (2026-05-30、開発機 = WSL2 / x86_64)

| 項目 | 結果 | 備考 |
|---|---|---|
| Rust toolchain | 1.94.1 (x86_64-unknown-linux-gnu) | OK |
| インストール済みターゲット | `x86_64-unknown-linux-gnu`, `wasm32-unknown-unknown` | aarch64 は `rustup target add aarch64-unknown-linux-gnu` で追加 |
| sdk2 同梱 `libddsc.so` x86_64 | 有効な ELF、必要 glibc 最大 2.7 | あらゆる Linux で動く見込み |
| sdk2 同梱 `libddsc.so` aarch64 | 有効な ARM aarch64 ELF | RPi/Jetson 用にそのまま使える |
| `cmake` | 3.x インストール済み | OK |
| `libclang` | 15/17/18/19/20 すべて入っている (libclang1-* パッケージ経由) | bindgen は OK |
| `clang` バイナリ | **未インストール** | `sudo apt install clang` 推奨 (bindgen の一部経路で要る) |
| `idlc` (Cyclone DDS IDL compiler) | **未インストール** | M2 で要る。後述の手順でビルド |
| `ros2` CLI | **未インストール** | XCDR2 wire の確認に便利 (任意) |
| **ネットワーク環境** | **WSL2 (`eth0`/`wsltap`)** | ⚠️ **重要**: DDS multicast が WSL2 NAT を越えられない可能性大。実機検証はネイティブ Linux 機 (RPi/Jetson) or Windows 側で行うこと |

## 2. 別セッション開始前にユーザが決めること

| 項目 | 内容 |
|---|---|
| GitHub オーナー名 | `git remote add origin git@github.com:<owner>/unitree-sdk-rs.git` 用 |
| public / private | private で開始するのが無難 |
| Go2 実機接続環境 | WSL2 では multicast が通らないので、別の Linux 機 (RPi 等) で実機検証する想定でよいか確認 |
| Go2 iface 名 | 実機接続環境で `ip link show` 実行、結果を控えておく |

## 3. 開始セッション最初のコマンド (コピペ用)

別セッション開始時、以下を順に実行する。

### 3-1. リポジトリ作成

```bash
mkdir -p ~/work/dp/unitree-sdk-rs
cd ~/work/dp/unitree-sdk-rs
git init
```

### 3-2. 計画書を新リポジトリへコピー

```bash
mkdir -p docs
cp ~/work/dp/articara/ref/docs/unitree-sdk-rs/{README,design,plan,kickoff}.md docs/
```

### 3-3. ライセンスファイル

```bash
curl -sL https://www.apache.org/licenses/LICENSE-2.0.txt > LICENSE-APACHE
# or 手動でコピー
```

### 3-4. Cargo workspace 雛形

```bash
mkdir -p crates/{cyclonedds-sys,unitree-msgs,unitree-dds,unitree-go2}/src
cat > Cargo.toml <<'EOF'
[workspace]
resolver = "2"
members = [
    "crates/cyclonedds-sys",
    "crates/unitree-msgs",
    "crates/unitree-dds",
    "crates/unitree-go2",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
repository = "https://github.com/<owner>/unitree-sdk-rs"
rust-version = "1.75"

[workspace.dependencies]
thiserror = "1"
tracing = "0.1"
bytes = "1"
EOF
```

各 crate に最小の `Cargo.toml` と `src/lib.rs` を置く (`cargo check --workspace` を通すため)。

### 3-5. `.msg` をスナップショットコピー

```bash
mkdir -p crates/unitree-msgs/msgs/{unitree_api,unitree_go,unitree_hg}
cp ~/work/dp/articara/ref/unitree_ros2/cyclonedds_ws/src/unitree/unitree_api/msg/*.msg crates/unitree-msgs/msgs/unitree_api/
cp ~/work/dp/articara/ref/unitree_ros2/cyclonedds_ws/src/unitree/unitree_go/msg/*.msg  crates/unitree-msgs/msgs/unitree_go/
cp ~/work/dp/articara/ref/unitree_ros2/cyclonedds_ws/src/unitree/unitree_hg/msg/*.msg  crates/unitree-msgs/msgs/unitree_hg/
ls crates/unitree-msgs/msgs/*/  # 43 ファイルあること
```

### 3-6. sdk2 同梱 libddsc を staging

開発の利便性のため、sdk2 同梱の `.so` を crate ローカルに symlink (or 環境変数で参照):

```bash
mkdir -p vendor/cyclonedds/lib/{x86_64,aarch64}
ln -sf ~/work/dp/articara/ref/unitree_sdk2/thirdparty/lib/x86_64/libddsc.so vendor/cyclonedds/lib/x86_64/
ln -sf ~/work/dp/articara/ref/unitree_sdk2/thirdparty/lib/aarch64/libddsc.so vendor/cyclonedds/lib/aarch64/
mkdir -p vendor/cyclonedds/include
cp -r ~/work/dp/articara/ref/unitree_sdk2/thirdparty/include/* vendor/cyclonedds/include/
```

`build.rs` は `vendor/cyclonedds/` を優先的に見るようにする (環境変数 `CYCLONEDDS_HOME` で上書き可能)。

### 3-7. M0 完了の検証

```bash
cargo check --workspace          # 空の crate 4 つが通る
file vendor/cyclonedds/lib/*/libddsc.so   # 両アーキの ELF 確認
```

## 4. 別環境 (RPi 4/5, AGX Orin, Jetson Nano) での確認手順

M0 で各実機にログインしてやること:

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 必要ツール
sudo apt update && sudo apt install -y clang cmake build-essential pkg-config

# sdk2 同梱の .so をテスト
scp $(host):path/to/libddsc.so .
ldd ./libddsc.so          # 解決できる?
objdump -T ./libddsc.so | grep -oE 'GLIBC_[0-9.]+' | sort -u | tail -3  # 最大要求 glibc
ldd --version | head -1   # OS 側の glibc

# Jetson Nano の場合
# glibc 2.27 を期待。sdk2 .so の要求が 2.7 までなので動くはず
```

各環境での結果は M0 PR (or issue) に貼り付ける。

## 5. 別セッションで AI アシスタントに最初に渡す引き継ぎプロンプト

```
unitree-sdk-rs の v0.1 実装 (M0: 足場作り) に着手したい。
- 計画書は ~/work/dp/articara/ref/docs/unitree-sdk-rs/ に揃っている (README, design, plan, kickoff)
- 作業ディレクトリは ~/work/dp/unitree-sdk-rs/ (新規 git repo を作る)
- kickoff.md §3 のコマンドを順に実行して M0 を完了させる
- M0 Exit 判定: cargo check --workspace 通過、libddsc.so の対応アーキ確認

まず kickoff.md と plan.md §1 M0 を読み、計画通り進める。
```

(articara dir で起動すれば auto-memory に [project_unitree_sdk_rs_sibling_crate.md](file:///home/kasai/.claude/projects/-home-kasai-work-dp-articara/memory/project_unitree_sdk_rs_sibling_crate.md) が乗るので、追加コンテキストは不要)

## 6. やらないこと (混乱防止)

- 別セッションでいきなり M1/M2 のコードを書き始めない (M0 完了が前提)
- 計画書の大幅変更を別セッションでしない (本セッションで合意済み)
- articara workspace 側の Cargo.toml を編集しない (取り込みは M6 完了後)
