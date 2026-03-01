# unitree-sdk-rs 計画書 — Go2 low-level

設計は [design.md](design.md) 参照。ここでは「何を、どの順で、どうやって作るか」をまとめる。

## 0. 完了の定義 (Definition of Done, v0.1)

以下 **すべて** を満たした時点を v0.1 リリースとする:

1. `examples/go2_low_level.rs` が C++ [`go2_low_level.cpp`](../../unitree_sdk2/example/go2/go2_low_level.cpp) と等価動作 (1 関節 sin 波)
2. `examples/go2_stand.rs` が ROS 2 例 [`go2_stand_example.cpp`](../../unitree_ros2/example/src/src/go2/go2_stand_example.cpp) と等価動作 (起立シーケンス)
3. CRC32 が C++ 版とビット単位一致 (黄金値テスト)
4. `LowState` を 1 分間連続受信して欠落 0 (best-effort QoS なので「同期取れている」レベル)
5. README / 例 / 簡単な使い方ドキュメント
6. 下記 4 環境すべてで `cargo build --release` と `examples/go2_lowstate_dump` の起動が通る:
   - x86_64: Intel/AMD PC (Ubuntu 20.04 or 22.04)
   - aarch64: Raspberry Pi 4 (RPi OS 64-bit Bookworm)
   - aarch64: Raspberry Pi 5 (RPi OS 64-bit Bookworm)
   - aarch64: AGX Orin (JetPack 5/6)
   - aarch64 (best effort): Jetson Nano (JetPack 4.x、glibc 2.27) — sdk2 同梱 `.so` が動かなければ報告のみで blocker にしない

## 1. マイルストーン

### M0: 足場 (見積もり: 1-2 日)

| タスク | 完了条件 |
|---|---|
| 新規 git リポジトリ作成 (`~/work/dp/unitree-sdk-rs/`, 独立 origin) | `git init`、初期コミット (LICENSE-APACHE, README) |
| `libddsc.so` / `libddscxx.so` を sdk2 同梱から取得し LD パス確認 | x86_64 + aarch64 両 `.so` が `ldd` で解決 |
| **対応 4 環境すべてで sdk2 同梱 `.so` の動作確認** (`ldd`, ABI check) | x86_64 PC / RPi 4 / RPi 5 / AGX Orin で OK。Jetson Nano は best effort で記録のみ |
| 開発機での Go2 ネットワーク疎通 (`ping 192.168.123.x`, `ros2 topic echo /rt/lowstate`) | LowState が見える |
| Cargo workspace 雛形 (`crates/{cyclonedds-sys,unitree-msgs,unitree-dds,unitree-go2}/`) | `cargo check --workspace` 通過 |
| feature 設計の確定 (`backend-cyclonedds` default、`backend-rustdds` placeholder) | Cargo.toml に書き込み、`cargo check --features backend-rustdds` がスタブで通る |

**Exit 判定**: 空の crate 4 つが互いに依存ツリーを成し、`cargo check` が通る。Jetson Nano 以外の 3 つの aarch64 環境で sdk2 同梱 `.so` が動くことを確認。

### M1: メッセージ型生成 (見積もり: 2-3 日)

| タスク | 完了条件 |
|---|---|
| `.msg` パーサ実装 (build.rs) | `unitree_go/msg/*.msg` を構造体名 + フィールドリストの IR に変換できる |
| Rust struct コード生成器 | LowCmd, LowState, MotorCmd, MotorState, IMUState, BmsCmd, BmsState, WirelessController, TimeSpec, Error の Rust 型が出る |
| XCDR2 シリアライザ/デシリアライザ (`unitree-dds::cdr`) | ラウンドトリップでバイト列が一致 |
| C++ wire との bit-compat テスト | `unitree_sdk2` で書き出した `LowCmd` の serialized payload (pcap or 仕込みコードで取得) と Rust の出力が完全一致 |

**Exit 判定**: `cargo test -p unitree-msgs` 全パス、特に「C++ XCDR2 出力との一致」テストが通る。

**詰まりそうな点**:
- `string`/`uint8[]` の終端規約 (null terminator の有無)
- アラインメント (XCDR2 では max 4-byte align)
- `start_consecutive(true,true)` の意味 (primitive 配列の高速パス)

→ 詰まったら **pcap で実機の流れる payload を採取して bit を読む**。順序は ① pcap 採取 ② Rust 実装合わせ込み ③ ユニットテスト化、で進める。

### M2: cyclonedds-sys (見積もり: 1-2 日)

