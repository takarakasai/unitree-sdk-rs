# unitree-sdk-rs 設計書 — Go2 low-level

## 1. 目的とスコープ

Unitree Go2 を **Rust から低レベル制御** (関節ごとの PD ゲイン+目標角+目標角速度+フィードフォワードトルク) するための基盤 crate を作る。`unitree_sdk2` (C++) の `example/go2/go2_low_level.cpp` 相当を Rust で書けることをゴールとする。

### 入っているもの

- `rt/lowcmd` (`unitree_go::msg::dds_::LowCmd_`) の送信
- `rt/lowstate` (`unitree_go::msg::dds_::LowState_`) の受信
- 送信前 CRC32 計算
- DDS Domain / NetworkInterface の設定
- 500 Hz スレッドループ用のタイマユーティリティ
- `rt/wirelesscontroller` (`WirelessController_`) の受信 (補助、リモコン入力)

### 入っていないもの

- 高レベル RPC (`sport_client`, `loco_client`) — 必要時は C++ 版 (unitree_sdk2) を併用する想定
- 他機種 (G1/H1/B2) — IDL は同じ枠組みで足せるが本計画では型生成のみ用意
- ROS 2 ノード API ラッパ
- 映像/音声/Lidar 等の大容量トピック
- Lease トークン管理 (lowcmd は lease 不要)

### 非機能要件

- 制御周期 **2 ms (500 Hz)** で `Write(LowCmd)` をジッタ ±200 µs 以内に維持
- `LowState` 受信遅延 (DDS → callback) も同等
- **対応 64-bit Linux**:
  - `x86_64`: Intel/AMD PC (Ubuntu 20.04+)
  - `aarch64`: Raspberry Pi 4 / 5 (RPi OS 64-bit), Jetson Nano (JetPack 4.x), AGX Orin (JetPack 5/6)
  - 32-bit (armhf/i686) は **対象外**
- `no_std` 不要 (Linux 前提)

### 最終形 (本計画 v0.1 完了後のロードマップ)

本 crate の長期ゴールは **Pure Rust 化** (libddsc.so 依存ゼロ)。本 v0.1 では libddsc FFI で実用性を取り、抽象 trait 越しに将来 `rustdds` に差し替えできる構造で出す。Phase 2 で rustdds バックエンド実装に移行する。

### 想定する利用側

独立 git リポジトリ (`unitree-sdk-rs`) として公開し、articara workspace 側の `gait-controller`, `misarta` などから git/crates.io 依存として取り込む。`unitree-sdk-rs` は articara 固有の概念を含まない汎用 crate に保つ。

## 2. アーキテクチャ全体像

```
┌──────────────────────────────────────────────────────────────────┐
│ user crate (examples/go2_stand.rs)                               │
└────────────┬─────────────────────────────────────────────────────┘
             │ unitree_go2::{Publisher, Subscriber, LowCmd, ...}
┌────────────▼─────────────────────────────────────────────────────┐
│ unitree-go2 (high-level facade)                                  │
│  - Pub<LowCmd>, Sub<LowState>                                    │
│  - LowCmd::compute_crc()                                         │
│  - LowCmdBuilder, joint index const (FR_0..RL_2)                 │
└────────────┬─────────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────────┐
│ unitree-dds  (Channel/Pub/Sub 抽象 + XCDR2 シリアライズ)         │
│  - DomainParticipant, Topic<T>, Reader<T>, Writer<T>             │
│  - trait DdsType { fn write_cdr(...); fn read_cdr(...); ... }    │
└────────────┬─────────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────────┐
│ unitree-msgs  (ROS 2 .msg から生成された型)                       │
│  - mod unitree_go { struct LowCmd, LowState, MotorCmd, ... }     │
│  - mod unitree_hg { ... }    ← 将来用                            │
│  - mod unitree_api { Request, Response, ... } ← 将来用            │
│  - impl DdsType for each (derive 経由)                           │
└────────────┬─────────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────────┐
│ cyclonedds-sys  (libddsc FFI バインディング)                     │
│  - bindgen で thirdparty/include/dds/dds.h から生成              │
└────────────┬─────────────────────────────────────────────────────┘
             │
┌────────────▼─────────────────────────────────────────────────────┐
│ libddsc.so  (Cyclone DDS C ライブラリ、unitree_sdk2 同梱)        │
└──────────────────────────────────────────────────────────────────┘
```

レイヤを 4 段に切る理由:
- **cyclonedds-sys** は unsafe FFI のみで責務を絞り、将来 `rustdds` への差し替えを `unitree-dds` 層だけで吸収可能にする
- **unitree-msgs** は build.rs で `.msg` から自動生成 → ロボット種別の追加コストを最小化
- **unitree-dds** は型に依存しない汎用層 (Go2 専用ロジックを入れない)
- **unitree-go2** は Go2 固有の便利関数 (CRC, ジョイント index, デフォルト値)

