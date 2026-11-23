//! `smix sim notification list|tap` — an app's notifications, and opening
//! one the way a finger does.

mod android;
mod ios;

use smix_screen::A11yNode;
use smix_simctl::SimctlClient;
use smix_simctl::registry::DeviceKind;
use std::time::{Duration, Instant};

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct NotificationArgs {
    #[command(subcommand)]
    pub action: NotificationAction,
}

#[derive(clap::Subcommand, Debug)]
pub enum NotificationAction {
    /// The notifications an app has and nobody has opened or dismissed,
    /// newest first, with where each is when a runner is up to look.
    List {
        /// Which simulator or emulator, by UDID, serial or registry alias.
        device: String,
        /// The app they belong to.
        #[arg(long)]
        bundle: String,
        /// Print a JSON array of `{"index", "id", "title", "subtitle",
        /// "body", "at", "where"}`. `where` is `banner`, `list` (in the
        /// notification centre, the shade or the lock screen, on screen
        /// now), `hidden`, or null without a runner.
        #[arg(long)]
        json: bool,
    },
    /// Open one of an app's notifications, with the app in the background
    /// or not running, and the screen locked or not. Its runner must be up.
    ///
    /// Returns once the notification is gone from the app's list.
    Tap {
        /// Which simulator or emulator, by UDID, serial or registry alias.
        device: String,
        /// The app it belongs to.
        #[arg(long)]
        bundle: String,
        /// Its title, exactly; the newest when several have it.
        #[arg(long, conflicts_with = "index", required_unless_present = "index")]
        title: Option<String>,
        /// Its place in `list`, 0 being the newest.
        #[arg(long)]
        index: Option<usize>,
        /// Runner port; unset: SMIX_RUNNER_PORT, the runner smix brought up on DEVICE (the --device of other verbs), its registered runnerPort, 22087 (iOS) / 28080 (Android).
        #[arg(long, alias = "runner-port")]
        port: Option<u16>,
    },
}

impl NotificationArgs {
    pub(crate) fn device(&self) -> &str {
        match &self.action {
            NotificationAction::List { device, .. } | NotificationAction::Tap { device, .. } => {
                device
            }
        }
    }
}

/// One notification, whichever platform it is on.
#[derive(Debug, Clone, PartialEq)]
struct Notice {
    id: String,
    title: String,
    subtitle: String,
    body: String,
    at: Option<chrono::DateTime<chrono::Utc>>,
    /// Android: opening it takes it away.
    auto_cancel: bool,
}

/// A notification drawn on the screen.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    title: String,
    body: String,
    /// Where to touch it, 0..1 of the screen.
    at: (f64, f64),
    /// Over whatever is in front, rather than in a list of them.
    banner: bool,
    /// The middle of what unfolds its group, when it is folded in one.
    unfold: Option<(f64, f64)>,
}

fn other(e: impl std::fmt::Display) -> crate::CliError {
    crate::CliError::Other(e.to_string())
}

fn walk<'a>(n: &'a A11yNode, path: &mut Vec<&'a A11yNode>, see: &mut dyn FnMut(&[&'a A11yNode])) {
    path.push(n);
    see(path);
    for c in &n.children {
        walk(c, path, see);
    }
    path.pop();
}

/// The middle of a node, 0..1 of `root`.
fn middle(n: &A11yNode, root: &A11yNode) -> (f64, f64) {
    let (b, r) = (&n.bounds, &root.bounds);
    (
        ((b.x + b.w / 2.0 - r.x) / r.w).clamp(0.0, 1.0),
        ((b.y + b.h / 2.0 - r.y) / r.h).clamp(0.0, 1.0),
    )
}

fn words(n: &A11yNode) -> &str {
    n.text
        .as_deref()
        .or(n.label.as_deref())
        .or(n.value.as_deref())
        .unwrap_or_default()
}

/// Whether a drawn row is this notification. One without a title is shown
/// under its app's name, so its words are its body.
fn is_row(n: &Notice, r: &Row) -> bool {
    r.body == n.body && (n.title.is_empty() || r.title == n.title)
}

/// The row that is `all[i]`. Among several with the same words both run
/// newest first, which tells them apart only when every one is drawn: with
/// one still folded away, the second drawn may be the third.
fn row_of<'a>(all: &[Notice], i: usize, rows: &'a [Row]) -> Option<&'a Row> {
    let n = &all[i];
    let newer = all[..i]
        .iter()
        .filter(|o| o.title == n.title && o.body == n.body)
        .count();
    let alike = all
        .iter()
        .filter(|o| o.title == n.title && o.body == n.body)
        .count();
    let same: Vec<&Row> = rows.iter().filter(|r| is_row(n, r)).collect();
    if newer > 0 && same.len() < alike {
        return None;
    }
    same.get(newer).copied()
}

fn which(
    all: &[Notice],
    title: Option<&str>,
    index: Option<usize>,
    bundle: &str,
) -> Result<usize, String> {
    let have = || {
        if all.is_empty() {
            return "it has none".to_string();
        }
        let names: Vec<String> = all
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{i}: {:?}", n.title))
            .collect();
        format!("it has {}", names.join(", "))
    };
    match (title, index) {
        (Some(t), _) => all
            .iter()
            .position(|n| n.title == t)
            .ok_or_else(|| format!("{bundle} has no notification titled {t:?}; {}", have())),
        (None, Some(i)) if i < all.len() => Ok(i),
        (None, i) => Err(format!(
            "{bundle} has no notification at index {}; {}",
            i.unwrap_or_default(),
            have()
        )),
    }
}

