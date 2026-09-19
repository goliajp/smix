//! Write a probe to a device's pasteboard through `devicectl`, read it
//! back, and say whether the two are the same bytes.
//!
//! Usage:
//!   devicectl_pasteboard <UDID> named   <PROBE>
//!   devicectl_pasteboard <UDID> general <PROBE> <KEEP-DIR>
//!
//! `named` works on the pasteboard `smix.e2e.probe`, which nobody else
//! reads or writes. `general` works on the one the user copies to, so it
//! reads what is there first and writes it back before it reports
//! anything — and what was there is never printed: this program's output
//! is byte counts and whether things were equal. While the original is
//! off the device it also sits in `<KEEP-DIR>/original`, readable by the
//! owner only, so that a run that dies halfway leaves something to put
//! back by hand. Once the original is read back equal, the file is gone.

use smix_sdk::device_control::DeviceControl;
use smix_sdk::devicectl_device::DevicectlClient;

const PROBE_PASTEBOARD: &str = "smix.e2e.probe";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ok = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [udid, "named", probe] => named(udid, probe).await,
        [udid, "general", probe, keep] => general(udid, probe, std::path::Path::new(keep)).await,
        _ => {
            eprintln!(
                "usage: devicectl_pasteboard <UDID> named <PROBE> | general <PROBE> <KEEP-DIR>"
            );
            std::process::exit(2);
        }
    };
    std::process::exit(i32::from(!ok));
}

async fn named(udid: &str, probe: &str) -> bool {
    let client = DevicectlClient::new(udid).on_pasteboard(PROBE_PASTEBOARD);
    let equal = round_trip(&client, udid, probe).await == Some(probe.to_string());
    println!("named probe_bytes={} probe_equal={equal}", probe.len());
    equal
}

async fn general(udid: &str, probe: &str, keep: &std::path::Path) -> bool {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let client = DevicectlClient::new(udid);
    let Some(original) = said(client.pasteboard_get(udid).await, "reading what is there") else {
        return false;
    };

    let kept = keep.join("original");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&kept)
        .and_then(|mut f| f.write_all(original.as_bytes()))
        .expect("the keep dir is writable and holds no earlier original");

    let probe_equal = round_trip(&client, udid, probe).await == Some(probe.to_string());

    // Put back whatever the probe did, and before judging it: a probe
    // that failed or compared unequal is a red verdict, and the user's
    // pasteboard should not be what pays for it.
    let put_back = said(
        client.pasteboard_set(udid, &original).await,
        "putting the original back",
    );
    let restored = said(
        client.pasteboard_get(udid).await,
        "reading the original back",
    );
    let restored_equal = put_back.is_some() && restored.as_deref() == Some(original.as_str());
    if restored_equal {
        std::fs::remove_file(&kept).expect("the file this run created is still there");
    } else {
        eprintln!(
            "the original did not come back equal; it is kept at {}",
            kept.display()
        );
    }
    println!(
        "general original_bytes={} probe_equal={probe_equal} restored_equal={restored_equal}",
        original.len()
    );
    probe_equal && restored_equal
}

/// What came back after writing `probe`, or `None` if either half failed.
async fn round_trip(client: &DevicectlClient, udid: &str, probe: &str) -> Option<String> {
    said(
        client.pasteboard_set(udid, probe).await,
        "writing the probe",
    )?;
    said(client.pasteboard_get(udid).await, "reading the probe back")
}

/// The value, or the error on stderr. devicectl's errors name the verb and
/// carry its stderr; none of them carries pasteboard content.
fn said<T, E: std::fmt::Display>(r: Result<T, E>, doing: &str) -> Option<T> {
    r.map_err(|e| eprintln!("{doing}: {e}")).ok()
}