## 3. クレート構成

**独立 git リポジトリ** として作成、articara の sibling 物理配置で開発する (`lkmotor-rs`, `robstride-rs` と同じ運用パターン)。

```
~/work/dp/unitree-sdk-rs/        ← 独立 git repo (origin は別 remote)
├── Cargo.toml                   # workspace
├── crates/
│   ├── cyclonedds-sys/          # libddsc FFI (bindgen)
│   │   ├── build.rs
│   │   ├── wrapper.h
│   │   └── src/lib.rs
│   ├── unitree-msgs/            # .msg → Rust struct 生成
│   │   ├── build.rs             # .msg パーサ
│   │   ├── msgs/                # .msg のコピー (or symlink)
│   │   └── src/lib.rs
│   ├── unitree-dds/             # DDS 抽象レイヤ (バックエンド差し替え可能)
│   │   ├── src/{lib,participant,topic,reader,writer,qos,cdr}.rs
│   │   └── src/backend/         # 各バックエンド実装
│   │       ├── mod.rs           # trait DdsBackend 定義
│   │       ├── cyclonedds.rs    # libddsc FFI 経由 (v0.1 で実装)
│   │       └── rustdds.rs       # Pure Rust (Phase 2 で実装)
│   └── unitree-go2/             # Go2 facade
│       ├── src/{lib,low_cmd,crc,joints,topics}.rs
│       └── examples/
│           ├── go2_lowstate_dump.rs
│           ├── go2_low_level.rs       # C++ go2_low_level.cpp と等価
│           └── go2_stand.rs           # ros2 go2_stand_example.cpp と等価
├── docs/                        # 本書を最終的にここへ移動
└── README.md
```

articara workspace からの利用例:

```toml
# articara/Cargo.toml
[workspace.dependencies]
unitree-go2 = { git = "https://github.com/<owner>/unitree-sdk-rs", tag = "v0.1.0" }
# or path 依存 (開発中)
# unitree-go2 = { path = "../unitree-sdk-rs/crates/unitree-go2" }
```

`gait-controller` / `misarta` 等は `unitree-go2` を通して Go2 を駆動する。`unitree-sdk-rs` 側に articara 固有の型 (例: `GaitState`, `Misa*`) を **持ち込まない**。articara との結合は articara 側に薄い adapter を書いて吸収する。

## 4. メッセージ型生成 (unitree-msgs)

### 4.1 入力

[ref/unitree_ros2/cyclonedds_ws/src/unitree/](../../unitree_ros2/cyclonedds_ws/src/unitree/) 配下の **43 個の `.msg`**:

| パッケージ | 用途 | 個数 |
|---|---|---|
| `unitree_api/msg/` | RPC envelope | 8 |
| `unitree_go/msg/` | Go2 / 四足共通 | 24 |
| `unitree_hg/msg/` | Humanoid (G1/H1) | 11 |

これを `crates/unitree-msgs/msgs/` にコピー (or git submodule) し、`build.rs` で読む。

### 4.2 `.msg` 文法サブセット

ROS 2 msg 仕様の最小サブセットだけ対応する。Unitree msg では:

| 構文 | 例 | Rust マッピング |
|---|---|---|
| 基本型 | `int32 code` | `i32` |
| 固定長配列 | `uint8[40] wireless_remote` | `[u8; 40]` |
| 可変長配列 | `uint8[] binary` | `Vec<u8>` |
| 文字列 | `string parameter` | `String` |
| メッセージ参照 | `MotorCmd[20] motor_cmd` | `[MotorCmd; 20]` (`same-package`) |
| クロス参照 | (Unitree msg では未使用、必要時 `pkg/Type` 形式想定) |

定数宣言 (`int32 FOO=1`) は Unitree msg では未使用なので **非対応** で進める。コメント `#` も未使用。

### 4.3 生成物

各 msg → 1 struct + `DdsType` 実装 + `TopicTraits` 相当のメタ情報。

```rust
// crates/unitree-msgs/src/unitree_go/low_cmd.rs (自動生成)

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LowCmd {
    pub head: [u8; 2],
    pub level_flag: u8,
    pub frame_reserve: u8,
    pub sn: [u32; 2],
    pub version: [u32; 2],
    pub bandwidth: u16,
    pub motor_cmd: [MotorCmd; 20],
    pub bms_cmd: BmsCmd,
    pub wireless_remote: [u8; 40],
    pub led: [u8; 12],
    pub fan: [u8; 2],
    pub gpio: u8,
    pub reserve: u32,
    pub crc: u32,
}

impl crate::DdsType for LowCmd {
    const TYPE_NAME: &'static str = "unitree_go::msg::dds_::LowCmd_";
    const IS_KEYLESS: bool = true;
    fn write_cdr(&self, w: &mut impl CdrWriter) -> Result<()> { ... }
    fn read_cdr(r: &mut impl CdrReader) -> Result<Self> { ... }
}
```

