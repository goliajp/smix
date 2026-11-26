//! `smix sim create --android`: make a new AVD from a system image that is
//! already installed, with the SDK's own avdmanager.
//!
//! Creating is all it does. The AVD is not registered and not started:
//! `smix sim register … --avd <name>` and `smix sim boot` do that, under
//! the same rules as any other emulator.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::CliError;

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct CreateArgs {
    /// Make an Android emulator (AVD), the only kind this verb makes.
    #[arg(long, required = true)]
    pub android: bool,
    /// The system image, as sdkmanager names it, e.g.
    /// `system-images;android-37.0;default;arm64-v8a`. It must be installed.
    #[arg(long)]
    pub image: String,
    /// The hardware profile, as `avdmanager list device -c` names it, e.g.
    /// `pixel_8`.
    #[arg(long = "device")]
    pub profile: String,
    /// The new AVD's name.
    #[arg(long)]
    pub name: String,
}

/// Where the image named by its sdkmanager package lives, relative to the
/// SDK: `system-images;android-37.0;default;arm64-v8a` →
/// `system-images/android-37.0/default/arm64-v8a`.
pub(crate) fn image_dir(package: &str) -> Result<PathBuf, String> {
    let parts: Vec<&str> = package.split(';').collect();
    let well_formed = parts.len() == 4
        && parts[0] == "system-images"
        && parts
            .iter()
            .all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains(['/', '\\']));
    if !well_formed {
        return Err(format!(
            "{package:?} is not a system image package; it has four parts \
             separated by ';', like system-images;android-37.0;default;arm64-v8a"
        ));
    }
    Ok(parts.iter().collect())
}

/// An AVD name avdmanager accepts: letters, digits, `.`, `_` and `-`.
pub(crate) fn check_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{name:?} is not an AVD name; use only letters, digits, '.', '_' and '-'"
        ))
    }
}

/// The SDK's avdmanager: `cmdline-tools/latest` first, then the highest
/// other version. One on PATH is not used, since it may belong to another
/// SDK and write an AVD pointing at images this one does not have.
pub(crate) fn avdmanager_in(sdk: &Path) -> Option<PathBuf> {
    let tools = sdk.join("cmdline-tools");
    let at = |dir: &Path| Some(dir.join("bin/avdmanager")).filter(|p| p.is_file());
    if let Some(p) = at(&tools.join("latest")) {
        return Some(p);
    }
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&tools)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    // by number: 9.0 is older than 19.0
    let number = |d: &PathBuf| -> Vec<u32> {
        d.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.split('.').map(|p| p.parse().unwrap_or(0)).collect())
            .unwrap_or_default()
    };
    versions.sort_by_key(number);
    versions.iter().rev().find_map(|d| at(d))
}

pub async fn run(args: CreateArgs) -> Result<(), CliError> {
    let fail = |s: String| CliError::Other(s);
    check_name(&args.name).map_err(fail)?;
    let image = image_dir(&args.image).map_err(fail)?;
    let sdk = smix_adb::sdk_root().ok_or_else(|| {
        fail("no Android SDK found: set ANDROID_HOME, or install it at its default location".into())
    })?;
    let home = crate::register_emulator::avd_home()
        .ok_or_else(|| fail("neither ANDROID_AVD_HOME nor HOME is set".into()))?;
    let ini = home.join(format!("{}.ini", args.name));
    if ini.exists() || home.join(format!("{}.avd", args.name)).exists() {
        return Err(fail(format!(
            "an AVD named {:?} already exists in {}; smix does not replace it. \
             Pick another --name.",
            args.name,
            home.display()
        )));
    }
    if !sdk.join(&image).is_dir() {
        return Err(fail(format!(
            "the image {} is not installed in {} (looked for {}), the SDK whose \
             emulator smix starts. Install it there with \
             sdkmanager --sdk_root=\"{}\" \"{}\" and run this again.",
            args.image,
            sdk.display(),
            image.display(),
            sdk.display(),
            args.image
        )));
    }
    let avdmanager = avdmanager_in(&sdk).ok_or_else(|| {
        fail(format!(
            "this SDK has no avdmanager (looked in {}/cmdline-tools/*/bin). Install \
             the command-line tools with sdkmanager \"cmdline-tools;latest\".",
            sdk.display()
        ))
    })?;
    let argv = [
        "create",
        "avd",
        "-n",
        &args.name,
        "-k",
        &args.image,
        "-d",
        &args.profile,
    ]
    .map(str::to_string);
    let opts = sdk_root_opts(&sdk, std::env::var("AVDMANAGER_OPTS").ok().as_deref());
    let (status, said) =
        tokio::task::spawn_blocking(move || avdmanager_create(&avdmanager, &argv, &opts))
            .await
            .map_err(|e| fail(e.to_string()))?
            .map_err(|e| fail(format!("could not run avdmanager: {e}")))?;
    // the AVD's own file is checked as well as the exit status, so an exit
    // of 0 with nothing written is not reported as made
    if !status.success() || !ini.is_file() {
        return Err(fail(format!(
            "avdmanager did not create {:?} ({status}):\n{}",
            args.name,
            said.trim_end()
        )));
    }
    println!(
        "created: AVD {} ({}, {}) in {}\nregister it, then boot it: \
         smix sim register <alias> --udid emulator-<port> --kind emulator --avd {}",
        args.name,
        args.image,
        args.profile,
        home.display(),
        args.name
    );
    Ok(())
}