| タスク | 完了条件 |
|---|---|
| `wrapper.h` で必要 API を絞り bindgen | `dds_create_participant`, `dds_create_topic`, `dds_create_writer/reader`, `dds_write`, `dds_take_next`, QoS 系が Rust から呼べる |
| `build.rs` で `UNITREE_SDK2_ROOT` (or `CYCLONEDDS_HOME`) 解決 + `cargo:rustc-link-lib=ddsc` | `cargo build -p cyclonedds-sys` が x86_64 / aarch64 で通る |
| **同プロセス Pub→Sub の echo テスト** (`dds_string` 型) | `tests/echo.rs` が走る |
| Topic descriptor をどう作るかの方式確定 | `idlc` 経由で `_desc.c` を生成 → `cc` クレートでコンパイル、または generic sertype で実装、いずれか動く |

**Exit 判定**: Rust から `dds_string` (Cyclone DDS デフォルト型) を Pub/Sub できる。

**最大の壁**: Topic descriptor 周り。**サブタスクとして単独で 1 日確保**する。詰まったら以下の順で逃げる (上から順に試し、できるだけ上で済ます):
1. `idlc` で `LowCmd_desc.c` を生成 → cc crate でビルド → `extern "C"` で descriptor 構造体を参照 ← **Pure Rust 度の維持に望ましい**
2. それでも駄目なら一旦 cyclors/cyclonedds-rs (既存 crate) に切り替え
3. **最終手段**: `unitree_sdk2.a` から該当部分の `_desc` シンボルだけリンク (`feature = "use-sdk2-descriptors"` でオプトイン、デフォルト無効)

> 設計方針として「極力 Rust で完結」を優先するため、3. に降りる場合は **issue で記録 + Phase 2 で解消する宿題化**。

### M3: unitree-dds 抽象レイヤ (見積もり: 2-3 日)

| タスク | 完了条件 |
|---|---|
| `DdsBackend` trait 定義 + `CycloneBackend` 実装 (libddsc 経由) | design.md §5.3 のシグネチャ通り |
| `RustddsBackend` placeholder (常に `unimplemented!()` を返すスタブ) | `cargo check --features backend-rustdds` で型エラーなし |
| `Participant`, `Topic<T>`, `Writer<T>`, `Reader<T>` の公開 API | feature 切替で内部 backend を選べる、API は backend 非依存 |
| `WriterQos::low_level_default()` / `ReaderQos::low_level_default()` | best-effort + keep_last(1) が設定される |
| ネットワーク IF 指定 (`CYCLONEDDS_URI` XML 経由) | `Participant::new(0, Some("enp3s0"))` で動く |
| `Reader::poll()` / `Reader::recv_timeout()` 実装 | poll が non-blocking、recv_timeout がブロッキング |
| `unitree_go::LowState` を pub-sub するループバック試験 | 同プロセスでメッセージ授受 |

**Exit 判定**: 同プロセスループバック OK、かつ `cargo check --features backend-rustdds` が通る (Phase 2 への足場ができている)。

### M4: unitree-go2 facade + LowState 受信 (見積もり: 1 日)

| タスク | 完了条件 |
|---|---|
| `topics`, `joint`, `POS_STOP_F` などの定数 | export 完了 |
| `examples/go2_lowstate_dump.rs` 実装 | Go2 実機から 1 秒間の LowState を CSV ダンプ |
| **Go2 実機でランディング検証** | 受信した quaternion / motor_state[].q が直立姿勢と整合 |

**Exit 判定**: 実機 LowState が Rust から読める。

### M5: LowCmd 送信 + CRC (見積もり: 2 日)

| タスク | 完了条件 |
|---|---|
| `LowCmd::for_go2_low_level()` (head/level_flag/mode/PosStopF 等の初期化) | C++ `InitLowCmd()` と等価 |
| CRC32 実装 (`crate::crc::crc32` + `LowCmd::finalize_crc()`) | 黄金値テストでビット一致 |
| `LowCmd` を C 構造体相当バイト列にパックする処理 | `LowCmdCrc` ミラー or 手書きバッファで size = 812 (要実測) |
| `examples/go2_low_level.rs` 実装 | C++ 版と同じ 1 関節 sin 波動作 |
| **Go2 実機 + 補助スタンド** で安全動作確認 | 1 関節がゆっくり動く、暴走しない |

**Exit 判定**: Go2 のロボットが指令通り動く。

**安全策**: ロボットを **吊り下げ or 横倒し** にしてテスト。`Damp` モード or `level_flag != 0xFF` で簡単に止まる経路を確保。