### 4.4 シリアライズ方式

unitree_sdk2 は **Cyclone DDS XCDR2** を使う。.hpp 生成コードを見るかぎり、Unitree の msg は:

- **すべて `@final`** 相当 (mutable/appendable のメタなし、`DHEADER` なし)
- プリミティブ連続フィールドは `start_consecutive(true,true)` → **パッド省略**
- バイト順は **little-endian** (`RepresentationOptions = 0x0001`)
- 文字列は `uint32 length + bytes + '\0'`
- 可変長 sequence は `uint32 length + items`

→ 「XCDR2 final + little-endian + 4-byte alignment max」の固定モードで割り切る。

エンディアン/representation header (4 バイト) を含む完全な payload を生成する関数 `serialize_xcdr2(&T) -> Vec<u8>` を `unitree-dds::cdr` に置く。

### 4.5 生成方式の選択肢

| 方式 | 評価 |
|---|---|
| **(A) build.rs + 文字列出力 (推奨)** | デバッグしやすい、生成物を `target/` 下に置けばクレートに焼かない |
| (B) proc-macro (`unitree_msg!{...}`) | 取り回し悪い |
| (C) 事前生成して `src/` にコミット | レビューしやすいが手動同期が必要 |

→ (A) を選び、CI で「生成結果が変わったら fail」する diff check を入れる。

## 5. DDS バックエンド

### 5.1 採用理由

候補比較:

| | 互換性 | 工数 | 依存物 |
|---|---|---|---|
| **(α) libddsc FFI (v0.1 採用)** | ◎ (実機 sdk2 と同じ実装) | 小 | libddsc.so |
| (β) cyclors / cyclonedds-rs (既存 crate) | ◎ | 最小 | 同上 + 第三者 crate の品質依存 |
| **(γ) rustdds (Phase 2 で採用予定)** | 要検証 (XCDR2/QoS 細部) | 大 | なし |

**v0.1 は (α) libddsc FFI** で実用性を確保し、Phase 2 で (γ) Pure Rust に置換するのを長期ゴールとする。

そのため **抽象 trait (`DdsBackend`) を最初から導入** し、backend 切替は cargo feature で行えるようにしておく:

```toml
# Cargo.toml
[features]
default = ["backend-cyclonedds"]
backend-cyclonedds = ["cyclonedds-sys"]   # v0.1 で実装
backend-rustdds = ["rustdds"]              # Phase 2 で実装
```

### 5.1.1 v0.1 の libddsc 入手元

v0.1 では `unitree_sdk2` 同梱の `thirdparty/lib/{x86_64,aarch64}/libddsc.so` をそのまま使う。

- `build.rs` 環境変数 `UNITREE_SDK2_ROOT` から自動探索 (デフォルトは `ref/unitree_sdk2`)
- 配布時は `libddsc.so` を実機に同梱、`LD_LIBRARY_PATH` を案内
- Cyclone DDS をシステムにインストールしている場合 (`apt install libcyclonedds-dev` 等) はそちらを優先する選択肢を `build.rs` に持たせる

### 5.1.2 C++ ラッパへの後退 (最終手段)

Topic descriptor 動的生成 (§5.2 参照) がどうしても解決できない場合に限り、`libunitree_sdk2.a` から該当部分の `_desc` シンボルだけリンクする選択肢を残す (`feature = "use-sdk2-descriptors"`)。**Rust で完結することを優先**し、この feature は v0.1 ではデフォルト無効、できれば最後まで無効のまま出す。

### 5.2 cyclonedds-sys クレート

```toml
# crates/cyclonedds-sys/Cargo.toml
[package]
name = "cyclonedds-sys"
build = "build.rs"

[build-dependencies]
bindgen = "0.69"
```

`build.rs` は:

1. 環境変数 `CYCLONEDDS_HOME` (デフォルト: `/opt/unitree_robotics` か `ref/unitree_sdk2/thirdparty`) からヘッダ/ライブラリを探す
2. `bindgen` で `wrapper.h` (`#include <dds/dds.h>`) → Rust バインディング
3. `cargo:rustc-link-lib=ddsc` + `cargo:rustc-link-search=...`

主要 API:

| C 関数 | 用途 |
|---|---|
| `dds_create_participant` | DomainParticipant |
| `dds_create_topic` | Topic |
| `dds_create_writer` / `dds_create_reader` | Writer/Reader |
| `dds_write` | 送信 |
| `dds_take` / `dds_take_next` | 受信 |
| `dds_set_listener` / `dds_create_waitset` | 非同期受信 |
| `dds_qset_*` | QoS 設定 |