/// The `AVDMANAGER_OPTS` that make avdmanager work in `sdk`.
///
/// avdmanager takes its SDK from where its own files are, after following
/// symlinks, and ignores `ANDROID_HOME`. A `cmdline-tools/latest` that links
/// to a Homebrew install therefore checks and finds images in Homebrew's
/// SDK, and the AVD it writes names an image the emulator in `sdk` does not
/// have. Its tools directory is a JVM property, and the SDK is two levels
/// above it; a path under `sdk` that is not a symlink keeps it in `sdk`.
/// What the caller had in `AVDMANAGER_OPTS` stays, ahead of this.
pub(crate) fn sdk_root_opts(sdk: &Path, had: Option<&str>) -> String {
    let ours = format!(
        "-Dcom.android.sdkmanager.toolsdir=\"{}\"",
        sdk.join("cmdline-tools").join("smix").display()
    );
    match had.filter(|h| !h.trim().is_empty()) {
        Some(h) => format!("{h} {ours}"),
        None => ours,
    }
}

/// Run avdmanager, answering "no" to its custom hardware profile question.
fn avdmanager_create(
    avdmanager: &Path,
    argv: &[String],
    opts: &str,
) -> std::io::Result<(std::process::ExitStatus, String)> {
    let mut child = Command::new(avdmanager)
        .args(argv)
        .env("AVDMANAGER_OPTS", opts)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"no\n")?;
    }
    let out = child.wait_with_output()?;
    let mut said = String::from_utf8_lossy(&out.stderr).into_owned();
    said.push_str(&String::from_utf8_lossy(&out.stdout));
    Ok((out.status, said))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_package_names_its_directory_under_the_sdk() {
        assert_eq!(
            image_dir("system-images;android-37.0;default;arm64-v8a"),
            Ok(PathBuf::from(
                "system-images/android-37.0/default/arm64-v8a"
            ))
        );
    }

    #[test]
    fn a_package_that_is_not_a_system_image_or_climbs_out_is_refused() {
        for bad in [
            "platforms;android-37",
            "system-images;android-37.0;default",
            "system-images;android-37.0;;arm64-v8a",
            "system-images;..;default;arm64-v8a",
            "system-images;android-37.0/../x;default;arm64-v8a",
            "system-images;a;b;c;d",
        ] {
            assert!(image_dir(bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn an_avd_name_is_letters_digits_dot_underscore_and_dash() {
        assert!(check_name("qip-insight-api37").is_ok());
        assert!(check_name("a.b_c-1").is_ok());
        for bad in ["", "has space", "slash/x", "ü"] {
            assert!(check_name(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn avdmanager_is_kept_in_the_sdk_and_the_callers_options_stay() {
        let sdk = Path::new("/Users/me/sdk");
        let ours = "-Dcom.android.sdkmanager.toolsdir=\"/Users/me/sdk/cmdline-tools/smix\"";
        assert_eq!(sdk_root_opts(sdk, None), ours);
        assert_eq!(sdk_root_opts(sdk, Some(" ")), ours);
        assert_eq!(sdk_root_opts(sdk, Some("-Xmx1g")), format!("-Xmx1g {ours}"));
    }

    #[test]
    fn avdmanager_is_taken_from_latest_then_the_highest_version() {
        let sdk = tempfile::tempdir().unwrap();
        let put = |v: &str| {
            let bin = sdk.path().join("cmdline-tools").join(v).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("avdmanager"), "").unwrap();
            bin.join("avdmanager")
        };
        assert_eq!(avdmanager_in(sdk.path()), None);
        put("9.0");
        let highest = put("19.0");
        assert_eq!(avdmanager_in(sdk.path()), Some(highest));
        let latest = put("latest");
        assert_eq!(avdmanager_in(sdk.path()), Some(latest));
    }
}
