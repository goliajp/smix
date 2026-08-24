# smix documentation

Everything here is written for someone **using** smix — an AI agent driving a
simulator, or a person writing flows and reading failures. Guides, reference,
cookbook, and the wire contracts an SDK implementor needs.

Design records, version plans, obtainability research, and internal ledgers are
**not** here, and are not in this repository at all — see [What is not in this
directory](#what-is-not-in-this-directory).

## Start here

| | |
|---|---|
| [`ai-guide/01-quickstart.md`](./ai-guide/01-quickstart.md) | First flow, end to end. |
| [`migrating-to-4.md`](./migrating-to-4.md) | Coming from 3.x. Device records moved to the machine; two SDK signatures changed. |
| [`migrating-to-8.md`](./migrating-to-8.md) | Coming from 7.x. Four Rust signatures, and two answers that changed: a port is checked against the device you named, and `inputText` no longer succeeds with nothing focused. |
| [`ai-guide/08-cookbook.md`](./ai-guide/08-cookbook.md) | Recipes for the situations that actually come up. |
| [`ai-guide/07-errors.md`](./ai-guide/07-errors.md) | What a failure means and what to change. |

## Reference

| | |
|---|---|
| [`ai-guide/02-yaml-reference.md`](./ai-guide/02-yaml-reference.md) | Every flow key. |
| [`ai-guide/03-selectors.md`](./ai-guide/03-selectors.md) | How to name an element, and which naming survives copy edits. |
| [`ai-guide/04-actions.md`](./ai-guide/04-actions.md) | Every action verb and its semantics. |
| [`ai-guide/05-cli.md`](./ai-guide/05-cli.md) | Every command and flag. |
| [`ai-guide/06-fixtures.md`](./ai-guide/06-fixtures.md) | The bundled fixture app to try things against. |
| [`ai-guide/09-sessions.md`](./ai-guide/09-sessions.md) | Session lifetime and reuse. |
| [`ai-guide/10-ai-assertions.md`](./ai-guide/10-ai-assertions.md) | Assertions that ask a model. |
| [`ai-guide/11-mcp.md`](./ai-guide/11-mcp.md) | Driving smix as an MCP server. |
| [`migrating-to-3.md`](./migrating-to-3.md) | Coming from 2.x. Three behaviours changed. |
| [`ai-guide/12-authoring.md`](./ai-guide/12-authoring.md) | Turning a manual session into a re-runnable flow. |

## Contracts (for SDK and integration authors)

| | |
|---|---|
| [`ai-guide/wire-format.md`](./ai-guide/wire-format.md) | Host↔runner wire shapes. |
| [`ai-guide/abi-stability.md`](./ai-guide/abi-stability.md) | Which crates are frozen and what a break costs. |
| [`ai-guide/verb-parity.md`](./ai-guide/verb-parity.md) | Verb coverage across platforms and against maestro. |
| [`ai-guide/activate-header-lifetime.md`](./ai-guide/activate-header-lifetime.md) | Per-request `--activate` / `--bundle-id` semantics. |
| [`ai-guide/schemas/`](./ai-guide/schemas/) | JSON schemas for machine-readable output. |

## What is not in this directory

smix keeps a substantial development record — a version boundary and decision
log, per-checkpoint plans, obtainability studies for capabilities that were
evaluated and rejected, perf decompositions, and internal defect and scope
ledgers. None of it is published, and none of it is in this repository.

That is deliberate rather than an omission. A reader who arrives to learn how
to write a flow should not have to walk past hundreds of pages of
work-in-progress to find the page that answers the question, and shelving both
together makes the published material harder to trust, because nothing tells
you which document is the current one.

So the rule here has one direction: **these guides never cite the development
record.** Where a guide needs a conclusion that was reached in it — why a
particular signal cannot be obtained, why an approach stays rejected — the
conclusion is written out here in full. You are not expected to go and read
something else, and there is nothing missing behind the sentence.
