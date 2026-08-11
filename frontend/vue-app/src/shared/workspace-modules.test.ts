// @vitest-environment node
import { describe, expect, it } from 'vitest';
import {
  isWorkspaceModuleId,
  resolveWorkspaceModuleFromDashboard,
  workspaceEmbedSrc,
  workspaceOpenUrl,
  WORKSPACE_MODULES,
} from './workspace-modules';

describe('workspace-modules', () => {
  it('catalog covers flight monitor as pinned module', () => {
    const flight = WORKSPACE_MODULES.find((m) => m.id === 'flight_monitor');
    expect(flight?.pinned).toBe(true);
  });

  it('accepts valid module ids', () => {
    expect(isWorkspaceModuleId('flight_monitor')).toBe(true);
    expect(isWorkspaceModuleId('login')).toBe(false);
    expect(isWorkspaceModuleId('dashboard')).toBe(false);
  });

  it('maps dashboard aliases', () => {
    expect(resolveWorkspaceModuleFromDashboard('dispatch')).toBe('dispatch_board');
    expect(resolveWorkspaceModuleFromDashboard('anomaly')).toBe('anomaly_monitor');
    expect(resolveWorkspaceModuleFromDashboard('kpi')).toBe('kpi_dashboard');
  });

  it('builds workspace and embed urls', () => {
    expect(workspaceOpenUrl('dispatch_board')).toBe('/frontend/workspace.html?tab=dispatch_board');
    expect(workspaceEmbedSrc('flight_monitor')).toContain('embed=1');
    expect(workspaceEmbedSrc('flight_monitor')).toContain('/frontend/flight_monitor.html');
  });
});