**Topic 登録の罠**: Cyclone DDS C API では `dds_create_topic_descriptor` 相当 (= `*_desc.c` 生成物) が必要。C++ ラッパは TopicTraits から自動生成しているが、C API では topic descriptor を別途用意する。

→ 戦略: **Generic topic + 自前シリアライザ**で回避する。`dds_create_topic_generic` (=`dds_create_topic_sertype` を使い `ddsi_sertype_default` を登録) で型ディスクリプタを動的に作る。**もしくは IDL コンパイラ `idlc` を使って `*.c` を生成**して bindgen に同梱する。

実装難易度から、最初は **`idlc` 経由で `*_desc.c` を生成 → C 側で扱う**方式を選ぶ。`idlc` は libddsc 同梱バイナリで、`.idl` を入力にする (= `.msg` → `.idl` 変換ステップが必要、これは 30 行程度で書ける)。

> **【2026-05-30 実装時調査による更新】**
>
> M2/M3 着手時に sdk2 同梱 `libddsc.so` (Cyclone DDS **0.10.2**) の export シンボルと
> 0.10.2 ソースを実測したところ、上記前提に **誤り** が見つかった:
>
> 1. **`idlc` は libddsc 同梱バイナリではない。** sdk2 配布物に `idlc` は無く、自前ビルドが必要。
>    幸い 0.10.2 ソースは生成済み `parser.c`/`scanner.c` を同梱しており、bison/flex 無しで
>    `cmake --target idlc` がビルドできた。
> 2. **`idlc` は実行時には不要。** ロボット/実行環境に要るのは `libddsc.so` だけで、`idlc` は
>    `.msg` 変更時に descriptor を再生成するメンテナ専用ツール。素の Ubuntu/RasPi OS の apt には
>    Cyclone DDS が無く (ROS リポジトリ経由のみ・バージョン不一致リスク)、各実行環境に idlc を
>    入れる前提は破綻する。**生成済み `*_desc.c` をリポジトリにコミットし、各環境は `cc` だけで
>    ビルドする** 方針に変更 (§4.5 の「生成物コミット + CI diff チェック」を descriptor にも適用)。
> 3. **「Generic topic で動的生成」は C API 単独では不可。** descriptor から sertype を作る
>    `ddsi_sertype_default_init` は `.so` に **export されていない**。`dds_create_topic`
>    (descriptor 版・public) で topic を作ると内部で sertype が生成されるが、それを **外部から
>    取り戻す公開 API (`dds_get_entity_sertype` 等) も存在しない**。sertype を入手するには
>    `dds_topic_pin` (export 済み) で handle→内部 `dds_topic*` を引き、内部ヘッダ
>    `dds__types.h` の `struct dds_topic { … struct ddsi_sertype *m_stype; }` を読むしかなく、
>    **内部ヘッダ + 構造体レイアウト依存** になる。
>
> この事実は §5.3 の backend trait シグネチャ選択に直結する (下記)。

### 5.3 抽象化 trait

