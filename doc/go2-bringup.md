# Go2 実機ブリングアップ手順書 — 伏せ⇄立ちの動作確認

実機 Go2 で `examples/go2_stand.rs`（低レベル制御）を**安全に**動作確認するための
コピペ手順。設計は [design.md](design.md)、計画は [plan.md](plan.md) 参照。

> ⚠️ **安全第一**: 低レベル制御は onboard のモーションコントローラ（sport_mode）を
> バイパスする。実行前に周囲を片付け、**手元の専用コントローラをすぐ操作できる状態**に
> しておくこと。低レベルプログラム実行中はコントローラの高レベルボタンは効かない。
> 緊急時は Ctrl-C（または `kill`）＋ロボットを物理的に支える。

## 1. 接続と疎通確認

| 項目 | 値 |
|---|---|
| ロボット IP | `192.168.123.161` |
| PC IP / iface | `192.168.123.99/24` / `eth0` |
| DDS | Cyclone DDS, domain 0, トピック `rt/lowcmd` / `rt/lowstate` |

```bash
ping -c 2 192.168.123.161          # 疎通確認
ip -brief addr show eth0           # PC 側 IP 確認
```

## 2. ビルドと直接実行時の注意

```bash
cd ~/work/keel/unitree-sdk-rs
cargo build -p unitree-go2 --examples
```

`cargo run` はライブラリパスを自動設定するが、**ビルド済みバイナリを直接実行**
（プロセスを backgrounds して PID 制御したい場合など）するときは `libddsc.so.0` の
場所を通す必要がある:

```bash
export LD_LIBRARY_PATH=/home/takara/cyclonedds-install/lib:$LD_LIBRARY_PATH
BIN=target/debug/examples/go2_stand
```

## 3. sport_mode の ON/OFF（C++ ヘルパ）

低レベル制御には sport_mode を OFF にする必要がある。Rust SDK は RPC サービスを
トグルできない（v0.1 範囲外）ため、`unitree_sdk2` 同梱の C++ ヘルパを使う。

```bash
SW=~/work/keel/unitree_sdk2/build/bin/go2_motion_ctrl
$SW release eth0    # sport_mode OFF（モータ脱力）。伏せていれば安全、立位だと沈み込む
$SW restore eth0    # sport_mode ON（onboard コントローラが立位を取る）
```

> `restore` を実行すると onboard コントローラが基本姿勢＝**立位**を取る。伏せのまま
> にしたい場合は `restore` せず sport_mode OFF のまま放置する（地面で伏せていれば
> 無指令でも安全）。

## 4. 安全ラダー（動かさない → 少しずつ動かす）

`go2_stand` の使い方:

```text
go2_stand <iface> <up|down|hold> [secs] [kp] [kd] [hold_secs]
  up   : 伏せ → 立ち（STAND_POS へ）
  down : 立ち → 伏せ（LIE_POS へ、地面で安全終了）
  hold : 現在姿勢を保持（目標運動なし。kp を上げて段階的に確認）
  secs : ramp 時間（既定 1.5）
  hold_secs : ramp 後に目標姿勢を送り続ける秒数（既定 2.0）
```

実機検証は以下の順で行った（いずれも伏せ姿勢・周囲クリアを確認のうえ）。

### 段階 0: 関節角度の読み取り（無動作）
```bash
cargo run -p unitree-go2 --example go2_lowstate_dump -- eth0
```
→ `rt/lowstate` を受信し 12 関節角度が CSV で出れば DDS 疎通 OK。

### 段階 1: 送信パス疎通（ほぼ無動作）
```bash
$SW release eth0
cargo run -p unitree-go2 --example go2_stand -- eth0 hold 3 0 2   # kp=0 純ダンピング
cargo run -p unitree-go2 --example go2_stand -- eth0 hold 3 5 2   # kp=5 弱い能動保持
$SW restore eth0
```
→ `done: reached target and held ...` が出れば CRC 付き `rt/lowcmd` が受理されている。

### 段階 2: 立ち → 伏せ（`down`、安全側の遷移）
```bash
$SW release eth0
cargo run -p unitree-go2 --example go2_stand -- eth0 down 2.5
# 地面で折り畳んで終了。伏せのままにするなら restore しない
```

### 段階 3: 伏せ → 立ち（`up`、sport_mode への安全ハンドオフ）

`up` は**立位で終わる**ため、プログラムが終了するとモータが指令を失い脱力して沈み込む。
これを避けるため、**保持送信を継続したまま sport_mode を復帰**させ、onboard
コントローラに立位を引き継いでから低レベル送信を止める。

```bash
export LD_LIBRARY_PATH=/home/takara/cyclonedds-install/lib:$LD_LIBRARY_PATH
BIN=target/debug/examples/go2_stand

$SW release eth0
"$BIN" eth0 up 1.5 60 5 20 &    # ramp 1.5s で立ち、その後 20s 保持送信
UPPID=$!
sleep 7                          # 立ち上がり＋安定を待つ
$SW restore eth0                 # 保持送信中に sport_mode が立位を引き継ぐ
sleep 1
kill "$UPPID"                    # 低レベル送信を停止（以降は sport_mode が保持）
```

→ ロボットは立位のまま sport_mode 制御下に入る（転倒なし）。以降コントローラが通常通り使える。

## 5. 主要パラメータ（`examples/go2_stand.rs`）

| 定数 | 値 | 意味 |
|---|---|---|
| `STAND_POS` | hip 0, thigh 0.67, calf -1.3 ×4 | 立位（C++ `_targetPos_2` 相当）|
| `LIE_POS` | hip 0/±0.2, thigh 1.36, calf -2.65 | 折り畳み（C++ `_targetPos_1` 相当）|
| `KP_MOVE` / `KD_MOVE` | 60 / 5 | up/down の既定ゲイン |
| `KP_HOLD` / `KD_HOLD` | 0 / 2 | hold の既定（純ダンピング）|
| `CONTROL_DT` | 2 ms | 500 Hz 制御周期 |

## 6. トラブルシュート

| 症状 | 原因 / 対処 |
|---|---|
| `... waiting for LowState` のまま | iface / `192.168.123.x` / 配線を確認。`ping` で疎通確認 |
| `error while loading shared libraries: libddsc.so.0` | 直接実行時は `LD_LIBRARY_PATH` を設定（§2）|
| 低レベルコマンドが効かない（ロボットが反応しない）| sport_mode が ON のまま。`go2_motion_ctrl release eth0` を先に実行 |
| `up` 後にロボットが沈み込む | 保持送信を止めるのが早すぎ。`hold_secs` を伸ばし `restore` 後に `kill` する（§4 段階3）|
