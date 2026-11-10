//! `smix sim budget`: how much room this machine has for devices, and
//! whether there is enough for the ones about to be booted.
//!
//! A release's device tests boot a device beside the two the release
//! keeps running. On a machine where somebody else had a phone and a
//! tablet up, the boot inside a test was refused part way through the
//! run. Asked first, the answer comes before anything has run, and says
//! how much is missing.

use crate::budget::{Class, Running, Verdict, decide, limits, machine, running};

/// What `smix sim budget` was given.
#[derive(clap::Args, Debug)]
pub(crate) struct BudgetArgs {
    /// A device about to be booted, by kind (repeatable): phone, tablet,
    /// tv, vision, watch, android-phone, android-tablet. Exits 1, saying
    /// what is short, when there is not room for all of them.
    #[arg(long = "need", value_parser = parse_class)]
    pub(crate) need: Vec<Class>,
}

fn parse_class(s: &str) -> Result<Class, String> {
    Ok(match s {
        "phone" => Class::Phone,
        "tablet" => Class::Tablet,
        "tv" => Class::Tv,
        "vision" => Class::Vision,
        "watch" => Class::Watch,
        "android-phone" => Class::AndroidPhone,
        "android-tablet" => Class::AndroidTablet,
        _ => {
            return Err(format!(
                "{s:?} is not a kind of device: phone, tablet, tv, vision, watch, \
                 android-phone, android-tablet"
            ));
        }
    })
}

/// Print the room, and check it against `args.need`.
pub(crate) async fn budget(args: BudgetArgs) -> Result<(), String> {
    let m = machine();
    let l = limits(m.total_gib)?;
    let up = running().await;
    let used: u64 = up.iter().map(|r| r.class.weight()).sum();
    println!(
        "budget: {} GiB for devices, {used} held by {} running, {} free",
        l.budget_gib,
        up.len(),
        l.budget_gib.saturating_sub(used)
    );
    for r in &up {
        println!(
            "  {} ({}, about {} GiB){}",
            r.id,
            r.class.name(),
            r.class.weight(),
            r.owner
                .as_deref()
                .map(|o| format!(" — {o}"))
                .unwrap_or_default()
        );
    }
    match room_for(&args.need, up, l.budget_gib, |c, r| decide(c, r, m, l)) {
        Ok(()) if args.need.is_empty() => Ok(()),
        Ok(()) => {
            println!("room for: {}", names(&args.need));
            Ok(())
        }
        Err(why) => Err(format!(
            "no room for {}: {why}\n  shut one down (`smix status` lists them), or set \
             SMIX_DEVICE_BUDGET_GIB if this machine has more to give",
            names(&args.need)
        )),
    }
}

/// Boot each of `need` in turn on paper: the first one refused says why.
/// A shortfall of memory is said in GiB.
fn room_for(
    need: &[Class],
    mut up: Vec<Running>,
    budget_gib: u64,
    decide: impl Fn(Class, &[Running]) -> Verdict,
) -> Result<(), String> {
    for (i, c) in need.iter().enumerate() {
        if let Verdict::Refuse(why) = decide(*c, &up) {
            let rest: u64 = need[i..].iter().map(|c| c.weight()).sum();
            let held: u64 = up.iter().map(|r| r.class.weight()).sum();
            let short = (held + rest).saturating_sub(budget_gib);
            let short = if short > 0 {
                format!("; short by {short} GiB")
            } else {
                String::new()
            };
            return Err(format!(
                "{why} (still to boot: {}, about {rest} GiB{short})",
                need.len() - i
            ));
        }
        up.push(Running {
            id: format!("the {} to be booted", c.name()),
            avd: None,
            class: *c,
            owner: None,
            since: None,
        });
    }
    Ok(())
}

fn names(need: &[Class]) -> String {
    need.iter().map(|c| c.name()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::{Limits, Machine};

    fn on(class: Class) -> Running {
        Running {
            id: "other".into(),
            avd: None,
            class,
            owner: Some("someone".into()),
            since: None,
        }
    }

    fn check(need: &[Class], up: Vec<Running>, budget_gib: u64) -> Result<(), String> {
        let m = Machine {
            total_gib: Some(64),
            pressure_level: Some(1),
            free_percent: Some(50),
        };
        let l = Limits {
            budget_gib,
            heavy_alone: true,
        };
        room_for(need, up, budget_gib, |c, r| decide(c, r, m, l))
    }

    #[test]
    fn a_release_beside_somebody_elses_phone_and_tablet_is_told_before_it_starts() {
        // the release's own two, then somebody's phone and tablet
        let up = vec![
            on(Class::Phone),
            on(Class::AndroidPhone),
            on(Class::Phone),
            on(Class::Tablet),
        ];
        let e = check(&[Class::Phone], up, 38).unwrap_err();
        assert!(e.contains("about 34 of a 38 GiB budget"), "{e}");
        assert!(
            e.contains("still to boot: 1, about 9 GiB; short by 5 GiB"),
            "{e}"
        );
    }

    #[test]
    fn room_for_all_of_them_passes_and_each_counts_against_the_next() {
        assert!(
            check(
                &[Class::Phone, Class::AndroidPhone],
                vec![on(Class::Phone)],
                38
            )
            .is_ok()
        );
        // 9 running and 9 each: the fourth does not fit in 38
        let e = check(&[Class::Phone; 4], vec![on(Class::Phone)], 38).unwrap_err();
        assert!(
            e.contains("still to boot: 1, about 9 GiB; short by 7 GiB"),
            "{e}"
        );
    }

    #[test]
    fn a_kind_is_read_by_its_word_and_an_unknown_one_lists_them() {
        assert_eq!(parse_class("android-tablet"), Ok(Class::AndroidTablet));
        assert!(parse_class("pixel").unwrap_err().contains("android-phone"));
    }
}
