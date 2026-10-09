use super::*;

fn settings(pairs: &[(&str, Option<&str>)]) -> Settings {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
        .collect()
}

/// A fresh emulator: two scales at the platform default, the third
/// never written.
fn fresh() -> Settings {
    settings(&[
        ("animator_duration_scale", None),
        ("transition_animation_scale", Some("1.0")),
        ("window_animation_scale", Some("1.0")),
    ])
}

fn zeros() -> Settings {
    settings(&[
        ("animator_duration_scale", Some("0")),
        ("transition_animation_scale", Some("0")),
        ("window_animation_scale", Some("0")),
    ])
}

fn is_zero(v: &str) -> bool {
    v.parse::<f64>().is_ok_and(|n| n == 0.0)
}

fn proc_(pid: u32) -> ProcIdentity {
    ProcIdentity {
        pid,
        started_at: format!("started-{pid}"),
        cmd: "smix run".into(),
    }
}

fn record(holders: &[u32]) -> HeldSettings {
    HeldSettings {
        device_id: "emulator-5554".into(),
        originals: fresh(),
        holders: holders.iter().map(|p| proc_(*p)).collect(),
    }
}

#[test]
fn nothing_recorded_means_the_device_is_the_truth() {
    assert_eq!(baseline(None, false, &zeros(), is_zero), Baseline::Current);
}

#[test]
fn a_live_holder_means_the_record_is_the_truth() {
    let r = record(&[10]);
    assert_eq!(
        baseline(Some(&r), true, &zeros(), is_zero),
        Baseline::Recorded
    );
    // even when the device shows something else: the holder's originals stand
    assert_eq!(
        baseline(Some(&r), true, &fresh(), is_zero),
        Baseline::Recorded
    );
}

/// The SIGKILL case: the record's holders are gone and the device shows
/// exactly the zeros they wrote. Those zeros are not what the device had.
#[test]
fn dead_holders_with_their_zeros_still_showing_keep_the_record() {
    let r = record(&[10]);
    assert_eq!(
        baseline(Some(&r), false, &zeros(), is_zero),
        Baseline::Recorded
    );
    // `0.0` is a zero too, however the device spells it
    let spelled = settings(&[
        ("animator_duration_scale", Some("0.0")),
        ("transition_animation_scale", Some("0.0")),
        ("window_animation_scale", Some("0.0")),
    ]);
    assert_eq!(
        baseline(Some(&r), false, &spelled, is_zero),
        Baseline::Recorded
    );
}

#[test]
fn dead_holders_and_a_changed_device_make_the_device_the_truth() {
    let r = record(&[10]);
    // one key put back by hand
    let mut current = zeros();
    current.insert("window_animation_scale".into(), Some("1.0".into()));
    assert_eq!(
        baseline(Some(&r), false, &current, is_zero),
        Baseline::Current
    );
    // a key deleted since
    let mut current = zeros();
    current.insert("animator_duration_scale".into(), None);
    assert_eq!(
        baseline(Some(&r), false, &current, is_zero),
        Baseline::Current
    );
}

#[test]
fn a_record_with_no_keys_cannot_vouch_for_anything() {
    let r = HeldSettings {
        originals: Settings::new(),
        ..record(&[10])
    };
    assert_eq!(
        baseline(Some(&r), false, &zeros(), is_zero),
        Baseline::Current
    );
}

fn dir() -> (tempfile::TempDir, HeldDir) {
    let t = tempfile::tempdir().unwrap();
    let d = HeldDir::at(t.path().join("held-settings"));
    (t, d)
}

fn on_disk(d: &HeldDir, device: &str) -> Option<HeldSettings> {
    read(&d.path().join(format!("{device}.json"))).unwrap()
}

#[test]
fn the_first_holder_records_what_it_saw_and_writes() {
    let (_t, d) = dir();
    let j = join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap();
    assert_eq!(
        j,
        Joined {
            first: true,
            originals: fresh()
        }
    );
    let r = on_disk(&d, "emulator-5554").expect("a record was written");
    assert_eq!(r.originals, fresh());
    assert_eq!(r.holders, vec![proc_(1)]);
    assert!(d.path().join("emulator-5554.lock").exists());
}

#[test]
fn a_later_holder_only_registers_and_inherits_the_originals() {
    let (_t, d) = dir();
    join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap();
    // by now the device shows zeros; they must not become the originals
    let j = join(&d, "emulator-5554", proc_(2), &zeros(), is_zero, |_| true).unwrap();
    assert_eq!(
        j,
        Joined {
            first: false,
            originals: fresh()
        }
    );
    let r = on_disk(&d, "emulator-5554").unwrap();
    assert_eq!(r.holders, vec![proc_(1), proc_(2)]);
}