> **【2026-05-30 実装時調査による更新 — データ経路の再検討】**
>
> 下記の元設計は backend 境界を **シリアライズ済み `&[u8]`** (`write_serialized` /
> `take_serialized`) としている。これには `dds_writecdr`/`dds_takecdr` + `ddsi_serdata_*`
> を使うが、serdata 構築には `ddsi_sertype*` が必須で、§5.2 の調査どおり sertype 入手は
> **内部ヘッダ + 構造体レイアウト依存** になる (= 経路 A)。
>
> 一方、公開 API のみで完結する経路 B (idlc 生成の **C 構造体** に Rust から値を詰めて
> `dds_write`、受信は `dds_take` で C 構造体を受けて Rust 型へ変換) は、M2 の echo テストで
> **動作実証済み**。内部構造体に一切触れない。
>
> | 観点 | 経路A: serialized (&[u8]) | 経路B: C構造体 (*T) |
> |---|---|---|
> | 必要 API | 内部ヘッダ + `m_stype` レイアウト依存 | 公開 API のみ |
> | 動作実証 | 未 (要 PoC) | ✅ echo で実証済み |
> | M1 自前 XCDR2 | backend で直接活用 | ワイヤ検証テスト用に降格 |
> | バージョン脆弱性 | 高 (内部構造体) | 低 |
> | Phase 2 (rustdds) 移行 | 境界が綺麗 | 型変換層を挟む |
>
> **採用方針: 経路 B (C 構造体)。** 内部ヘッダ依存を避け公開 API のみで v0.1 を成立させることを
> 優先する (R5/R5b のバージョン脆弱性も下げる)。backend trait は下記の `write_serialized`/
> `take_serialized` ではなく、型ごとの **C 構造体ミラー (`#[repr(C)]`) を介した typed I/O** に
> 変更する。具体的には:
>
> - `unitree-msgs` は各型に「Rust 型 ⇄ idlc 生成 C 構造体」変換 (`to_c`/`from_c`) を生成し、
>   `#[repr(C)]` ミラー構造体と topic descriptor (`extern` シンボル) を持つ。
> - `CycloneBackend` は `dds_create_topic(desc)` / `dds_write(*c_struct)` / `dds_take` を呼ぶ
>   typed バックエンド。trait は `write::<T>(handle, &T)` / `take::<T>(handle) -> Option<T>`
>   相当のジェネリックにする (関連メソッドを型パラメタ化、または `DdsCType` trait で抽象化)。
> - M1 の自前 XCDR2 (`cdr.rs`) は破棄せず、**C 構造体ミラーと wire 一致を突き合わせる検証テスト**
>   および将来の rustdds backend 用に残す。
> - String/sequence/可変長を含む型 (HeightMap 等) は C 構造体側でポインタ管理が要るので、
>   `dds_alloc`/`dds_sample_free` (echo.h 参照) を使うか、Go2 low-level で実際に使う固定長中心の
>   型 (LowCmd/LowState 等) を優先する。
>
> 以降の元設計テキストは「serialized 境界」を前提に書かれているが、上記方針では typed 境界へ
> 読み替えること。`Participant`/`Topic<T>`/`Writer<T>`/`Reader<T>` の **公開 API は変更なし**
> (`Writer::write(&T)` / `Reader::poll() -> Option<T>` のまま) で、変わるのは backend 内部のみ。

DDS バックエンドを差し替え可能にする中心が **`DdsBackend` trait**。`Participant<B>` のような型パラメタを表に出さず、cargo feature で 1 個に固定する設計にする (アプリ側を簡潔に保つ):

```rust
// crates/unitree-dds/src/backend/mod.rs
pub(crate) trait DdsBackend: 'static {
    type ParticipantHandle: Send + Sync;
    type TopicHandle:       Send + Sync;
    type WriterHandle:      Send + Sync;
    type ReaderHandle:      Send + Sync;

    fn create_participant(domain: u32, iface: Option<&str>) -> Result<Self::ParticipantHandle>;
    fn create_topic(p: &Self::ParticipantHandle, name: &str, type_name: &str, is_keyless: bool)
        -> Result<Self::TopicHandle>;
    fn create_writer(p: &Self::ParticipantHandle, topic: &Self::TopicHandle, qos: &WriterQos)
        -> Result<Self::WriterHandle>;
    fn create_reader(p: &Self::ParticipantHandle, topic: &Self::TopicHandle, qos: &ReaderQos)
        -> Result<Self::ReaderHandle>;
    fn write_serialized(w: &Self::WriterHandle, payload: &[u8]) -> Result<()>;
    fn take_serialized (r: &Self::ReaderHandle) -> Result<Option<Vec<u8>>>;
}

#[cfg(feature = "backend-cyclonedds")]
pub(crate) type ActiveBackend = backend::cyclonedds::CycloneBackend;
#[cfg(feature = "backend-rustdds")]
pub(crate) type ActiveBackend = backend::rustdds::RustddsBackend;
```

ユーザ向け公開 API は backend に依存しない:

```rust
// crates/unitree-dds/src/lib.rs
pub trait DdsType: Default + Clone {
    const TYPE_NAME: &'static str;
    const IS_KEYLESS: bool;
    fn write_cdr(&self, w: &mut impl CdrWriter) -> Result<()>;
    fn read_cdr(r: &mut impl CdrReader) -> Result<Self>;
}

pub struct Participant { inner: <ActiveBackend as DdsBackend>::ParticipantHandle }
pub struct Topic<T: DdsType> { inner: <ActiveBackend as DdsBackend>::TopicHandle, _t: PhantomData<T> }
pub struct Writer<T: DdsType> { inner: <ActiveBackend as DdsBackend>::WriterHandle, _t: PhantomData<T> }
pub struct Reader<T: DdsType> { inner: <ActiveBackend as DdsBackend>::ReaderHandle, _t: PhantomData<T> }

impl Participant {
    pub fn new(domain: u32, network_iface: Option<&str>) -> Result<Self>;
    pub fn create_topic<T: DdsType>(&self, name: &str) -> Result<Topic<T>>;
}

impl<T: DdsType> Writer<T> {
    pub fn new(p: &Participant, topic: &Topic<T>, qos: WriterQos) -> Result<Self>;
    pub fn write(&self, msg: &T) -> Result<()>;   // 内部で T::write_cdr → backend write_serialized
}

impl<T: DdsType> Reader<T> {
    pub fn new(p: &Participant, topic: &Topic<T>, qos: ReaderQos) -> Result<Self>;
    pub fn poll(&self) -> Result<Option<T>>;     // ノンブロッキング
    pub fn recv_timeout(&self, t: Duration) -> Result<T>;
    pub fn into_stream(self) -> impl Stream<Item=T>;  // async (将来)
}
```

