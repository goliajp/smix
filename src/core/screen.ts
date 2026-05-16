import type { A11yNode, ElementSummary } from './schemas.js'

export type { Rect, A11yNode, ElementSummary, ScreenDescription } from './schemas.js'

export function summarizeNode(node: A11yNode): ElementSummary {
  const summary: ElementSummary = {
    role: node.role ?? 'unknown',
    bounds: node.bounds,
    enabled: node.enabled,
  }
  const name = node.label ?? node.text ?? node.value
  if (name) summary.name = name
  if (node.identifier) summary.id = node.identifier
  if (node.text && node.text !== summary.name) summary.text = node.text
  return summary
}
