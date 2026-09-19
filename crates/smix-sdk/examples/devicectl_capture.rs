//! Take a screenshot and a three-second recording through `devicectl`,
//! and say how many bytes each came to.
//!
//! This is the half a unit test cannot do: the argv and the parser are
//! checked without a device, and whether `devicectl` accepts that argv
//! and writes a playable file is a question only a device answers. Xcode
//! 27's `devicectl` treats a simulator as a device, so the same program
//! runs against either.
//!
//! Usage: devicectl_capture <UDID> <OUT-DIR>

use smix_sdk::device_control::DeviceControl;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(udid), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: devicectl_capture <UDID> <OUT-DIR>");
        std::process::exit(2);
    };
    let out = std::path::PathBuf::from(out);
    let client = smix_sdk::devicectl_device::DevicectlClient::new(udid.clone());

    let png = match client.screenshot(&udid).await {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("screenshot: {e}");
            std::process::exit(1);
        }
    };
    let shot = out.join("shot.png");
    std::fs::write(&shot, &png).expect("the out dir is writable");
    println!("screenshot: {} bytes -> {}", png.len(), shot.display());

    let movie = out.join("rec.mp4");
    if let Err(e) = client.start_recording(&udid, &movie).await {
        // Its own exit code: a device that does not offer recording is
        // refused here, by name and before anything is started, and the
        // caller has to be able to tell that from a recording that broke.
        println!("start_recording refused: {e}");
        std::process::exit(3);
    }
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    if let Err(e) = client.stop_recording().await {
        eprintln!("stop_recording: {e}");
        std::process::exit(1);
    }
    let len = std::fs::metadata(&movie).map(|m| m.len()).unwrap_or(0);
    println!("recording: {len} bytes -> {}", movie.display());
}
