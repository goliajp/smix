//! A simulator's PosterBoard store, checked before the simulator boots.
//!
//! PosterBoard keeps the lock screen and ambient clock posters under
//! `data/Library/Application Support/PRBPosterExtensionDataStore`. A commit
//! of its own that fails (the staged version gone from the shared
//! `data/tmp/PFTemporaryDirectory`, `NSCocoaErrorDomain Code=4`) is rolled
//! back without restoring everything: a later boot finds configurations
//! whose descriptor is gone, rewrites them without a descriptor identifier,
//! and from then on PosterBoard dies in `-[PRPosterDescriptor _initWithPath:]`
//! at every start, restarted and dying again, each time with a crash report
//! on this Mac. Erasing the simulator only buys time (it came back two weeks
//! after one). The store is PosterBoard's to rebuild: removed while the
//! simulator is off, it is written afresh at the next boot, and all that is
//! lost is the wallpaper and clock face going back to their defaults.

use std::path::{Path, PathBuf};

const STORE: &str = "Library/Application Support/PRBPosterExtensionDataStore";
/// Written alongside the store; removed with it so nothing there refers to
/// the posters that were thrown away.
const POSTER_KIT: &str = "Library/Application Support/com.apple.PosterKit";
const DESCRIPTOR_ID: &str = "com.apple.posterkit.provider.descriptor.identifier";

/// A configuration PosterBoard cannot load, named for the note that says
/// why the store was rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenPoster {
    /// The extension's bundle id, e.g. `com.apple.ClockPoster.ClockPosterExtension`.
    pub extension: String,
    /// The configuration's directory name (a UUID).
    pub configuration: String,
    /// What is wrong with it.
    pub why: Why,
}

/// Why PosterBoard cannot load a configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// No descriptor identifier: the state PosterBoard asserts on.
    NoDescriptorId,
    /// The identifier names a descriptor the extension no longer has, or
    /// one with no versions: the state one boot before the assertion.
    DescriptorGone(String),
}

impl std::fmt::Display for BrokenPoster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.why {
            Why::NoDescriptorId => write!(
                f,
                "{} configuration {} has no descriptor identifier",
                self.extension, self.configuration
            ),
            Why::DescriptorGone(id) => write!(
                f,
                "{} configuration {} names descriptor `{id}`, which is gone",
                self.extension, self.configuration
            ),
        }
    }
}

/// `~/Library/Developer/CoreSimulator/Devices/<udid>/data`.
pub fn data_dir(udid: &str) -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| {
        PathBuf::from(h)
            .join("Library/Developer/CoreSimulator/Devices")
            .join(udid)
            .join("data")
    })
}

/// Every configuration in the store under `data` that PosterBoard cannot
/// load. Empty for a healthy store and for one not written yet.
pub fn broken_posters(data: &Path) -> Vec<BrokenPoster> {
    let mut broken = vec![];
    // the store sits under a numbered schema directory (`61` on iOS 27)
    for schema in entries(&data.join(STORE)) {
        for ext in entries(&schema.join("Extensions")) {
            let known = descriptor_ids(&ext);
            for config in entries(&ext.join("configurations")) {
                let why = match read_id(&config) {
                    None => Why::NoDescriptorId,
                    Some(id) if known.contains(&id) => continue,
                    Some(id) => Why::DescriptorGone(id),
                };
                broken.push(BrokenPoster {
                    extension: name(&ext),
                    configuration: name(&config),
                    why,
                });
            }
        }
    }
    broken
}