### M6: Stand example (見積もり: 1 日)

| タスク | 完了条件 |
|---|---|
| `examples/go2_stand.rs` (12 関節 PID で立位遷移) | 4 段階の `target_pos_*` を補間して立つ/座る |
| `LowState.motor_state[].q` のフィードバック取得 | initial position 取得 OK |
| **実機で起立** | ロボットが立ち上がる |

**Exit 判定**: 安全に起立する。

### M7: 仕上げ (見積もり: 1 日)

| タスク | 完了条件 |
|---|---|
| ドキュメント (各 crate の README, 使い方 example) | 新規ユーザが 5 分で `go2_lowstate_dump` を走らせられる |
| `cargo doc` がクリーン | warning なし |
| ライセンスファイル | LICENSE-APACHE + LICENSE-MIT + NOTICE (Unitree / Cyclone DDS) |
| `cargo clippy` 通過 | `clippy::pedantic` までは要らない、default だけ |
| CI (GitHub Actions or 同等): `cargo check` / `cargo test -p unitree-msgs` | バインディングテストは skip 可、msg レイヤだけ自動化 |
| **articara 側からの利用例** | articara crate で `unitree-go2` を依存に追加し最小コードが書ける確認 |

## 2. 工数見積もり

| M | 期間 | 内容 |
|---|---|---|
| M0 | 1-2 日 | 足場 (リポジトリ作成、4 環境での `.so` 動作確認) |
| M1 | 2-3 日 | msg → Rust 型 + XCDR2 |
| M2 | 1-2 日 | cyclonedds-sys |
| M3 | 2-3 日 | dds 抽象 (`DdsBackend` trait + rustdds スタブ) |
| M4 | 1 日 | LowState 受信 |
| M5 | 2 日 | LowCmd 送信 (CRC 込み) |
| M6 | 1 日 | 起立 example |
| M7 | 1-2 日 | 仕上げ + マルチアーキ build 確認 |
| **計 (v0.1)** | **12-16 日** | (1 人換算、純作業時間) |

実機検証や DDS のハマリを考慮し **暦で 3-4 週間** をバッファとして見ておく。

Phase 2 (Pure Rust 化) は別計画として v0.1 完了後に立てる。粗い見積もりで **2-3 週間** 規模 (rustdds の XCDR2/Discovery 互換性次第)。

## 3. リスク管理

| # | リスク | 影響 | 確率 | 緩和 |
|---|---|---|---|---|
| R1 | XCDR2 の細部 (mutability, EMHEADER, padding) を読み違える | 全 msg が通信不能 | 中 | M1 を pcap-driven にする (実機 wire を見ながら実装) |
| R2 | Topic descriptor 動的生成が C API では難しい | M2 で詰まる | 高 | 事前評価。`idlc` 出力を使う方針を最初から採用 |
| R3 | CRC32 が C++ 構造体パディングに依存 | M5 で詰まる | 中 | 「手動 push」と「`#[repr(C)]` ミラー」両方実装し相互一致テスト |
| R4 | 500 Hz の周期維持 (Linux 標準スレッド) | 実機で stable に動かない | 低 | jitter 計測テスト、必要なら SCHED_FIFO 対応 |
| R5 | `libddsc.so` のバージョン差で ABI 不一致 | 別環境で動かない | 低 | `unitree_sdk2` 同梱の version を pin、`build.rs` で検出 |
| R5b | Jetson Nano (glibc 2.27) で sdk2 同梱 `.so` が動かない | Jetson Nano 対象から外す可能性 | 中 | M0 で確認、必要なら Cyclone DDS を自前ビルド or Jetson Nano は対象外宣言 |
| R6 | Cyclone DDS の Discovery が enp3s0 等のインターフェース指定で不安定 | 実機接続が断続的 | 中 | `CYCLONEDDS_URI` XML を厳密に書く、log を取る |
| R7 | Go2 実機破損 (関節暴走) | 物理損傷・けが | 中 | スタンド吊り下げ・PosStopF/VelStopF を必ず初期値に・最大トルク制限 |
| R8 | 開発時の rebase コストが大きい (4 crate 同時編集) | 進捗鈍化 | 低 | 各 M ごとに git tag、PR は M 単位 |

## 4. 環境要件

### 開発機

