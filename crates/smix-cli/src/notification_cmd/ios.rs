//! A simulator's: SpringBoard draws them, so every look and touch is
//! aimed at SpringBoard. Aimed at the app under test, one that is in the
//! background or gone keeps the runner waiting half a minute.

use super::{Notice, Row, is_row, middle, other, row_of, until, walk, which, words};
use smix_runner_client::HttpRunnerClient;
use smix_screen::A11yNode;
use smix_simctl::SimctlClient;
use std::time::Duration;

const SPRINGBOARD: &str = "com.apple.springboard";
/// The lock screen and the notification centre are the one window.
const COVER: &str = "SBCoverSheetWindow";
const CELL: &str = "NotificationShortLookView";
/// Looks before giving up; each does one thing towards the row.
const LOOKS: usize = 18;

pub(super) async fn delivered(
    simctl: &SimctlClient,
    udid: &str,
    bundle: &str,
) -> Result<Vec<Notice>, crate::CliError> {
    let all = simctl.delivered_notifications(udid, bundle).await?;
    Ok(all
        .into_iter()
        .map(|d| Notice {
            id: d.id,
            title: d.title,
            subtitle: d.subtitle,
            body: d.body,
            #[allow(clippy::cast_possible_truncation)]
            at: d
                .at
                .and_then(|t| chrono::DateTime::from_timestamp_millis((t * 1000.0) as i64)),
            auto_cancel: true,
        })
        .collect())
}

pub(super) struct Screen(HttpRunnerClient);

/// What is drawn now.
pub(super) struct Looked {
    pub(super) rows: Vec<Row>,
    /// The lock screen or the notification centre is up.
    cover: bool,
}

/// The first descendant with `id`.
fn find<'a>(n: &'a A11yNode, id: &str) -> Option<&'a A11yNode> {
    if n.identifier.as_deref() == Some(id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, id))
}

fn looked(root: &A11yNode) -> Looked {
    let mut out = Looked {
        rows: Vec::new(),
        cover: false,
    };
    walk(root, &mut Vec::new(), &mut |path| {
        let Some(n) = path.last() else { return };
        let covered = path.iter().any(|p| p.identifier.as_deref() == Some(COVER));
        out.cover |= covered;
        if n.identifier.as_deref() != Some(CELL) {
            return;
        }
        let said = |id: &str| find(n, id).map(words).unwrap_or_default().to_string();
        out.rows.push(Row {
            title: said("NotificationTitle"),
            body: said("TextContent.Primary"),
            at: middle(n, root),
            banner: !covered,
            unfold: None,
        });
    });
    out.rows.sort_by(|a, b| a.at.1.total_cmp(&b.at.1));
    out
}

impl Screen {
    pub(super) fn on(port: u16) -> Self {
        Self(crate::runner_target::runner_client(port).with_target_bundle_id(SPRINGBOARD))
    }

    pub(super) async fn look(&self) -> Result<Looked, crate::CliError> {
        let tree = self.0.get_tree(None).await.map_err(other)?;
        Ok(looked(&tree.root))
    }

    async fn touch(&self, at: (f64, f64)) -> Result<(), crate::CliError> {
        self.0
            .tap_at_norm_coord(at.0, at.1)
            .await
            .map(drop)
            .map_err(other)
    }

    async fn drag(&self, from: (f64, f64), to: (f64, f64)) -> Result<(), crate::CliError> {
        self.0.swipe_at_norm_coord(from, to).await.map_err(other)?;
        // what it moved comes to rest
        tokio::time::sleep(Duration::from_millis(1200)).await;
        Ok(())
    }

    async fn home(&self) -> Result<(), crate::CliError> {
        self.0
            .press_key(smix_input::KeyName::Home)
            .await
            .map_err(other)?;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        Ok(())
    }

    /// Open a drawn notification: a banner by a touch, a row in a list by
    /// the swipe to the right, since SpringBoard answers a touch on one by
    /// only showing that it can be swiped.
    async fn open(&self, row: &Row) -> Result<(), crate::CliError> {
        if row.banner {
            self.touch(row.at).await
        } else {
            self.drag((0.15, row.at.1), (0.95, row.at.1)).await
        }
    }
}

