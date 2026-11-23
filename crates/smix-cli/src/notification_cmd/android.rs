//! An emulator's: SystemUI's shade, read through the runner's tree of
//! every window.

use super::{Notice, Row, middle, other, row_of, until, walk, words};
use smix_runner_client::HttpRunnerClient;
use smix_screen::A11yNode;
use smix_sdk::android_notifications::parse;
use smix_sdk::device_control::DeviceControl;
use std::time::Duration;

const ROW: &str = "expandableNotificationRow";
/// Looks before giving up; each does one thing towards the row.
const LOOKS: usize = 12;

async fn shell(serial: &str, argv: &[&str]) -> Result<String, crate::CliError> {
    smix_adb::AdbClient::new()
        .shell(serial, argv)
        .await
        .map_err(other)
}

/// The app's, newest first, without the headings of its groups.
pub(super) async fn posted(serial: &str, bundle: &str) -> Result<Vec<Notice>, crate::CliError> {
    let dump = shell(serial, &["dumpsys", "notification", "--noredact"]).await?;
    let mut all: Vec<_> = parse(&dump)
        .into_iter()
        .filter(|n| n.package == bundle && !n.group_summary)
        .collect();
    all.sort_by_key(|n| std::cmp::Reverse(n.at_ms));
    Ok(all
        .into_iter()
        .map(|n| Notice {
            id: n.key,
            title: n.title,
            subtitle: String::new(),
            body: n.body,
            at: n.at_ms.and_then(chrono::DateTime::from_timestamp_millis),
            auto_cancel: n.auto_cancel,
        })
        .collect())
}

pub(super) struct Screen {
    client: HttpRunnerClient,
    serial: String,
}

fn find<'a>(n: &'a A11yNode, ids: &[&str]) -> Option<&'a A11yNode> {
    if n.identifier.as_deref().is_some_and(|i| ids.contains(&i)) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, ids))
}

fn is_a_row(n: &A11yNode) -> bool {
    n.identifier.as_deref() == Some(ROW)
}

/// Every notification drawn, top first. One folded in its group is drawn
/// as a line of it, under ids of its own.
fn rows_in(root: &A11yNode, shade: bool) -> Vec<Row> {
    let mut out = Vec::new();
    walk(root, &mut Vec::new(), &mut |path| {
        let Some(n) = path.last().copied() else {
            return;
        };
        if !is_a_row(n) || n.children.iter().any(holds_a_row) {
            return;
        }
        let line = find(n, &["notification_title"]);
        let Some(title) = line.or_else(|| find(n, &["title"])) else {
            return;
        };
        let body = find(n, &["notification_text", "text", "big_text"]);
        let group = path.iter().copied().find(|p| is_a_row(p));
        out.push(Row {
            title: words(title).to_string(),
            body: body.map(words).unwrap_or_default().to_string(),
            // in an unfolded group the line with the title keeps a touch to
            // itself; the middle of the row is the row's
            at: middle(n, root),
            banner: !shade,
            unfold: line
                .and(group)
                .and_then(|g| find(g, &["expand_button"]))
                .map(|b| middle(b, root)),
        });
    });
    out.sort_by(|a, b| a.at.1.total_cmp(&b.at.1));
    out
}

fn holds_a_row(n: &A11yNode) -> bool {
    is_a_row(n) || n.children.iter().any(holds_a_row)
}

impl Screen {
    pub(super) fn on(port: u16, serial: &str) -> Self {
        Self {
            client: crate::runner_target::runner_client(port),
            serial: serial.to_string(),
        }
    }

    /// The shade or the lock screen is what is in front.
    async fn shade(&self) -> Result<bool, crate::CliError> {
        let dump = shell(&self.serial, &["dumpsys", "window"]).await?;
        Ok(dump
            .lines()
            .any(|l| l.contains("mCurrentFocus=") && l.contains("NotificationShade")))
    }

    pub(super) async fn rows(&self) -> Result<Vec<Row>, crate::CliError> {
        let shade = self.shade().await?;
        let tree = self
            .client
            .accessibility_tree_only(None)
            .await
            .map_err(other)?;
        Ok(rows_in(&tree.root, shade))
    }

