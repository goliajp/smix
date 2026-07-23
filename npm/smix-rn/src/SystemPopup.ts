// A system popup as the runner reports it — the TS mirror of
// `smix_runner_wire::SystemPopup`. `App.systemPopups()` returns these
// verbatim from the wire; it is NOT an A11yNode (a popup has `id` / `type` /
// `source`, not accessibility geometry), so it carries its own type rather
// than being cast to one.

export interface SystemPopupButton {
  readonly id: string
  readonly label: string
  /** Semantic role, e.g. "cancel" / "destructive" / "default". */
  readonly role: string
  readonly destructive: boolean
}

export interface SystemPopup {
  readonly id: string
  /** Discriminator, e.g. "alert" / "sheet" / "banner". */
  readonly type: string
  /** Originator, e.g. a bundle id or system framework name. */
  readonly source: string
  readonly title: string
  readonly body: string
  readonly buttons: readonly SystemPopupButton[]
}
