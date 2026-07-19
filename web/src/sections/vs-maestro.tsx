import { LINKS } from '../data/site'
import { VERB_SUBSET, VERB_TABLE_TOTAL } from '../data/verbs'

const TAG_LABEL: Record<string, string> = {
  'ai-tier': 'AI tier',
  native: 'native',
  idle: 'true idle',
}

// Beyond a plain verb port — smix's native surface. Each traces to a cited
// source (CLAUDE.md §9/§12, docs/v2.md, llms.txt).
const EXTENSIONS: { title: string; detail: string }[] = [
  {
    title: 'Three-layer core',
    detail: 'sense · decide · act as an architectural invariant, not a driver detail.',
  },
  {
    title: 'AI-readable failures',
    detail: 'Every miss carries visible elements and suggested fixes — structured for an agent.',
  },
  {
    title: 'OCR sensing',
    detail: 'Vision OCR as a second sense when the accessibility tree drops a label.',
  },
  {
    title: 'Fenced AI-assertion tier',
    detail: 'assertCondition / extractWithAI judge a screenshot via a local claude CLI — opt-in.',
  },
  {
    title: 'MCP driving surface',
    detail: 'Drive a live simulator from an agent over MCP, no yaml in between.',
  },
  {
    title: 'One wire client, four SDKs',
    detail: 'Swift and Kotlin drive over FFI on a single Rust wire client; TypeScript shares the model.',
  },
  {
    title: 'First-class sessions',
    detail: 'Sessions are explicit and enforced in v2 — no implicit global state.',
  },
  {
    title: 'iOS + Android parity',
    detail: 'Every verb states its iOS and Android status, gated against the verb table.',
  },
]

export function VsMaestro() {
  return (
    <section id="maestro" className="border-b border-border">
      <div className="mx-auto max-w-[1180px] px-6 py-16 sm:py-20">
        <p className="eyebrow mb-6">Why not maestro</p>
        <h2
          className="max-w-[22ch] font-mono font-500 text-fg"
          style={{ fontSize: 'clamp(26px, 3.6vw, 40px)', lineHeight: 1.1, letterSpacing: '-0.02em' }}
        >
          Every maestro verb, plus a native surface.
        </h2>
        <p className="mt-5 max-w-[62ch] text-fg-muted">
          smix runs maestro-compatible yaml directly — the same verbs your tests already use, each
          mapped to a smix-canonical form. Then it adds the surface maestro has no answer for.
        </p>

        <div className="mt-10 grid gap-8 lg:grid-cols-[1.1fr_1fr] lg:items-start">
          {/* Verb matrix — a representative subset. */}
          <div>
            <div className="mb-3 flex items-baseline justify-between">
              <span className="mono-label">verb map · subset</span>
              <a href={LINKS.llms} target="_blank" rel="noreferrer" className="link-accent font-mono text-[12px]">
                full table ({VERB_TABLE_TOTAL}) →
              </a>
            </div>
            <div className="overflow-x-auto border border-border">
              <table className="w-full border-collapse font-mono text-[13px]">
                <thead>
                  <tr className="border-b border-border bg-bg-inset text-left">
                    <th className="px-3 py-2 font-500 tracking-wide text-fg-muted uppercase">maestro</th>
                    <th className="px-3 py-2 font-500 tracking-wide text-fg-muted uppercase">smix</th>
                    <th className="px-3 py-2 font-500 tracking-wide text-fg-muted uppercase">category</th>
                  </tr>
                </thead>
                <tbody>
                  {VERB_SUBSET.map((v) => (
                    <tr key={v.maestro} className="border-b border-border last:border-0">
                      <td className="px-3 py-1.5 text-fg-muted">{v.maestro}</td>
                      <td className="px-3 py-1.5 text-fg">
                        {v.smix}
                        {v.tag ? (
                          <span
                            className="ml-2 align-middle font-mono text-[10px] tracking-wide text-accent uppercase"
                          >
                            {TAG_LABEL[v.tag]}
                          </span>
                        ) : null}
                      </td>
                      <td className="px-3 py-1.5 text-fg-dim">{v.category}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="mt-3 font-mono text-[11px] text-fg-dim">
              {VERB_SUBSET.length} of {VERB_TABLE_TOTAL} rows. maestro name → smix-canonical name,
              from smix_verbs::VERB_TABLE.
            </p>
          </div>

          {/* Extensions. */}
          <div>
            <span className="mono-label">beyond maestro</span>
            <ul className="mt-3 divide-y divide-border border border-border">
              {EXTENSIONS.map((e) => (
                <li key={e.title} className="flex gap-3 p-4">
                  <span aria-hidden className="mt-1.5 h-1.5 w-1.5 shrink-0 bg-accent" />
                  <div>
                    <span className="font-mono text-[13px] font-600 text-fg">{e.title}</span>
                    <p className="mt-1 text-[13px] leading-relaxed text-fg-muted">{e.detail}</p>
                  </div>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>
    </section>
  )
}