ポイント:
- **CDR シリアライズは backend の外側で行う** (`T::write_cdr` → `Vec<u8>` を作って backend に渡す)。これで XCDR2 ロジックを 1 箇所に集約でき、rustdds 移行時も使い回せる
- backend は `&[u8]` を Pub/Sub する I/O 層に過ぎないので、libddsc 用 sertype と rustdds の Generic データを同じ trait で表現可能

### 5.4 QoS のデフォルト

unitree_sdk2 のデフォルト QoS は `default_xml` に倣う:

| Policy | Writer/Reader | 値 |
|---|---|---|
| Reliability | Both | BEST_EFFORT (lowcmd/lowstate は loss tolerant) |
| History | Both | KEEP_LAST (depth=1) |
| Durability | Both | VOLATILE |
| Deadline | Both | infinite |
| Liveliness | Both | AUTOMATIC |

これを `WriterQos::low_level_default()` / `ReaderQos::low_level_default()` として固定提供。

### 5.5 ネットワーク設定

unitree_sdk2 の `ChannelFactory::Init(domainId, iface)` は内部で Cyclone DDS の `CYCLONEDDS_URI` 環境変数を書き換えている (XML)。Rust 側でも同じく:

```rust
pub fn init(domain: u32, iface: Option<&str>) -> Result<Participant> {
    if let Some(iface) = iface {
        let xml = format!(
            "<CycloneDDS><Domain><General><Interfaces>\
             <NetworkInterface name=\"{}\" priority=\"default\" multicast=\"default\" />\
             </Interfaces></General></Domain></CycloneDDS>", iface);
        std::env::set_var("CYCLONEDDS_URI", xml);
    }
    Participant::new(domain, iface)
}
```

> 環境変数を触るのはプロセス全体に影響するため、`Participant::new` 一度だけ呼ぶ規約をドキュメント化する。

## 6. Go2 facade (unitree-go2)

### 6.1 公開 API

```rust
pub use unitree_msgs::unitree_go::{LowCmd, LowState, MotorCmd, MotorState, BmsCmd, BmsState, IMUState, WirelessController};

pub mod topics {
    pub const LOW_CMD: &str = "rt/lowcmd";
    pub const LOW_STATE: &str = "rt/lowstate";
    pub const SPORT_MODE_STATE: &str = "rt/sportmodestate";
    pub const WIRELESS_CONTROLLER: &str = "rt/wirelesscontroller";
}

pub mod joint {
    // motor_cmd[0..12] の意味づけ
    pub const FR_0: usize = 0;  pub const FR_1: usize = 1;  pub const FR_2: usize = 2;
    pub const FL_0: usize = 3;  pub const FL_1: usize = 4;  pub const FL_2: usize = 5;
    pub const RR_0: usize = 6;  pub const RR_1: usize = 7;  pub const RR_2: usize = 8;
    pub const RL_0: usize = 9;  pub const RL_1: usize = 10; pub const RL_2: usize = 11;
}

pub const POS_STOP_F: f32 = 2.146e9;
pub const VEL_STOP_F: f32 = 16000.0;

impl LowCmd {
    /// Go2 既定の安全初期化: head=0xFE,0xEF / level_flag=0xFF / mode=0x01 / q=PosStopF / dq=VelStopF
    pub fn for_go2_low_level() -> Self { ... }

    /// 各モータに同じパラメータを設定するヘルパ
    pub fn set_all_motor(&mut self, q: f32, kp: f32, kd: f32) { ... }

    /// motor_crc.cpp と等価。送信直前に必ず呼ぶ
    pub fn finalize_crc(&mut self) { ... }
}
```

### 6.2 CRC32 実装

[ref/unitree_ros2/example/src/src/common/motor_crc.cpp](../../unitree_ros2/example/src/src/common/motor_crc.cpp) を参照。

```rust
// crates/unitree-go2/src/crc.rs
const POLY: u32 = 0x04c11db7;

// 入力: msg を「C 側 LowCmd 構造体相当のバイト列」にパックした 812 byte
pub(crate) fn crc32(data: &[u32]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &word in data {
        let mut xbit: u32 = 1 << 31;
        for _ in 0..32 {
            crc = if crc & 0x80000000 != 0 { (crc << 1) ^ POLY } else { crc << 1 };
            if word & xbit != 0 { crc ^= POLY; }
            xbit >>= 1;
        }
    }
    crc
}
```