- Ubuntu 20.04 / 22.04 (sdk2 と同じ glibc 世代)
- Rust 1.75+ (workspace inheritance 等を使う)
- `clang` (bindgen の依存)
- `cmake` 3.10+ (cyclonedds ビルドが必要になった場合)
- `wireshark` / `tshark` (XCDR2 wire 観測用)
- aarch64 用クロスコンパイル: `aarch64-unknown-linux-gnu` ターゲット追加 (`rustup target add`) — もしくは各実機上でネイティブビルド

### 対応実行環境 (64-bit Linux のみ)

| 環境 | OS | glibc | 備考 |
|---|---|---|---|
| Intel/AMD PC | Ubuntu 20.04 / 22.04 (x86_64) | 2.31 / 2.35 | 第一ターゲット |
| Raspberry Pi 4 | RPi OS 64-bit Bookworm (aarch64) | 2.36 | |
| Raspberry Pi 5 | RPi OS 64-bit Bookworm (aarch64) | 2.36 | |
| AGX Orin | JetPack 5/6 (Ubuntu 20.04/22.04 aarch64) | 2.31/2.35 | |
| Jetson Nano | JetPack 4.x (Ubuntu 18.04 aarch64) | 2.27 | sdk2 同梱 `.so` が動かない可能性 (R5b 参照) |

### 実機接続

- Go2 のロボット側と有線 LAN (LAN ケーブル直結 or スイッチ経由)
- 開発機側で `192.168.123.x` の IP を持ち、`192.168.123.161` (Go2 既定) に到達できる
- 通信に使う NIC のインターフェイス名 (例: `enp3s0`, `eth0`) を `ip link show` で確認

### ソフト依存

| Crate | 用途 | 採否 |
|---|---|---|
| `bindgen` | libddsc バインド | 必須 |
| `cc` | `_desc.c` ビルド | 必須 |
| `thiserror` | エラー型 | 必須 |
| `tracing` | ログ | 推奨 |
| `bytes` / `byteorder` | CDR | 必須 (どちらか) |
| `crc` (crates.io) | 既存 CRC | **不採用** (専用多項式、自前実装) |
| `rustdds` | Phase 2 用 backend | M3 で feature 名のみ追加、Phase 2 で実装 |
| `tokio` | async | 当面不要 |
| `rclrs` | ROS 2 統合 | 範囲外 |

## 5. 進め方 (作業フロー)

1. **PR 単位は M ごと**。M を跨ぐ PR は作らない
2. 各 M の Exit 判定をチェックボックスにして PR 説明に貼る
3. 実機検証を伴う M (M4, M5, M6) は **動画 or ログ** を PR に添付
4. articara 本体への取り込み (`Cargo.toml` の `[workspace.dependencies]` への追加) は **M6 完了後**

## 6. 確定済み方針 (2026-05-30 時点)

| # | 項目 | 決定 |
|---|---|---|
| Q1 | crate 配置 | articora とは別の独立リポジトリ (`~/work/dp/unitree-sdk-rs/`) |
| Q2 | 対応 arch | 64-bit のみ: x86_64 / aarch64 (RPi 4, RPi 5, Jetson Nano, AGX Orin)。32-bit (armhf/i686) 対象外 |
| Q3 | iface 名 | 実行時引数。設計確定不要 (`ip link show` で確認) |
| Q4-1 | DDS 依存 | v0.1 は sdk2 同梱の `libddsc.so` をそのまま使う |
| Q4-2 | 抽象化 | `DdsBackend` trait を最初から導入。Phase 2 で `rustdds` バックエンドへ移行が長期ゴール |
| Q4-3 | C++ 逃げ道 | `feature = "use-sdk2-descriptors"` で残すがデフォルト無効、最後の手段。極力 Rust で完結 |
| Q5 | ライセンス | **Apache-2.0** 単体 |
| Q6 | リポジトリ形態 | 独立 git repo (lkmotor-rs と同じ運用)。articara の `gait-controller` / `misarta` が path/git 依存で取り込む |
| Q7 | sport_client | v0.1 範囲外。実行が必要なときは sdk2 のネイティブアプリで代替 |

## 7. 次のアクション

**M0 (足場作り)** に着手する:

1. `~/work/dp/unitree-sdk-rs/` で `git init`
2. Cargo workspace 雛形を作成
3. sdk2 同梱の `libddsc.so` を x86_64 / aarch64 (RPi 4, RPi 5, AGX Orin) で `ldd` 確認
4. Jetson Nano は best effort で確認、glibc 不一致なら Phase 2 で対応する旨を README に明記

Phase 2 (Pure Rust 化) は v0.1 完了時点で別計画書 (`plan-phase2.md` 想定) を立てる。