    async fn touch(&self, at: (f64, f64)) -> Result<(), crate::CliError> {
        self.client
            .tap_at_norm_coord(at.0, at.1)
            .await
            .map(drop)
            .map_err(other)
    }
}

/// Open one, and return its title.
pub(super) async fn tap(
    screen: &Screen,
    serial: &str,
    bundle: &str,
    all: &[Notice],
    i: usize,
) -> Result<String, crate::CliError> {
    let n = &all[i];
    let control = smix_sdk::android_device::AndroidDeviceControl::new();
    control.wake(serial).await?;
    let rested = || tokio::time::sleep(Duration::from_millis(900));
    let mut scrolls = 0;
    rested().await;
    for _ in 0..LOOKS {
        let rows = screen.rows().await?;
        let Some(row) = row_of(all, i, &rows) else {
            if !screen.shade().await? {
                shell(serial, &["cmd", "statusbar", "expand-notifications"]).await?;
            } else if scrolls < 4 {
                scrolls += 1;
                screen
                    .client
                    .swipe_at_norm_coord((0.5, 0.75), (0.5, 0.35))
                    .await
                    .map_err(other)?;
            } else {
                break;
            }
            rested().await;
            continue;
        };
        if let Some(unfold) = row.unfold {
            screen.touch(unfold).await?;
            rested().await;
            continue;
        }
        // one still sliding in drops the touch
        rested().await;
        let rows = screen.rows().await?;
        let Some(row) = row_of(all, i, &rows) else {
            continue;
        };
        screen.touch(row.at).await?;
        let opened = until(Duration::from_secs(5), || async {
            if n.auto_cancel {
                return Ok(!posted(serial, bundle).await?.iter().any(|p| p.id == n.id));
            }
            let front = control.frontmost_app(serial).await?;
            Ok(front.is_some_and(|f| f.package == bundle) && !screen.shade().await?)
        })
        .await?;
        if opened {
            return Ok(n.title.clone());
        }
    }
    let locked = shell(serial, &["dumpsys", "window"])
        .await?
        .contains("isKeyguardShowing=true");
    let why = if locked {
        "the screen is locked, and one that asks for a PIN has to be unlocked first"
    } else {
        "it was not found in the shade, or touching it did not open it"
    };
    Err(crate::CliError::Other(format!(
        "{bundle}'s notification {:?} was not opened on {serial}: {why}; `smix sim screenshot \
         {serial} <file>` shows what is in front",
        n.title
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &str, text: &str, y: f64, children: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({"rawType": "android.view.View", "identifier": id, "text": text,
            "bounds": {"x": 44, "y": y, "w": 992, "h": 100}, "enabled": true, "visible": true,
            "selected": false, "hasFocus": false, "children": children})
    }

    fn root(children: Vec<serde_json::Value>) -> A11yNode {
        serde_json::from_value(
            serde_json::json!({"rawType": "hierarchy", "enabled": true, "visible": true,
            "selected": false, "hasFocus": false,
            "bounds": {"x": 0, "y": 0, "w": 1080, "h": 2340}, "children": children}),
        )
        .expect("tree")
    }

    #[test]
    fn a_folded_group_is_unfolded_by_its_own_button_and_an_unfolded_one_is_touched() {
        let line = |y, t| {
            n(
                ROW,
                "",
                y,
                vec![
                    n("notification_title", t, y, vec![]),
                    n("notification_text", "B", y, vec![]),
                ],
            )
        };
        let folded = root(vec![n(
            ROW,
            "",
            600.0,
            vec![
                n("expand_button", "Expand", 610.0, vec![]),
                line(800.0, "T2"),
                line(900.0, "T1"),
            ],
        )]);
        let rows = rows_in(&folded, true);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!((rows[0].title.as_str(), rows[0].body.as_str()), ("T2", "B"));
        assert!((rows[0].unfold.unwrap().1 - 660.0 / 2340.0).abs() < 1e-9);

        let open = root(vec![n(
            ROW,
            "",
            600.0,
            vec![n(
                ROW,
                "",
                800.0,
                vec![
                    n("title", "T2", 810.0, vec![]),
                    n("text", "B", 860.0, vec![]),
                ],
            )],
        )]);
        let rows = rows_in(&open, false);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].unfold.is_none() && rows[0].banner);
    }
}