**重要な注意**: CRC は **C 構造体の packed layout** (XCDR2 の bytestream ではない) に対して計算する。元コードでは `struct LowCmd raw{}` を `memcpy` で埋め、`(uint32_t*)&raw` でキャストしている。

→ Rust 側では `#[repr(C)]` の **CRC 専用ミラー構造体** `LowCmdCrc` を別途定義し、明示的にパッキングを再現する:

```rust
#[repr(C)]
struct LowCmdCrc {
    head: [u8; 2],
    level_flag: u8,
    frame_reserve: u8,
    sn: [u32; 2],
    version: [u32; 2],
    bandwidth: u16,
    _pad_after_bandwidth: [u8; 2],   // 明示的に書く
    motor_cmd: [MotorCmdCrc; 20],
    bms: BmsCmdCrc,
    wireless_remote: [u8; 40],
    led: [u8; 12],
    fan: [u8; 2],
    gpio: u8,
    _pad_before_reserve: [u8; 1],    // gpio (offset 802) -> reserve (offset 804)
    reserve: u32,
    crc: u32,
}
```

`std::mem::size_of::<LowCmdCrc>()` が **812 byte (or C 側と同値)** になることを **コンパイル時 static assert** で保証する。

> 別案: `bytes::BufMut` で 1 フィールドずつ手書き push してもよく、こちらのほうが repr 依存しない。`finalize_crc` を実装ベースで 2 通り用意し、テストで一致確認する。

### 6.3 制御ループユーティリティ

タイマループは `std::thread::sleep_until` ベースの単純なものを提供 (Go2 の 500 Hz には十分):

```rust
pub fn run_recurring<F: FnMut() + Send + 'static>(period: Duration, mut f: F) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut next = Instant::now();
        loop {
            next += period;
            f();
            let now = Instant::now();
            if now < next { std::thread::sleep(next - now); }
            else { /* missed deadline, log */ }
        }
    })
}
```

リアルタイム性が必要なら後で `SCHED_FIFO` + `mlockall` 対応 (`libc` crate 経由) を入れる余地を残す。

## 7. ワイヤフォーマット詳細

### 7.1 DDS RTPS over UDP

- Cyclone DDS が unicast/multicast の Discovery (SPDP/SEDP) を自動でやる
- Go2 と同じネットワーク (有線 LAN、Go2 のロボット側は `192.168.123.x`) に居れば自動でつながる
- DomainId は `0` (sdk2 既定)

### 7.2 XCDR2 payload 構造 (LowCmd 例)

```
RTPS Data submessage
  └─ serialized_payload
       ├─ representation_identifier (2B) = 0x00 0x07  // XCDR2 LE plain
       ├─ representation_options    (2B) = 0x00 0x00
       └─ data (XCDR2 encoding)
            head[0], head[1], level_flag, frame_reserve,         // 4 byte
            sn[0..2] (u32 LE × 2),                                // 8 byte
            version[0..2],                                        // 8 byte
            bandwidth (u16),                                      // 2 byte
            ─ pad to align(4) ──                                 // 2 byte
            motor_cmd[0..20]
              for each: mode (u8), ─pad 3─, q, dq, tau, kp, kd, reserve[3]
            bms_cmd: off (u8), reserve[3] (u8×3)
            wireless_remote[0..40],
            led[0..12],
            fan[0..2],
            gpio (u8),
            ─ pad to align(4) ──
            reserve (u32),
            crc (u32)
```

### 7.3 LowState payload

`unitree_go/msg/LowState.msg` (オフセット込みで 600 byte 程度):

```
head[2], level_flag, frame_reserve,
sn[2], version[2], bandwidth,
imu_state: quaternion[4]f32, gyroscope[3]f32, accelerometer[3]f32, rpy[3]f32, temperature(i8)
motor_state[20]: mode, q, dq, ddq, tau_est, q_raw, dq_raw, ddq_raw, temperature(i8), lost(u32), reserve[2]
bms_state: ...
foot_force[4] (i16), foot_force_est[4] (i16),
tick (u32),
wireless_remote[40],
bit_flag (u8),
adc_reel (f32),
temperature_ntc1 (i8), temperature_ntc2 (i8),
power_v (f32), power_a (f32),
fan_frequency[4] (u16),
reserve (u32),
crc (u32)
```

詳細は [LowState.msg](../../unitree_ros2/cyclonedds_ws/src/unitree/unitree_go/msg/LowState.msg) で確認しつつ自動生成。

## 8. エラーハンドリング

`thiserror` で型化:

```rust
#[derive(Debug, thiserror::Error)]
pub enum DdsError {
    #[error("Cyclone DDS error: {code} ({source})")]
    Native { code: i32, source: &'static str },
    #[error("CDR serialization failed: {0}")]
    Cdr(String),
    #[error("Topic creation failed: {topic}")]
    TopicCreate { topic: String },
    #[error("Receive timeout")]
    Timeout,
}
```

DDS の C API は戻り値 `dds_return_t` (負値がエラー)。`check(rc, "dds_create_writer")?` 風ヘルパでラップ。

## 9. ロギング

`tracing` を採用 (or `log`)。crate 内では `tracing::{debug,info,warn,error}!` のみ使い、外部の subscriber は呼び出し側で設定。articara workspace の規約に揃える。

## 10. テスト戦略

### 10.1 ユニットテスト

- **CRC32 黄金値**: C++ `get_crc()` の出力を実機 or `unitree_sdk2` ビルドから採取し、Rust 実装と一致確認
- **XCDR2 ラウンドトリップ**: 各 msg 型を ser → de して一致確認
- **XCDR2 vs C++**: `unitree_sdk2` で書き出したバイト列 (pcap or ハードコード) と Rust の出力を bitcompare

### 10.2 統合テスト (libddsc 必須)

- `unitree-dds` の Pub→Sub ループバック (同一プロセスで Writer/Reader を作って疎通)
- ROS 2 ノードとの相互運用: `ros2 topic echo /rt/lowcmd` で受信できること

### 10.3 ハード in the loop

- Go2 実機で `examples/go2_lowstate_dump.rs` → 1 秒間の `LowState` を CSV ダンプ
- Go2 実機で `examples/go2_low_level.rs` → 1 関節だけ sin 波振動 (C++ 版と同じ)
- Go2 実機で `examples/go2_stand.rs` → 立ち上がり/座り遷移

### 10.4 CI

- libddsc を CI に入れるのが重い → `unitree-msgs` 単体ユニットテストと bindgen ビルドだけ CI で回し、HIL は手動

## 11. ライセンスと配布

- `unitree_sdk2`: BSD-3-Clause 相当 (要確認 → [LICENSE](../../unitree_sdk2/LICENSE))
- `unitree_ros2` 配下 `.msg`: Apache-2.0 (要確認 → [LICENSE](../../unitree_ros2/LICENSE))
- `cyclonedds`: Eclipse Public License 2.0
- **本 crate は Apache-2.0 単体ライセンス**
- README に「`libddsc.so` は別途 EPL-2.0」「`.msg` 原本は Unitree Robotics 由来」を明記

## 12. 既知の懸念

| 懸念 | 影響 | 対策 |
|---|---|---|
| Topic descriptor を C API で動的生成 | DDS バックエンド着手の最初の壁 | `idlc` で `_desc.c` 生成 → cc crate でコンパイル同梱 |
| XCDR2 のメンバ ID/EMHEADER 解釈 | sdk2 と相互運用できなくなる | 実 pcap で wire を観測して合わせる |
| 環境変数 `CYCLONEDDS_URI` のグローバル汚染 | 同プロセス内で domain 切替不可 | ドキュメント化、`Participant::new_with_xml(xml)` 提供 |
| C++ struct のパディング再現 (CRC) | CRC が一致せず Go2 が指令を弾く | 単体テスト + 黄金値 |
| Realtime 性 (500 Hz ジッタ) | プロトでは許容、本番要件 | `SCHED_FIFO` 対応を将来オプションで |
| Cargo workspace 配置の最終決定 | 後で動かすコストが増える | M0 で確定 |

## 13. 将来拡張

### Phase 2 (v0.2): Pure Rust 化

**最優先の継続開発項目**。`backend-rustdds` feature を実装し、libddsc.so 依存を排除する。

- `rustdds` crate を評価 → XCDR2 / QoS / Discovery 互換性を確認
- 不足分は fork して PR、または `unitree-dds::backend::rustdds` で補完
- v0.1 と同じ統合テスト (実機 Go2 起立) が rustdds backend でも通ることをもって完了
- `cargo build --no-default-features --features backend-rustdds` で `.so` 依存ゼロを確認

### Phase 3 以降

- 高レベル RPC (`Request_/Response_` + JSON parameter) → `unitree-rpc` クレート
- `loco_client` (G1), `sport_client` (Go2) → `unitree-g1`, `unitree-go2-sport` crate
  - ただし当面は C++ unitree_sdk2 ネイティブアプリ併用で代替可
- ROS 2 統合 (`rclrs` 連携)
- 非同期 API (`tokio` + `dds_waitset` または rustdds の Stream)
- リアルタイム性向上 (`SCHED_FIFO`, `mlockall`, lock-free ring buffer)