/// Remove the store so PosterBoard writes a new one. The simulator must be
/// shut down: a running PosterBoard holds the store open and writes it back.
pub fn rebuild(data: &Path) -> std::io::Result<()> {
    for dir in [STORE, POSTER_KIT] {
        match std::fs::remove_dir_all(data.join(dir)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// The descriptor identifiers an extension holds, from both the descriptors
/// it reported and the static ones it ships, counting only those with a
/// version to load.
fn descriptor_ids(ext: &Path) -> Vec<String> {
    ["descriptors", "staticdescriptors"]
        .iter()
        .flat_map(|kind| entries(&ext.join(kind)))
        .filter(|d| !entries(&d.join("versions")).is_empty())
        .filter_map(|d| read_id(&d))
        .collect()
}

fn read_id(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(DESCRIPTOR_ID))
        .ok()
        .map(|s| s.trim().to_string())
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

fn name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: &str = "com.apple.ClockPoster.ClockPosterExtension";

    /// The layout of sim-smix-02's store on 2026-11-01, cut down to one
    /// extension: descriptors and configurations each in a UUID directory
    /// holding the identifier file and numbered versions.
    struct Store(tempfile::TempDir);

    impl Store {
        fn new() -> Self {
            Store(tempfile::tempdir().expect("tempdir"))
        }
        fn ext(&self) -> PathBuf {
            self.0.path().join(STORE).join("61/Extensions").join(CLOCK)
        }
        fn descriptor(&self, uuid: &str, id: &str, versions: &[u32]) {
            let d = self.ext().join("descriptors").join(uuid);
            std::fs::create_dir_all(d.join("versions")).unwrap();
            std::fs::write(d.join(DESCRIPTOR_ID), id).unwrap();
            for v in versions {
                std::fs::create_dir_all(d.join("versions").join(v.to_string())).unwrap();
            }
        }
        fn configuration(&self, uuid: &str, id: Option<&str>) {
            let c = self.ext().join("configurations").join(uuid);
            std::fs::create_dir_all(c.join("versions/129/contents")).unwrap();
            std::fs::write(
                c.join("com.apple.posterkit.role.identifier"),
                "PRPosterRoleAmbient",
            )
            .unwrap();
            if let Some(id) = id {
                std::fs::write(c.join(DESCRIPTOR_ID), id).unwrap();
            }
        }
        fn data(&self) -> &Path {
            self.0.path()
        }
    }

    #[test]
    fn a_configuration_whose_descriptor_is_there_is_whole() {
        let s = Store::new();
        s.descriptor("12DA991F", "world", &[2]);
        s.configuration("801C062B", Some("world\n"));
        assert_eq!(broken_posters(s.data()), vec![]);
    }

    #[test]
    fn a_configuration_with_no_descriptor_id_is_broken() {
        let s = Store::new();
        s.descriptor("12DA991F", "world", &[2]);
        s.configuration("801C062B", None);
        assert_eq!(
            broken_posters(s.data()),
            vec![BrokenPoster {
                extension: CLOCK.into(),
                configuration: "801C062B".into(),
                why: Why::NoDescriptorId,
            }]
        );
    }

    #[test]
    fn a_configuration_naming_a_missing_descriptor_is_broken() {
        let s = Store::new();
        s.descriptor("414DCC44", "solar", &[2]);
        s.configuration("801C062B", Some("world"));
        assert_eq!(
            broken_posters(s.data())[0].why,
            Why::DescriptorGone("world".into())
        );
    }

    #[test]
    fn a_descriptor_with_no_versions_left_counts_as_gone() {
        let s = Store::new();
        s.descriptor("12DA991F", "world", &[]);
        s.configuration("801C062B", Some("world"));
        assert_eq!(broken_posters(s.data()).len(), 1);
    }

    #[test]
    fn a_static_descriptor_answers_for_a_configuration() {
        let s = Store::new();
        let d = s.ext().join("staticdescriptors/9C1A");
        std::fs::create_dir_all(d.join("versions/0")).unwrap();
        std::fs::write(d.join(DESCRIPTOR_ID), "analog").unwrap();
        s.configuration("801C062B", Some("analog"));
        assert_eq!(broken_posters(s.data()), vec![]);
    }

    #[test]
    fn every_broken_configuration_is_named() {
        let s = Store::new();
        s.descriptor("12DA991F", "world", &[2]);
        for (i, uuid) in ["A", "B", "C", "D", "E"].iter().enumerate() {
            s.configuration(uuid, (i == 0).then_some("world"));
        }
        assert_eq!(broken_posters(s.data()).len(), 4);
    }

    #[test]
    fn a_store_not_written_yet_is_not_broken() {
        let s = Store::new();
        assert_eq!(broken_posters(s.data()), vec![]);
    }

    #[test]
    fn rebuild_removes_the_store_and_poster_kit_and_nothing_else() {
        let s = Store::new();
        s.configuration("801C062B", None);
        let kit = s.data().join(POSTER_KIT).join("temp");
        std::fs::create_dir_all(&kit).unwrap();
        let app = s.data().join("Containers/Data/Application/X");
        std::fs::create_dir_all(&app).unwrap();
        rebuild(s.data()).unwrap();
        assert!(!s.data().join(STORE).exists());
        assert!(!s.data().join(POSTER_KIT).exists());
        assert!(app.exists(), "an app's data went with the posters");
        rebuild(s.data()).expect("a store already gone is what was asked for");
    }
}