fn print_list(all: &[Notice], rows: Option<&[Row]>, json: bool) {
    let place = |i: usize| {
        rows.map(|rows| match row_of(all, i, rows) {
            Some(r) if r.banner => "banner",
            Some(_) => "list",
            None => "hidden",
        })
    };
    if json {
        let out: Vec<serde_json::Value> = all
            .iter()
            .enumerate()
            .map(|(i, n)| {
                serde_json::json!({
                    "index": i,
                    "id": n.id,
                    "title": n.title,
                    "subtitle": n.subtitle,
                    "body": n.body,
                    "at": n.at.map(|t| t.to_rfc3339()),
                    "where": place(i),
                })
            })
            .collect();
        println!("{:#}", serde_json::Value::Array(out));
        return;
    }
    if all.is_empty() {
        println!("(no notifications)");
    }
    for (i, n) in all.iter().enumerate() {
        let at = n.at.map_or_else(String::new, |t| {
            format!("  {}", t.with_timezone(&chrono::Local).format("%H:%M:%S"))
        });
        let place = place(i).map_or_else(String::new, |p| format!("  [{p}]"));
        println!("{i}. {:?} — {:?}{at}{place}", n.title, n.body);
    }
}

/// Wait until `gone` answers true, up to `most`.
async fn until<F, Fut>(most: Duration, mut gone: F) -> Result<bool, crate::CliError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<bool, crate::CliError>>,
{
    let deadline = Instant::now() + most;
    loop {
        // a look that began before the deadline still counts
        let looked = Instant::now();
        if gone().await? {
            return Ok(true);
        }
        if looked >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn run(simctl: &SimctlClient, args: NotificationArgs) -> Result<(), crate::CliError> {
    let is_ios = crate::device_kind_of(args.device()) == DeviceKind::Simulator;
    let device = if is_ios {
        crate::resolve_device(args.device())?
    } else {
        crate::resolve_android_serial(args.device())?
    };
    match &args.action {
        NotificationAction::List { bundle, json, .. } => {
            let up = crate::runner_target::recorded_runner_port(&device);
            let (all, rows) = if is_ios {
                let rows = match up {
                    Some(p) => Some(ios::Screen::on(p).look().await?.rows),
                    None => None,
                };
                (ios::delivered(simctl, &device, bundle).await?, rows)
            } else {
                let rows = match up {
                    Some(p) => Some(android::Screen::on(p, &device).rows().await?),
                    None => None,
                };
                (android::posted(&device, bundle).await?, rows)
            };
            print_list(&all, rows.as_deref(), *json);
            Ok(())
        }
        NotificationAction::Tap {
            bundle,
            title,
            index,
            port,
            ..
        } => {
            let (port, _) =
                crate::runner_target::runner_dial_target(*port, Some(args.device()), None)?;
            let (title, index) = (title.as_deref(), *index);
            let opened = if is_ios {
                let all = ios::delivered(simctl, &device, bundle).await?;
                ios::tap(
                    simctl,
                    &ios::Screen::on(port),
                    &device,
                    bundle,
                    all,
                    title,
                    index,
                )
                .await?
            } else {
                let all = android::posted(&device, bundle).await?;
                let i = which(&all, title, index, bundle).map_err(other)?;
                android::tap(
                    &android::Screen::on(port, &device),
                    &device,
                    bundle,
                    &all,
                    i,
                )
                .await?
            };
            println!("opened: {device} {bundle} {opened:?}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(title: &str, body: &str) -> Notice {
        Notice {
            id: String::new(),
            title: title.into(),
            subtitle: String::new(),
            body: body.into(),
            at: None,
            auto_cancel: true,
        }
    }

    fn row(title: &str, body: &str, y: f64) -> Row {
        Row {
            title: title.into(),
            body: body.into(),
            at: (0.5, y),
            banner: false,
            unfold: None,
        }
    }

    #[test]
    fn a_row_is_found_by_its_words_and_its_place_among_the_same() {
        let all = [notice("A", "1"), notice("B", "2"), notice("A", "1")];
        let rows = [row("A", "1", 0.1), row("B", "2", 0.2), row("A", "1", 0.3)];
        assert_eq!(row_of(&all, 2, &rows).map(|r| r.at.1), Some(0.3));
        assert_eq!(row_of(&all, 1, &rows).map(|r| r.at.1), Some(0.2));
        assert_eq!(row_of(&all, 2, &rows[..1]), None, "folded away");
        let three = [notice("A", "1"), notice("A", "1"), notice("A", "1")];
        let two = [row("A", "1", 0.1), row("A", "1", 0.3)];
        assert_eq!(row_of(&three, 1, &two), None, "which two are drawn?");
        assert_eq!(row_of(&three, 0, &two).map(|r| r.at.1), Some(0.1));
        let untitled = [notice("", "1")];
        assert!(row_of(&untitled, 0, &[row("The App", "1", 0.1)]).is_some());
    }

    #[test]
    fn a_title_names_the_newest_and_a_miss_says_what_there_is() {
        let all = [notice("A", "1"), notice("B", "2"), notice("A", "3")];
        assert_eq!(which(&all, Some("A"), None, "x"), Ok(0));
        assert_eq!(which(&all, None, Some(2), "x"), Ok(2));
        assert!(
            which(&all, Some("C"), None, "x")
                .unwrap_err()
                .contains("\"B\"")
        );
        assert!(
            which(&all, None, Some(3), "x")
                .unwrap_err()
                .contains("index 3")
        );
        assert!(
            which(&[], Some("C"), None, "x")
                .unwrap_err()
                .contains("none")
        );
    }
}
