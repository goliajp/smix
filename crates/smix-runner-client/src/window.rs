//! `POST /window-size` — an app's desktop window set to a size in dp.

use serde::{Deserialize, Serialize};
use smix_input::WindowSize;

use crate::{HttpRunnerClient, RunnerTransportError};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Req<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<&'a str>,
    width_dp: u32,
    height_dp: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Answer {
    width_dp: u32,
    height_dp: u32,
}

impl HttpRunnerClient {
    /// `POST /window-size` — resize the window of `app` (the app in front
    /// when `None`) and read back the size it then has. Only the Android
    /// runner serves it. A window that reads back at another size is an
    /// answer, not an error: the caller compares. A window that cannot be
    /// resized (none, fullscreen, too large, below the system's smallest)
    /// is refused with the runner's reason.
    pub async fn window_size(
        &self,
        app: Option<&str>,
        size: WindowSize,
    ) -> Result<WindowSize, RunnerTransportError> {
        let a: Answer = self
            .json_post_within(
                "/window-size",
                &Req {
                    package: app,
                    width_dp: size.width_dp,
                    height_dp: size.height_dp,
                },
                None,
                crate::route_limits::wait_for_request(
                    "/window-size",
                    std::time::Duration::ZERO,
                    self.auto_activate,
                )?,
            )
            .await?;
        Ok(WindowSize {
            width_dp: a.width_dp,
            height_dp: a.height_dp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_the_one_the_android_runner_reads() {
        let v = serde_json::to_value(Req {
            package: Some("a.b"),
            width_dp: 1024,
            height_dp: 768,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"package": "a.b", "widthDp": 1024, "heightDp": 768})
        );
        let v = serde_json::to_value(Req {
            package: None,
            width_dp: 1,
            height_dp: 2,
        })
        .unwrap();
        assert_eq!(v, serde_json::json!({"widthDp": 1, "heightDp": 2}));
    }

    #[test]
    fn the_answer_is_the_size_read_back() {
        let a: Answer =
            serde_json::from_str(r#"{"ok":false,"widthDp":220,"heightDp":220,"package":"a.b"}"#)
                .unwrap();
        assert_eq!((a.width_dp, a.height_dp), (220, 220));
    }
}
