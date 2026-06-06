//! Subscribe to `rt/lowstate` and dump BMS / power fields.
//!
//! Used to decode the Go2 MotionProcessor debug log (port4) `Ste:N; <value>`
//! field: is `<value>` battery SOC*100 or pack voltage?
//!
//! Usage: cargo run -p unitree-go2 --example go2_bms_dump -- eth0 [seconds]

use std::time::{Duration, Instant};

use unitree_go2::{topics, LowState, Participant, ReaderQos};

fn main() {
    let mut args = std::env::args().skip(1);
    let iface = args.next().unwrap_or_else(|| {
        eprintln!("usage: go2_bms_dump <iface> [seconds]");
        std::process::exit(2);
    });
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);

    if let Err(e) = run(&iface, secs) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(iface: &str, secs: u64) -> unitree_go2::Result<()> {
    eprintln!("opening participant on domain 0, iface {iface} ...");
    let dp = Participant::new(0, Some(iface))?;
    let topic = dp.create_topic::<LowState>(topics::LOW_STATE)?;
    let reader = dp.create_reader(&topic, ReaderQos::low_level_default())?;

    let start = Instant::now();
    let deadline = start + Duration::from_secs(secs);
    let mut count: u64 = 0;
    let mut last_warn = start;

    while Instant::now() < deadline {
        match reader.poll()? {
            Some(s) => {
                count += 1;
                // Print once per ~0.5s to keep it readable.
                if count == 1 || count % 250 == 0 {
                    let b = &s.bms_state;
                    let cells: Vec<u16> = b.cell_vol.iter().copied().take_while(|&v| v != 0).collect();
                    let vsum: u32 = cells.iter().map(|&v| v as u32).sum();
                    println!(
                        "soc={}%  soc*100={}  current={}mA  power_v={:.2}V  power_a={:.2}A  \
                         cycle={}  status={}  ver={}.{}",
                        b.soc, (b.soc as u32) * 100, b.current,
                        s.power_v, s.power_a,
                        b.cycle, b.status, b.version_high, b.version_low
                    );
                    println!(
                        "   bq_ntc={:?}  mcu_ntc={:?}  ntc1={} ntc2={}  cell_mV(sum={}mV={:.2}V)={:?}",
                        b.bq_ntc, b.mcu_ntc, s.temperature_ntc1, s.temperature_ntc2,
                        vsum, vsum as f64 / 1000.0, cells
                    );
                    let im = &s.imu_state;
                    println!(
                        "   IMU temp={}C  rpy(deg)=[{:.2}, {:.2}, {:.2}]",
                        im.temperature,
                        im.rpy[0].to_degrees(), im.rpy[1].to_degrees(), im.rpy[2].to_degrees()
                    );
                }
            }
            None => {
                if count == 0 && last_warn.elapsed() >= Duration::from_secs(1) {
                    eprintln!("... no LowState yet (check cabling / 192.168.123.x / iface {iface})");
                    last_warn = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    eprintln!("received {count} samples in {secs}s");
    if count == 0 {
        return Err(unitree_go2::DdsError::Timeout);
    }
    Ok(())
}