/// Open one, and return its title.
pub(super) async fn tap(
    simctl: &SimctlClient,
    screen: &Screen,
    udid: &str,
    bundle: &str,
    all: Vec<Notice>,
    title: Option<&str>,
    index: Option<usize>,
) -> Result<String, crate::CliError> {
    let i = match which(&all, title, index, bundle) {
        Ok(i) => i,
        Err(why) => return banner_only(screen, title, why).await,
    };
    let id = all[i].id.clone();
    let opened = |most: Duration| {
        let id = id.clone();
        async move {
            until(most, || async {
                let now = delivered(simctl, udid, bundle).await?;
                Ok(!now.iter().any(|n| n.id == id))
            })
            .await
        }
    };
    let mut unfolded: Vec<(String, String)> = Vec::new();
    let mut nudges = 0;
    for _ in 0..LOOKS {
        let now = screen.look().await?;
        if let Some(row) = row_of(&all, i, &now.rows) {
            screen.open(row).await?;
            if opened(Duration::from_secs(4)).await? {
                return Ok(all[i].title.clone());
            }
            // a folded group unfolds at a touch
            if !row.banner {
                screen.touch(row.at).await?;
                tokio::time::sleep(Duration::from_millis(1200)).await;
            }
            continue;
        }
        if !now.cover {
            // the notification centre, from the top edge clear of the island
            screen.drag((0.2, 0.0), (0.2, 0.7)).await?;
            continue;
        }
        // one of this app's is in the way: the front of its folded group
        if let Some(front) = now.rows.iter().find(|r| {
            all.iter().any(|n| is_row(n, r))
                && !unfolded.contains(&(r.title.clone(), r.body.clone()))
        }) {
            unfolded.push((front.title.clone(), front.body.clone()));
            screen.touch(front.at).await?;
            tokio::time::sleep(Duration::from_millis(1200)).await;
            continue;
        }
        // further down the list, or behind a lock screen that is asleep or
        // keeps earlier ones below the edge
        nudges += 1;
        if nudges % 3 == 0 {
            unfolded.clear();
            screen.home().await?;
        } else {
            screen.drag((0.5, 0.75), (0.5, 0.35)).await?;
        }
    }
    Err(crate::CliError::Other(format!(
        "{bundle}'s notification {:?} is delivered, but it was not found on the screen of {udid} \
         to open; `smix sim screenshot {udid} <file>` shows what is in front",
        all[i].title
    )))
}

/// A notification shown as a banner and kept nowhere else, as an app in
/// front may ask for.
async fn banner_only(
    screen: &Screen,
    title: Option<&str>,
    why: String,
) -> Result<String, crate::CliError> {
    let Some(title) = title else {
        return Err(crate::CliError::Other(why));
    };
    let shown = |rows: &[Row]| rows.iter().find(|r| r.banner && r.title == title).cloned();
    let Some(row) = shown(&screen.look().await?.rows) else {
        return Err(crate::CliError::Other(why));
    };
    screen.open(&row).await?;
    let gone = until(Duration::from_secs(3), || async {
        Ok(shown(&screen.look().await?.rows).is_none())
    })
    .await?;
    if gone {
        return Ok(title.to_string());
    }
    Err(crate::CliError::Other(format!(
        "the banner {title:?} was touched and is still there"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(json: serde_json::Value) -> A11yNode {
        serde_json::from_value(json).expect("node")
    }

    fn cell(y: f64, title: &str, body: &str) -> serde_json::Value {
        let at = serde_json::json!({"x": 14, "y": y, "w": 374, "h": 66});
        let leaf = |id: &str, label: &str| {
            serde_json::json!({"rawType": "staticText", "identifier": id, "label": label,
                "bounds": at, "enabled": true, "visible": true, "selected": false,
                "hasFocus": false, "children": []})
        };
        serde_json::json!({"rawType": "other", "identifier": CELL, "bounds": at, "enabled": true,
            "visible": true, "selected": false, "hasFocus": false,
            "children": [leaf("NotificationTitle", title), leaf("TextContent.Primary", body)]})
    }

    fn screen(children: Vec<serde_json::Value>, window: &str) -> A11yNode {
        let full = serde_json::json!({"x": 0, "y": 0, "w": 402, "h": 874});
        node(
            serde_json::json!({"rawType": "application", "bounds": full, "enabled": true,
            "visible": true, "selected": false, "hasFocus": false, "children": [
                {"rawType": "window", "identifier": window, "bounds": full, "enabled": true,
                 "visible": true, "selected": false, "hasFocus": false, "children": children}]}),
        )
    }

    #[test]
    fn rows_on_the_cover_sheet_are_a_list_and_the_rest_are_banners() {
        let l = looked(&screen(
            vec![cell(676.0, "T1", "B1"), cell(602.0, "T2", "B2")],
            COVER,
        ));
        assert!(l.cover);
        let titles: Vec<&str> = l.rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["T2", "T1"], "top first");
        assert!(l.rows.iter().all(|r| !r.banner));
        assert!((l.rows[0].at.1 - 635.0 / 874.0).abs() < 1e-9);

        let l = looked(&screen(vec![cell(60.0, "T", "B")], "other"));
        assert!(!l.cover && l.rows[0].banner && l.rows[0].body == "B");
    }
}
