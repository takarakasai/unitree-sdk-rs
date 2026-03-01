# unitree-sdk-rs

Rust SDK for Unitree robots (Go2 first), talking native DDS to the robot via a
vendored [Eclipse Cyclone DDS](https://github.com/eclipse-cyclonedds/cyclonedds)
(`libddsc`). No ROS 2 installation required.

> Status: **M0 — scaffolding.** The workspace builds but the crates are
> placeholders. See [doc/plan.md](doc/plan.md) for the milestone roadmap.

## Crates

| Crate | Role |
|---|---|
| [`cyclonedds-sys`](crates/cyclonedds-sys) | Low-level FFI bindings to `libddsc` (M1) |
| [`unitree-msgs`](crates/unitree-msgs) | Rust types + CDR for Unitree ROS2 messages (M2) |
| [`unitree-dds`](crates/unitree-dds) | Safe DDS pub/sub layer (M1/M3) |
| [`unitree-go2`](crates/unitree-go2) | High-level Go2 sport/state API (M4) |

## Building

```bash
cargo check --workspace
```

### Prerequisites

- Rust 1.75+ (toolchain pinned via `rust-version` in `Cargo.toml`)
- `clang`, `cmake`, `build-essential`, `pkg-config` (needed from M1 for bindgen
  and DDS linking)

### Vendored Cyclone DDS

The native `libddsc` shared libraries are **not committed** to this repo. The
headers under `vendor/cyclonedds/include/` are committed for bindgen, but the
per-architecture `.so` files must be staged locally:

```
vendor/cyclonedds/lib/x86_64/libddsc.so
vendor/cyclonedds/lib/aarch64/libddsc.so
```

These ship with the [unitree_sdk2](https://github.com/unitreerobotics/unitree_sdk2)
under `thirdparty/lib/<arch>/`. Copy them into `vendor/cyclonedds/lib/<arch>/`,
or point `build.rs` at an external Cyclone DDS install via the `CYCLONEDDS_HOME`
environment variable (M1+).

## Connecting to a Go2 (LowState dump)

The robot speaks DDS on **domain 0** over the wired LAN. The robot is
`192.168.123.161`; give the dev machine an address on that subnet, then read
`rt/lowstate`:

```bash
# 1. cable the Go2 to the wired NIC, bring it up and assign an IP
sudo ip link set eno1 up
sudo ip addr add 192.168.123.99/24 dev eno1     # any free .x

# 2. confirm reachability
ping -c3 192.168.123.161

# 3. dump 3 s of LowState as CSV (quaternion + 12 leg-joint positions)
cargo run -p unitree-go2 --example go2_lowstate_dump -- eno1 3
```

Replace `eno1` with your interface (`ip -br link`). If no samples arrive, the
example prints a hint — check cabling, the `192.168.123.x` address, and that you
named the right interface.

## Documentation

- [doc/README.md](doc/README.md) — overview
- [doc/design.md](doc/design.md) — architecture
- [doc/plan.md](doc/plan.md) — milestone plan
- [doc/kickoff.md](doc/kickoff.md) — environment setup

## License

Licensed under [Apache-2.0](LICENSE-APACHE).
