# Migrating to smix 3.0

Two things in 3.0 change what code you already have does. Everything
else is additive, and this page is only about the two.

Read it if you have flows, scripts, or an SDK integration written
against 2.x. If you drive smix by hand, `fill` behaves the way its name
always claimed and there is nothing to do.

---

## 1. `fill` replaces the field it names

**Before**: filling a field typed on the end of whatever was already
there. Returning to a form and filling the same field again left both
values concatenated.

**Now**: **you can only replace a field you named.**

| what you write | 2.x | 3.0 |
|---|---|---|
| `fill(id:email)` / `inputText: {id, text}` | appends | **replaces** |
| `inputText: "text"` (scalar, no field named) | appends | appends |
| `pasteText` | appends | appends |
| `App::fill(&focused(), …)` | appends | appends |

Typing into whatever holds focus still appends, because there is no
named field to empty — and that is also what maestro's verbs of that
shape do, so a flow ported from maestro still means what it meant.

### What to check in your flows

Search for a field filled twice without a clear between:

```bash
grep -n -A3 'inputText:' your-flows/*.yaml | grep -B1 'id:'
```

Three shapes, and what to do with each:

- **Filled once.** Nothing to do — the field was empty, and emptying an
  empty field changes nothing.
- **`eraseText` then `inputText: {id, …}`.** Still correct. The
  `eraseText` is now redundant, not wrong; leaving it costs a round
  trip.
- **Two `inputText: {id, …}` on the same field, expecting the values to
  join.** This is the one that changes. Rewrite it as one step with the
  whole value, or tap the field and use the scalar form for the second
  part:

  ```yaml
  # 2.x: relied on appending
  - inputText: { id: "search", text: "hello " }
  - inputText: { id: "search", text: "world" }

  # 3.0: say what the field should hold
  - inputText: { id: "search", text: "hello world" }

  # 3.0: or keep the two steps, appending deliberately
  - tapOn: { id: "search" }
  - inputText: "hello "
  - inputText: "world"
  ```

### Why the default flipped rather than gaining a flag

The guides have described this verb as replacing since it existed
("Fill — replaces focused field content"). The implementation appended.
In a password field the difference is invisible — the dots look right —
so it surfaces as a login rejecting a correct password, which is what it
cost the person who reported it.

A flag would have left that bug in place for everyone who did not know
to set it. The wire carries `clearFirst` on `POST /fill` and it defaults
to true; a runner too old to know the field appends, which is what it
did before, so the field is additive on the wire.

---

## 2. `describe` and `tree` leave out the keyboard's keys

**Before**: every key of the software keyboard appeared as its own
element — a summary per letter, plus `Next keyboard`, `Dictate`, shift
and delete. Around sixty of them, the same sixty on every screen of
every app.

**Now**: `describe` never enumerates them. `tree` collapses them and
prints how many it left out; `smix tree --keyboard` includes them.

**The keyboard element itself still appears.** A keyboard covering the
thing you wanted to tap is the explanation for a failure, and hiding it
would turn a legible failure into a mystery. Only the keys go.

### What to check

- **Counting elements from `describe --json`.** The count drops on any
  screen with the keyboard up. If you were using it as a fingerprint,
  it is a different fingerprint now.
- **Selecting a key by label** (`text:a`, `label:return`). Use
  `pressKey` / `smix press-key`, which names keys directly and does not
  depend on the tree at all.
- **A tree snapshot committed as a fixture.** Regenerate it, or pass
  `--keyboard` to keep the old shape.

---

## Rust API

Two crates changed shape. This affects you only if you depend on them
directly — `smix-sdk`'s `App` is unchanged.

```rust
// smix-driver: the Driver trait
async fn fill(&self, selector: &Selector, text: &str,
              include: Option<IncludeScope>,
              clear_first: bool) -> Result<(), ExpectationFailure>;

// smix-runner-client
pub async fn fill(&self, selector: &Selector, text: &str,
                  include: Option<IncludeScope>,
                  clear_first: bool) -> Result<RunnerKeyboardResult, RunnerTransportError>;
```

Pass `true` for the 3.0 behaviour. `App::fill` derives it from whether
the selector names a field, which is the rule above expressed once.

`smix-runner-client` also gained `clear_text()` for the Android runner's
`POST /clear-text`, and `KNOWN_UNAVAILABLE_CATEGORIES`, which now
includes `not-running`.
