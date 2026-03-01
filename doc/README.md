# unitree-sdk-rs — Go2 low-level SDK Rust 化プロジェクト

unitree_sdk2 (C++) を参照しつつ、**Unitree Go2 の低レベル制御 (rt/lowcmd ↔ rt/lowstate) を Rust から扱える独立 crate** を作る。

## ドキュメント

- [design.md](design.md) — 設計書: アーキテクチャ・型・ワイヤ仕様・API 設計
- [plan.md](plan.md) — 計画書: マイルストーン・リスク・検証戦略
- [kickoff.md](kickoff.md) — 別セッションで実装に着手する人向けの手順とコピペコマンド

## TL;DR

- **対象**: Go2 の low-level (関節 PD + トルク制御、500 Hz)
- **通信路**: Cyclone DDS / トピック `rt/lowcmd`, `rt/lowstate` (XCDR2)
- **IDL 一次ソース**: [ref/unitree_ros2/cyclonedds_ws/src/unitree/](../../unitree_ros2/cyclonedds_ws/src/unitree/) の `.msg`
- **v0.1 バックエンド**: `libddsc` (Cyclone DDS C API) を FFI でラップ。`unitree_sdk2` 同梱のプリビルド `.so` をそのまま使う
- **長期ゴール (Phase 2)**: `rustdds` バックエンドへ移行して libddsc 依存を排除。そのため `DdsBackend` trait による抽象を v0.1 から導入する
- **対応 arch (64-bit Linux のみ)**: x86_64 (Intel/AMD PC), aarch64 (Raspberry Pi 4 / 5, Jetson Nano, AGX Orin)
- **リポジトリ形態**: articora とは別の **独立 git リポジトリ** (`~/work/dp/unitree-sdk-rs/`)。articara workspace の `gait-controller` / `misarta` が path/git 依存で取り込む
- **ライセンス**: Apache-2.0
- **最初の Deliverable**: `examples/go2_stand.rs` が C++ 版 [go2_stand_example.cpp](../../unitree_ros2/example/src/src/go2/go2_stand_example.cpp) と同じ起立動作を Go2 で再現する

## 範囲外 (v0.1 では扱わない)

- High-level RPC (`sport_client`, `loco_client`) — 必要時は sdk2 ネイティブアプリを併用
- G1 / H1 / B2 など他機種
- ROS 2 ノード化 / rclrs 統合
- WebRTC / 画像ストリーム / Audio

## ロードマップ

| Phase | 目標 | 状態 |
|---|---|---|
| v0.1 | libddsc FFI + Go2 low-level + 起立 example | 計画完了、M0 着手前 |
| Phase 2 | `rustdds` バックエンドで libddsc.so 依存を排除 | v0.1 完了後に別計画書を作成 |
| Phase 3+ | RPC (`sport_client`/`loco_client`)、他機種、ROS 2 統合、非同期 API、リアルタイム性向上 | 未着手 |
