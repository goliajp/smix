// A hand-curated, representative subset of the canonical VERB_TABLE
// (crates/smix-verbs/src/lib.rs). The full table is 47 entries — verified
// against llms.txt ("The canonical yaml verb table (47 entries)"). Every row
// below is copied verbatim from VERB_TABLE (maestro_name, smix_name,
// category). The full table lives in llms.txt.

export type VerbRow = {
  maestro: string
  smix: string
  category: string
  // Flag rows that are smix-native surface beyond a plain maestro port.
  tag?: 'ai-tier' | 'native' | 'idle'
}

export const VERB_SUBSET: VerbRow[] = [
  { maestro: 'tapOn', smix: 'tap', category: 'Tap' },
  { maestro: 'inputText', smix: 'fill', category: 'Input' },
  { maestro: 'eraseText', smix: 'clear', category: 'Input' },
  { maestro: 'assertVisible', smix: 'expect', category: 'Assert' },
  { maestro: 'assertNotVisible', smix: 'expectNotVisible', category: 'Assert' },
  { maestro: 'extendedWaitUntil', smix: 'expect', category: 'Assert' },
  { maestro: 'launchApp', smix: 'launchApp', category: 'Lifecycle' },
  { maestro: 'stopApp', smix: 'terminate', category: 'Lifecycle' },
  { maestro: 'clearState', smix: 'reset', category: 'Lifecycle' },
  { maestro: 'scroll', smix: 'scroll', category: 'Gesture' },
  { maestro: 'swipe', smix: 'swipe', category: 'Gesture' },
  { maestro: 'openLink', smix: 'openUrl', category: 'Device' },
  { maestro: 'runFlow', smix: 'runFlow', category: 'ControlFlow' },
  { maestro: 'assertWithAI', smix: 'assertCondition', category: 'Assert', tag: 'ai-tier' },
  { maestro: 'extractTextWithAI', smix: 'extractWithAI', category: 'Assert', tag: 'ai-tier' },
  { maestro: 'fixture', smix: 'fixture', category: 'SmixNative', tag: 'native' },
  {
    maestro: 'waitForAnimationToEnd',
    smix: 'waitForAnimationToEnd',
    category: 'Utility',
    tag: 'idle',
  },
]

export const VERB_TABLE_TOTAL = 47
