// @vitest-environment node
import { describe, it, expect } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';

const componentPath = path.resolve(
  __dirname,
  '../../components/flight-monitor/DispatchCollaborationChat.vue',
);

function readComponent(): string {
  return fs.readFileSync(componentPath, 'utf-8');
}

describe('DispatchCollaborationChat.vue delegates to useDispatchChat composable', () => {
  it('does not define a local fetchJson helper (data fetching lives in the composable)', () => {
    const source = readComponent();
    expect(source).not.toMatch(/\bconst\s+fetchJson\s*=\s*async/);
    expect(source).not.toMatch(/\bfunction\s+fetchJson\s*\(/);
  });

  it('does not own the SSE connect/disconnect/reconnect lifecycle', () => {
    const source = readComponent();
    expect(source).not.toMatch(/\bconst\s+connectStream\s*=/);
    expect(source).not.toMatch(/\bconst\s+disconnectStream\s*=/);
    expect(source).not.toMatch(/\bconst\s+scheduleReconnect\s*=/);
  });

  it('does not parse SSE frames locally (handleSsePayload belongs in the composable)', () => {
    const source = readComponent();
    expect(source).not.toMatch(/\bhandleSsePayload\s*\(/);
  });

  it('does not re-implement data-loading or sending logic', () => {
    const source = readComponent();
    // The composable owns /api/v2/dispatch/collaboration/... URL construction
    // and the fetch plumbing. The component only renders and wires props.
    expect(source).not.toMatch(/\/dispatch\/collaboration\/groups/);
    expect(source).not.toMatch(/api\.raw\(/);
    expect(source).not.toMatch(/apiBase\.value/);
  });
});
