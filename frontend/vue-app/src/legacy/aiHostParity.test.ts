import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import type { AiEntryName } from './aiEntryLoader';

const __dirname = dirname(fileURLToPath(import.meta.url));
const vueAppRoot = resolve(__dirname, '../..');
const repoRoot = resolve(vueAppRoot, '../..');
const aiReactRoot = resolve(repoRoot, 'frontend/ai-react');

const ACTIVE_REACT_ENTRIES: Array<{
  name: AiEntryName;
  hostFile: string;
  hostId: string;
  hostMatcher: RegExp;
}> = [
  {
    name: 'ai_monitor',
    hostFile: 'src/pages/ai_monitor/AiMonitor.vue',
    hostId: 'ai-react-root',
    hostMatcher: /AiReactEntryShell[\s\S]*?ai_monitor/,
  },
  {
    name: 'llm_eval_lab',
    hostFile: 'src/pages/llm_eval_lab/LlmEvalLab.vue',
    hostId: 'ai-react-root',
    hostMatcher: /AiReactEntryShell[\s\S]*?llm_eval_lab/,
  },
  {
    name: 'nl_query',
    hostFile: 'src/pages/nl_query/NlQuery.vue',
    hostId: 'ai-react-root',
    hostMatcher: /AiReactEntryShell[\s\S]*?nl_query/,
  },
  {
    name: 'dashboard_ai_widget',
    hostFile: 'src/pages/dashboard/Dashboard.vue',
    hostId: 'dashboard-ai-widget-root',
    hostMatcher: /AiReactEntryShell[\s\S]*?dashboard_ai_widget/,
  },
  {
    name: 'dispatch_board_ai',
    hostFile: 'src/pages/dispatch_board/DispatchBoard.vue',
    hostId: 'dispatch-ai-root',
    hostMatcher: /AiReactEntryShell[\s\S]*?dispatch_board_ai/,
  },
];

const RETIRED_REACT_ENTRIES = [
  'ai_config_center',
  'flight_monitor_ai',
  'flowable_assistant_ai',
] as const;

describe('ai host parity', () => {
  it.each(ACTIVE_REACT_ENTRIES)(
    'active React entry %s is hosted by the matching Vue page through AiReactEntryShell',
    ({ hostFile, hostMatcher }) => {
      const fullPath = join(vueAppRoot, hostFile);
      expect(existsSync(fullPath)).toBe(true);
      const text = readFileSync(fullPath, 'utf8');
      expect(text).toMatch(hostMatcher);
    },
  );

  it.each(ACTIVE_REACT_ENTRIES)(
    'active React entry %s has its registry host id mapped by AiReactEntryShell',
    ({ name, hostId }) => {
      const shell = readFileSync(
        join(vueAppRoot, 'src/components/ai/AiReactEntryShell.vue'),
        'utf8',
      );
      expect(shell).toMatch(new RegExp(`${name}: ['"]${hostId}['"]`));
    },
  );

  it.each(ACTIVE_REACT_ENTRIES)(
    'active React entry %s has a matching .tsx file under ai-react/src/entries',
    ({ name }) => {
      const entryPath = join(aiReactRoot, 'src/entries', `${name}.tsx`);
      expect(existsSync(entryPath)).toBe(true);
    },
  );

  it.each(RETIRED_REACT_ENTRIES)(
    'retired React entry %s is absent from ai-react/src/entries',
    (name) => {
      const entryPath = join(aiReactRoot, 'src/entries', `${name}.tsx`);
      expect(existsSync(entryPath)).toBe(false);
    },
  );

  it('ai-react vite config input contains only active entries', () => {
    const viteConfig = readFileSync(join(aiReactRoot, 'vite.config.ts'), 'utf8');
    for (const { name } of ACTIVE_REACT_ENTRIES) {
      expect(viteConfig).toMatch(new RegExp(`\\b${name}\\b`));
    }
    for (const name of RETIRED_REACT_ENTRIES) {
      expect(viteConfig).not.toMatch(new RegExp(`\\b${name}\\b`));
    }
  });

  it('FRONTEND_ENTRY_REGISTRY contains only active entries', () => {
    const registry = readFileSync(
      join(aiReactRoot, 'src/lib/shell/entryRegistry.ts'),
      'utf8',
    );
    for (const { name } of ACTIVE_REACT_ENTRIES) {
      expect(registry).toMatch(new RegExp(`\\b${name}\\b`));
    }
    for (const name of RETIRED_REACT_ENTRIES) {
      expect(registry).not.toMatch(new RegExp(`\\b${name}\\b`));
    }
  });

  it('no Vue source file references a retired React entry id', () => {
    const offenders: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        if (name === 'node_modules' || name === 'dist' || name.startsWith('.')) continue;
        const full = join(dir, name);
        const st = statSync(full);
        if (st.isDirectory()) walk(full);
        else if (/\.(vue|ts)$/.test(name)) {
          // Skip this test file itself.
          if (full === __filename) continue;
          // The page parity matrix legitimately documents retired entries.
          if (full.endsWith('pageParityMatrix.ts')) continue;
          const text = readFileSync(full, 'utf8');
          for (const retired of RETIRED_REACT_ENTRIES) {
            // The AiConfigCenter.vue page-author TODO comment is allowed.
            if (retired === 'ai_config_center') continue;
            if (new RegExp(`['"]${retired}['"]`).test(text)) {
              offenders.push(`${full.replace(vueAppRoot, '')}: ${retired}`);
            }
          }
        }
      }
    };
    walk(join(vueAppRoot, 'src'));
    expect(offenders).toEqual([]);
  });
});
