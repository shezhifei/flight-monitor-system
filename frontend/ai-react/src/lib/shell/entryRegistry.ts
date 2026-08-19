export type FrontendEntrySurface = 'page' | 'widget' | 'drawer' | 'modal';

export interface FrontendEntryDefinition {
  entryName: string;
  hostId: string;
  surface: FrontendEntrySurface;
}

export const FRONTEND_ENTRY_REGISTRY: Record<string, FrontendEntryDefinition> = {
  ai_monitor: {
    entryName: 'ai_monitor',
    hostId: 'ai-react-root',
    surface: 'page',
  },
  nl_query: {
    entryName: 'nl_query',
    hostId: 'ai-react-root',
    surface: 'page',
  },
  dispatch_board_ai: {
    entryName: 'dispatch_board_ai',
    hostId: 'dispatch-ai-root',
    surface: 'drawer',
  },
};

export function getFrontendEntryDefinition(entryName: string): FrontendEntryDefinition {
  const definition = FRONTEND_ENTRY_REGISTRY[entryName];
  if (!definition) {
    throw new Error(`Unknown frontend entry: ${entryName}`);
  }
  return definition;
}

/**
 * Playground capabilities (Task C5) an entry page can enable. The Vue host
 * shell (`AiReactEntryShell`) declares the subset it wants via the
 * `data-ai-features` attribute on the entry host element; pages read it
 * through `resolveEntryFeatures`. Legacy hosts without the attribute keep
 * every feature enabled (backward compatible with the static HTML pages).
 */
export const PLAYGROUND_FEATURES = [
  'plan-board',
  'subagent-tree',
  'run-resume',
  'compression-notice',
] as const;

export type PlaygroundFeature = (typeof PLAYGROUND_FEATURES)[number];

export function resolveEntryFeatures(entryName: string): Set<PlaygroundFeature> {
  const definition = getFrontendEntryDefinition(entryName);
  if (typeof document === 'undefined') {
    return new Set(PLAYGROUND_FEATURES);
  }
  const host = document.getElementById(definition.hostId);
  const raw = host?.getAttribute('data-ai-features');
  if (!raw) {
    return new Set(PLAYGROUND_FEATURES);
  }
  const enabled = new Set(
    raw
      .split(',')
      .map((item) => item.trim())
      .filter((item): item is PlaygroundFeature =>
        (PLAYGROUND_FEATURES as readonly string[]).includes(item),
      ),
  );
  return enabled;
}

