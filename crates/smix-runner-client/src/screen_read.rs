//! Reading the screen: the probe's tree where the app carries one, the
//! accessibility tree for everything else.

use smix_screen::{A11yNode, derive_roles_recursive};

use crate::{HttpRunnerClient, IncludeScope, PerceivedTree, RunnerTransportError, TreeSource};

impl HttpRunnerClient {
    /// `GET /tree?include=` — full a11y tree dump.
    ///
    /// Post-deser pass [`derive_roles_recursive`] fills
    /// `A11yNode.role` from `raw_type` because the Swift `/tree` route
    /// only emits `rawType` on the wire. Without this, `Selector::Role`
    /// would never match real-sim payloads.
    pub async fn get_tree(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<PerceivedTree, RunnerTransportError> {
        // The probe first. One perception primitive with two satisfiers,
        // chosen at the source: every caller downstream keeps working
        // against one tree, and none of them grows an
        // `if probe { … } else { … }`.
        //
        // The app is named when the caller has a name — a flow always
        // does — and left out when it does not, in which case the runner
        // answers about the window holding the focus. It used to be a
        // precondition: no bundle, no probe. Every CLI verb builds its
        // client without one, so `smix tree` answered from the
        // accessibility projection while a flow on the same screen
        // answered from the semantics tree, and two e2e scripts in this
        // cycle had to bypass the CLI to see what the flow was seeing.
        //
        // A probe that is not there, or a screen with no app window in
        // front, falls through to the accessibility tree — and the answer
        // says which, because a screen the accessibility reader has gone
        // blind on and a screen with nothing on it print identically
        // otherwise.
        //
        // The probe sees the app and nothing else — not the keyboard, not
        // the system bars, not another app's dialog on top. Its tree alone
        // made those absent rather than unread: `role:keyboard` timed out
        // on every app that carried the probe while the keyboard was on
        // screen. So the app's windows come from the probe and every other
        // window from the accessibility reader, and a reader that cannot
        // answer is an error here rather than half a screen passed off as
        // the whole of it.
        //
        // The reader is told which app that is, and leaves its windows
        // hollow — the window itself (where it sits in the stack, whether
        // it has the focus), not what is in it. Its contents are replaced
        // by the probe's a line below, so walking them was work thrown
        // away. Measured on the fixture's watch screen (2026-09-25): the
        // read 30 → 19 ms, and `neverVisible` over the same span 12 looks
        // with a 131 ms longest gap → 18–19 looks, 82–89 ms (host load 13
        // and 10, so not a strict A/B). A runner that predates the
        // parameter ignores it and walks everything: slower, still right.
        //
        // Once the probe has said it is there, its tree and the rest of
        // the screen are asked for at once: a look is then the probe's
        // answer plus the slower of the two, not the sum of all three.
        // On the fixture's watch screen (2026-09-27, load 7-9) that took
        // `neverVisible` from 12-14 looks in about 2.2 s to 12-19.
        let named = self.target_bundle_id.as_deref();
        if let Some(answered_for) = self.probe_present(named).await {
            let app = self.target_bundle_id.clone().or(answered_for);
            // `None`: no app to leave hollow, so the rest of the screen is
            // read whole, and only once the probe's tree is in hand.
            let (app_tree, screen) = match app.as_deref() {
                Some(a) => {
                    let endpoint = format!("/tree?hollow={a}");
                    let hollow = self.json_get::<A11yNode>(&endpoint, include);
                    let (app_tree, screen) = tokio::join!(self.probe_tree(named), hollow);
                    (app_tree, Some(screen))
                }
                None => (self.probe_tree(named).await, None),
            };
            let Some(app_tree) = app_tree else {
                return self.accessibility_tree_only(include).await;
            };
            let screen = match screen {
                Some(read) => {
                    let mut root = read?;
                    derive_roles_recursive(&mut root);
                    root
                }
                None => self.accessibility_tree_only(include).await?.root,
            };
            let mut root = smix_screen::beside_other_windows(screen, app_tree, app.as_deref());
            self.mark_a_one_app_root(&mut root);
            return Ok(PerceivedTree {
                source: TreeSource::Semantics,
                root,
            });
        }
        self.accessibility_tree_only(include).await
    }

    /// Say whose screen a tree is when the whole tree is one app's.
    ///
    /// An iOS tree is the `XCUIApplication` for the bundle this client
    /// named, and a probe's tree is the app's own roots; neither carries
    /// window information, because there is only the one window to
    /// speak of. A failure then had no line saying whose screen it
    /// happened on. The host knows which app it asked for; when it asked
    /// for none, the package is left out rather than guessed. Android's
    /// accessibility tree is not touched: its windows are its children
    /// and say whose they are themselves.
    fn mark_a_one_app_root(&self, root: &mut A11yNode) {
        if root.window.is_none()
            && (root.raw_type == "application" || root.raw_type == "SemanticsRoots")
        {
            root.window = Some(smix_screen::WindowInfo {
                package: self.target_bundle_id.clone(),
                kind: smix_screen::WindowKind::Application,
                focused: true,
                layer: None,
                touchable: None,
            });
        }
    }

    /// The semantics tree and nothing else, or an error saying why not.
    ///
    /// [`Self::get_tree`] takes whichever reader can answer, best first.
    /// That is right for a caller who wants to see the screen and wrong
    /// for one comparing the two readers: handed the other tree without
    /// a word, a comparison finds the two in perfect agreement. So this
    /// refuses instead — a reader named is a reader asked.
    pub async fn semantics_tree_only(&self) -> Result<PerceivedTree, RunnerTransportError> {
        match self.semantics_tree(self.target_bundle_id.as_deref()).await {
            Some((root, _)) => Ok(PerceivedTree {
                source: TreeSource::Semantics,
                root,
            }),
            None => Err(RunnerTransportError::NonSuccessStatus {
                endpoint: "/probe/tree".into(),
                status: 404,
                body: "no semantics tree here: the app in front carries no smix probe, \
                       or no application window holds the focus. `smix tree` without \
                       --reader takes the accessibility tree in that case."
                    .into(),
            }),
        }
    }

    /// The accessibility tree and nothing else. Sibling of
    /// [`Self::semantics_tree_only`], and the reason is the same one.
    pub async fn accessibility_tree_only(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<PerceivedTree, RunnerTransportError> {
        let mut root: A11yNode = self.json_get("/tree", include).await?;
        derive_roles_recursive(&mut root);
        self.mark_a_one_app_root(&mut root);
        Ok(PerceivedTree {
            source: TreeSource::Accessibility,
            root,
        })
    }

    /// The app's own semantics tree and the package it is the tree of, or
    /// `None` when there is no probe. The package is the runner's answer
    /// when the caller named none — it picks the app holding the focus.
    ///
    /// Deliberately swallowing here, and only here: every reason this can
    /// fail — no probe, an older probe, a runner without the route — means
    /// the same thing to a caller, which is "carry on with the tree you
    /// have always had". What must NOT be swallowed is which tree was used,
    /// and that is the return value of the function above.
    async fn semantics_tree(&self, app: Option<&str>) -> Option<(A11yNode, Option<String>)> {
        // The parameter is carried only when there is one to carry. An
        // `app=` with nothing after it is not the same request: it names
        // the empty package, and the runner would have to decide what
        // that meant rather than being free to answer about the window
        // in front.
        let answered_for = self.probe_present(app).await?;
        let root = self.probe_tree(app).await?;
        Some((root, answered_for))
    }

    /// Whether the probe is there: `Some` with the package the runner
    /// answered for, `None` when there is no probe to ask. Swallowing for
    /// the reason [`Self::semantics_tree`] gives.
    async fn probe_present(&self, app: Option<&str>) -> Option<Option<String>> {
        let named = app.map(|a| format!("?app={a}")).unwrap_or_default();
        let raw: serde_json::Value = self.json_get(&format!("/probe{named}"), None).await.ok()?;
        if raw.get("present")?.as_bool() != Some(true) {
            return None;
        }
        Some(raw.get("app").and_then(|a| a.as_str()).map(str::to_owned))
    }

    /// The probe's tree, or `None` when it could not be had.
    async fn probe_tree(&self, app: Option<&str>) -> Option<A11yNode> {
        let named = app.map(|a| format!("?app={a}")).unwrap_or_default();
        let payload = self
            .json_get::<serde_json::Value>(&format!("/probe/tree{named}"), None)
            .await
            .ok()?;
        let mut root = smix_screen::probe_tree_to_a11y(&payload.to_string())?;
        derive_roles_recursive(&mut root);
        Some(root)
    }
}
