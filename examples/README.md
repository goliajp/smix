# examples

Golden-path smix flows. Copy one, swap in your app's `appId` and testids,
and run it.

| file | what it shows |
|---|---|
| [`hello.yaml`](hello.yaml) | the minimal launch → tap → assert loop |

Parse-check any example without a device:

```bash
smix run --check examples/hello.yaml
```

Run against a booted simulator:

```bash
smix run --device <udid> examples/hello.yaml
```

For recipes (login, modals, deep links, OCR fallback, cross-platform),
see [`docs/ai-guide/08-cookbook.md`](../docs/ai-guide/08-cookbook.md).
