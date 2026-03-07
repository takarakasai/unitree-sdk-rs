//! Smoke test: construct the rt/utlidar/switch publisher and write a
//! std_msgs/String sample (no robot needed — exercises participant/topic/
//! writer creation + dds_write of the C-struct mirror over loopback).
fn main() {
    let sw = unitree_go2::UtlidarSwitch::new("lo").expect("create UtlidarSwitch on lo");
    // Publish directly (don't wait for a subscriber — none on loopback).
    sw.publish("OFF").expect("publish OFF");
    sw.publish("ON").expect("publish ON");
    println!("OK: created rt/utlidar/switch writer and wrote std_msgs/String samples");
}
