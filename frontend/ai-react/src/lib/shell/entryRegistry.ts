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
  llm_eval_lab: {
    entryName: 'llm_eval_lab',
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
  dashboard_ai_widget: {
    entryName: 'dashboard_ai_widget',
    hostId: 'dashboard-ai-widget-root',
    surface: 'widget',
  },
};

export function getFrontendEntryDefinition(entryName: string): FrontendEntryDefinition {
  const definition = FRONTEND_ENTRY_REGISTRY[entryName];
  if (!definition) {
    throw new Error(`Unknown frontend entry: ${entryName}`);
  }
  return definition;
}

