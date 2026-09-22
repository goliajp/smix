//! Set a device's simulated location, or start it along a route, through
//! `devicectl` — by way of `DeviceControl`, as a flow would.
//!
//! Usage:
//!   devicectl_location <UDID> set <LAT> <LON>
//!   devicectl_location <UDID> route <SPEED-MPS> <LAT,LON> <LAT,LON>…
//!
//! Exit 0 means devicectl accepted it and its own account of what it set
//! agrees with what was sent. Nothing here clears the location afterwards:
//! whoever runs this owes the device a
//! `xcrun devicectl device simulate location clear --device <UDID>`.

use smix_sdk::device_control::DeviceControl;
use smix_sdk::devicectl_device::DevicectlClient;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let done = match words[..] {
        [udid, "set", lat, lon] => {
            DevicectlClient::new(udid)
                .location_set(udid, number(lat), number(lon))
                .await
        }
        [udid, "route", speed, ref points @ ..] => {
            let points: Vec<(f64, f64)> = points
                .iter()
                .map(|p| {
                    let (lat, lon) = p.split_once(',').unwrap_or_else(|| usage());
                    (number(lat), number(lon))
                })
                .collect();
            DevicectlClient::new(udid)
                .location_start(udid, &points, Some(number(speed)))
                .await
        }
        _ => usage(),
    };
    if let Err(e) = done {
        eprintln!("{e}");
        std::process::exit(1);
    }
    println!("agreed");
}

fn number(s: &str) -> f64 {
    s.parse().unwrap_or_else(|_| usage())
}

fn usage() -> ! {
    eprintln!(
        "usage: devicectl_location <UDID> set <LAT> <LON> | route <SPEED-MPS> <LAT,LON> <LAT,LON>…"
    );
    std::process::exit(2);
}
