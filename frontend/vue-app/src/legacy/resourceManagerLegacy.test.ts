import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
// @ts-expect-error jsdom is installed for Vitest, but this project does not ship @types/jsdom.
import { JSDOM } from 'jsdom';
import { describe, expect, it, vi } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const resourceManagerPath = resolve(
  __dirname,
  '../../../backup/legacy-frontend-archive/html/resource_manager.html',
);

interface LegacyWindow {
  document: Document;
  Auth?: {
    requireAuthAsync: () => Promise<boolean>;
    fetch?: typeof fetch;
  };
  eval: (code: string) => void;
  showCreateTeamTypeModal?: () => void;
  showCreateEquipTypeModal?: () => void;
  saveTeamType?: () => Promise<void>;
  saveEquipmentType?: () => Promise<void>;
  [key: string]: unknown;
}

function loadResourceManagerScript() {
  const html = readFileSync(resourceManagerPath, 'utf8');
  const dom = new JSDOM(html, {
    url: 'http://localhost/frontend/html/resource_manager.html',
    runScripts: 'outside-only',
  });
  const win = dom.window as unknown as LegacyWindow;
  const script = Array.from(win.document.querySelectorAll('script'))
    .find((node: Element) => String(node.textContent || '').includes('function showCreateTeamTypeModal')) as
    | { textContent?: string }
    | undefined;

  if (!script?.textContent) {
    throw new Error('Missing resource manager inline script');
  }

  win.Auth = {
    requireAuthAsync: () => Promise.resolve(false),
  };
  win.eval(script.textContent);
  return dom;
}

type FetchMock = ReturnType<typeof vi.fn<(url: string, options?: RequestInit) => Promise<{ ok: boolean; status: number; json: () => Promise<unknown> }>>>;

describe('legacy resource manager type creation', () => {
  it('creates a team type from the legacy modal', async () => {
    const dom = loadResourceManagerScript();
    const win = dom.window as unknown as LegacyWindow;
    const fetchMock = vi.fn(async (url: string, options: RequestInit = {}) => {
      if (String(url).endsWith('/team-types') && options.method === 'POST') {
        return {
          ok: true,
          status: 201,
          json: () => Promise.resolve({
            success: true,
            data: { id: 'tt_1', name: '机务班组', code: 'MX', color: '#007AFF', task_types: ['boarding'] },
          }),
        };
      }

      if (String(url).endsWith('/team-types')) {
        return {
          ok: true,
          status: 200,
          json: () => Promise.resolve([
            { id: 'tt_1', name: '机务班组', code: 'MX', color: '#007AFF', task_types: ['boarding'] },
          ]),
        };
      }

      throw new Error(`Unexpected request: ${url}`);
    }) as FetchMock;

    win.showCreateTeamTypeModal?.();
    (win.document.getElementById('team-type-name') as HTMLInputElement).value = '机务班组';
    (win.document.getElementById('team-type-code') as HTMLInputElement).value = 'MX';
    (win.document.getElementById('team-type-color') as HTMLInputElement).value = '#007AFF';
    (win.document.getElementById('team-type-task-types') as HTMLInputElement).value = 'boarding, cleaning';
    (win.document.getElementById('team-type-description') as HTMLTextAreaElement).value = '负责航前保障';
    if (win.Auth) win.Auth.fetch = fetchMock as unknown as typeof fetch;

    await win.saveTeamType?.();

    expect(fetchMock).toHaveBeenCalledWith('/api/v2/dispatch/team-types', expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({
        name: '机务班组',
        code: 'MX',
        color: '#007aff',
        task_types: ['boarding', 'cleaning'],
        description: '负责航前保障',
        is_driver_type: false,
      }),
    }));
    expect(win.document.getElementById('team-type-modal')?.classList.contains('show')).toBe(false);
    expect((win.document.getElementById('team-type-id') as HTMLSelectElement).options).toHaveLength(2);
  });

  it('creates an equipment type from the legacy modal', async () => {
    const dom = loadResourceManagerScript();
    const win = dom.window as unknown as LegacyWindow;
    const fetchMock = vi.fn(async (url: string, options: RequestInit = {}) => {
      if (String(url).endsWith('/equipment-types') && options.method === 'POST') {
        return {
          ok: true,
          status: 201,
          json: () => Promise.resolve({
            success: true,
            data: { id: 'et_1', name: '牵引车', code: 'TUG', category: 'vehicle', requires_driver: true },
          }),
        };
      }

      if (String(url).endsWith('/equipment-types')) {
        return {
          ok: true,
          status: 200,
          json: () => Promise.resolve([
            { id: 'et_1', name: '牵引车', code: 'TUG', category: 'vehicle', requires_driver: true },
          ]),
        };
      }

      throw new Error(`Unexpected request: ${url}`);
    }) as FetchMock;

    win.showCreateEquipTypeModal?.();
    (win.document.getElementById('equip-type-name') as HTMLInputElement).value = '牵引车';
    (win.document.getElementById('equip-type-code') as HTMLInputElement).value = 'TUG';
    (win.document.getElementById('equip-type-category') as HTMLSelectElement).value = 'vehicle';
    (win.document.getElementById('equip-type-requires-driver') as HTMLInputElement).checked = true;
    (win.document.getElementById('equip-type-description') as HTMLTextAreaElement).value = '用于航空器牵引';
    if (win.Auth) win.Auth.fetch = fetchMock as unknown as typeof fetch;

    await win.saveEquipmentType?.();

    expect(fetchMock).toHaveBeenCalledWith('/api/v2/dispatch/equipment-types', expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({
        name: '牵引车',
        code: 'TUG',
        category: 'vehicle',
        requires_driver: true,
        driver_team_type_id: null,
        icon: null,
        description: '用于航空器牵引',
      }),
    }));
    expect(win.document.getElementById('equip-type-modal')?.classList.contains('show')).toBe(false);
    expect((win.document.getElementById('equipment-type-id') as HTMLSelectElement).options).toHaveLength(2);
  });
});