#[test]
fn leaving_restores_only_when_the_last_holder_goes() {
    let (_t, d) = dir();
    join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap();
    join(&d, "emulator-5554", proc_(2), &zeros(), is_zero, |_| true).unwrap();
    assert!(
        leave(&d, "emulator-5554", &proc_(1), |_| true)
            .unwrap()
            .is_none()
    );
    let r = on_disk(&d, "emulator-5554").expect("one holder remains");
    assert_eq!(r.holders, vec![proc_(2)]);
    let last = leave(&d, "emulator-5554", &proc_(2), |_| true)
        .unwrap()
        .expect("the last holder is told so");
    assert_eq!(*last.originals(), fresh());
    assert!(
        on_disk(&d, "emulator-5554").is_some(),
        "the record stays until the device is put back"
    );
    last.restored().unwrap();
    assert!(on_disk(&d, "emulator-5554").is_none(), "the record is gone");
}

/// A restore that never finishes — adb failed, or the process died
/// between leaving and writing — keeps the record, so the next run
/// finds the originals rather than the zeros.
#[test]
fn a_restore_that_does_not_finish_keeps_the_record_for_the_next_run() {
    let (_t, d) = dir();
    join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap();
    let last = leave(&d, "emulator-5554", &proc_(1), |_| true)
        .unwrap()
        .unwrap();
    drop(last);
    let j = join(&d, "emulator-5554", proc_(2), &zeros(), is_zero, |h| {
        h.pid != 1
    })
    .unwrap();
    assert_eq!(
        j,
        Joined {
            first: true,
            originals: fresh()
        }
    );
}

#[test]
fn leaving_with_nothing_recorded_restores_nothing() {
    let (_t, d) = dir();
    assert!(
        leave(&d, "emulator-5554", &proc_(1), |_| true)
            .unwrap()
            .is_none()
    );
}

/// A holder killed by SIGKILL never leaves. The next run prunes it and,
/// being alone, takes the recorded originals over the zeros it sees.
#[test]
fn a_dead_holder_is_pruned_and_its_originals_taken_over() {
    let (_t, d) = dir();
    join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap();
    let dead = |h: &ProcIdentity| h.pid != 1;
    let j = join(&d, "emulator-5554", proc_(2), &zeros(), is_zero, dead).unwrap();
    assert_eq!(
        j,
        Joined {
            first: true,
            originals: fresh()
        }
    );
    let r = on_disk(&d, "emulator-5554").unwrap();
    assert_eq!(r.holders, vec![proc_(2)], "the dead holder is gone");
    let last = leave(&d, "emulator-5554", &proc_(2), dead)
        .unwrap()
        .unwrap();
    assert_eq!(*last.originals(), fresh());
}

#[test]
fn a_dead_holder_whose_zeros_were_undone_is_not_believed() {
    let (_t, d) = dir();
    join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap();
    let by_hand = settings(&[
        ("animator_duration_scale", Some("0.5")),
        ("transition_animation_scale", Some("0.5")),
        ("window_animation_scale", Some("0.5")),
    ]);
    let j = join(&d, "emulator-5554", proc_(2), &by_hand, is_zero, |h| {
        h.pid != 1
    })
    .unwrap();
    assert_eq!(j.originals, by_hand);
}

#[test]
fn leaving_prunes_the_dead_too() {
    let (_t, d) = dir();
    join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap();
    join(&d, "emulator-5554", proc_(2), &zeros(), is_zero, |_| true).unwrap();
    // 1 died; 2 leaving is the last alive, so it restores
    let last = leave(&d, "emulator-5554", &proc_(2), |h| h.pid != 1)
        .unwrap()
        .unwrap();
    assert_eq!(*last.originals(), fresh());
}

#[test]
fn a_device_id_that_is_not_a_filename_is_refused() {
    let (_t, d) = dir();
    let err = join(&d, "../x", proc_(1), &fresh(), is_zero, |_| true).unwrap_err();
    assert!(matches!(err, LeaseError::BadDeviceId { .. }), "{err}");
    let err = leave(&d, "", &proc_(1), |_| true).unwrap_err();
    assert!(matches!(err, LeaseError::BadDeviceId { .. }), "{err}");
    assert!(!d.path().exists(), "nothing was created for a refused id");
}

#[test]
fn a_malformed_record_is_an_error_not_an_absence() {
    let (_t, d) = dir();
    std::fs::create_dir_all(d.path()).unwrap();
    std::fs::write(d.path().join("emulator-5554.json"), b"{").unwrap();
    let err = join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap_err();
    assert!(matches!(err, LeaseError::Malformed { .. }), "{err}");
}

#[test]
fn an_absent_key_survives_the_round_trip_as_absent() {
    let (_t, d) = dir();
    join(&d, "emulator-5554", proc_(1), &fresh(), is_zero, |_| true).unwrap();
    let text = std::fs::read_to_string(d.path().join("emulator-5554.json")).unwrap();
    assert!(
        text.contains("\"animator_duration_scale\": null"),
        "absent is written as null, not as a default:\n{text}"
    );
    let last = leave(&d, "emulator-5554", &proc_(1), |_| true)
        .unwrap()
        .unwrap();
    let back = last.originals();
    assert_eq!(back["animator_duration_scale"], None);
    assert_eq!(back["window_animation_scale"].as_deref(), Some("1.0"));
}
