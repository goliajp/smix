# smix-core performance budgets

`smix-core` is a workspace anchor + version sentinel. It exposes only
one `&'static str` constant (`__CRATE_VERSION`, doc-hidden) and pulls
in zero downstream stones. **No measurable hot path; no enforced
budget.**

If you arrived here looking for real perf numbers, jump to the
relevant leaf stone:

| You want… | See |
|---|---|
| A11y tree primitives | [`smix-screen` BUDGETS.md](../smix-screen/BUDGETS.md) |
| Selector match | [`smix-selector` BUDGETS.md](../smix-selector/BUDGETS.md) |
| Selector → node resolution | [`smix-selector-resolver` BUDGETS.md](../smix-selector-resolver/BUDGETS.md) |
| Selector → normalized coord | [`smix-host-coord-resolver` BUDGETS.md](../smix-host-coord-resolver/BUDGETS.md) |
| AI-readable failure render | [`smix-error` BUDGETS.md](../smix-error/BUDGETS.md) |
| Recorder IR accessors / sort / serde | [`smix-authoring-ir` BUDGETS.md](../smix-authoring-ir/BUDGETS.md) |
| Input enum primitives | [`smix-input` BUDGETS.md](../smix-input/BUDGETS.md) |
| Runner wire serde | [`smix-runner-wire` BUDGETS.md](../smix-runner-wire/BUDGETS.md) |
| `xcrun simctl` IO baseline | [`smix-simctl` BUDGETS.md](../smix-simctl/BUDGETS.md) |
