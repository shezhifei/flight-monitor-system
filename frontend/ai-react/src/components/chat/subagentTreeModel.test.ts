import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { SubagentTree } from '@/components/chat/SubagentTree';
import {
  applyDelegateToolEvent,
  applySubagentStreamEvent,
  type SubagentNodeModel,
} from '@/components/chat/subagentTreeModel';

describe('applyDelegateToolEvent', () => {
  it('ignores non-delegation tools', () => {
    expect(
      applyDelegateToolEvent([], { toolName: 'update_plan', phase: 'call' }),
    ).toBeUndefined();
  });

  it('adds a running proposal_only node on delegate_to_subagent call', () => {
    const nodes = applyDelegateToolEvent([], {
      toolName: 'delegate_to_subagent',
      phase: 'call',
      args: { entity_id: 'dispatch_ops', task: 'replan' },
    });
    expect(nodes).toHaveLength(1);
    expect(nodes?.[0]).toMatchObject({
      label: 'dispatch_ops',
      status: 'running',
      proposalOnly: true,
      toolCalls: 0,
    });
  });

  it('resolves the running node on tool result', () => {
    let nodes = applyDelegateToolEvent([], {
      toolName: 'delegate_to_subagent',
      phase: 'call',
      args: { entity_id: 'dispatch_ops' },
    }) as SubagentNodeModel[];
    nodes = applyDelegateToolEvent(nodes, {
      toolName: 'delegate_to_subagent',
      phase: 'result',
      status: 'succeeded',
    }) as SubagentNodeModel[];
    expect(nodes[0].status).toBe('done');

    nodes = applyDelegateToolEvent(nodes, {
      toolName: 'delegate_to_subagent',
      phase: 'call',
      args: { entity_id: 'anomaly_ops' },
    }) as SubagentNodeModel[];
    nodes = applyDelegateToolEvent(nodes, {
      toolName: 'delegate_to_subagent',
      phase: 'result',
      status: 'failed',
    }) as SubagentNodeModel[];
    expect(nodes[1].status).toBe('error');
  });
});

describe('applySubagentStreamEvent', () => {
  it('creates an implicit node from a bubbled event when no delegate call was seen', () => {
    const nodes = applySubagentStreamEvent([], {
      run_id: 'run-parent',
      parent_run_id: 'run-parent',
      event_type: 'tool_call',
      subagent_depth: 1,
      tool_name: 'flight_status_lookup',
      tool_type: 'read_only',
    });
    expect(nodes).toHaveLength(1);
    expect(nodes[0]).toMatchObject({
      parentRunId: 'run-parent',
      depth: 1,
      status: 'running',
      proposalOnly: true,
      toolCalls: 1,
      lastActivity: 'flight_status_lookup',
    });
  });

  it('attaches bubbled events to the running node and completes it', () => {
    let nodes = applyDelegateToolEvent([], {
      toolName: 'delegate_to_subagent',
      phase: 'call',
      args: { entity_id: 'dispatch_ops' },
    }) as SubagentNodeModel[];

    nodes = applySubagentStreamEvent(nodes, {
      run_id: 'run-parent',
      event_type: 'tool_call',
      subagent_depth: 2,
      tool_name: 'add_flight_note',
      tool_type: 'write_action',
    });
    expect(nodes[0].toolCalls).toBe(1);
    expect(nodes[0].depth).toBe(2);
    expect(nodes[0].proposalOnly).toBe(true);

    nodes = applySubagentStreamEvent(nodes, {
      run_id: 'run-parent',
      event_type: 'completed',
      subagent_depth: 2,
    });
    expect(nodes[0].status).toBe('done');
  });

  it('marks the node error on child error events', () => {
    const nodes = applySubagentStreamEvent([], {
      run_id: 'run-parent',
      parent_run_id: 'run-parent',
      event_type: 'error',
      subagent_depth: 1,
    });
    expect(nodes[0].status).toBe('error');
  });
});

describe('SubagentTree component', () => {
  it('renders nodes with status and proposal_only marker', () => {
    const nodes: SubagentNodeModel[] = [
      {
        id: 'sub_1',
        depth: 1,
        label: 'dispatch_ops',
        status: 'running',
        proposalOnly: true,
        toolCalls: 2,
        lastActivity: 'flight_status_lookup',
      },
      {
        id: 'sub_2',
        depth: 2,
        label: 'anomaly_ops',
        status: 'done',
        proposalOnly: true,
        toolCalls: 0,
      },
    ];
    const html = renderToStaticMarkup(createElement(SubagentTree, { nodes }));
    expect(html).toContain('subagent-tree');
    expect(html).toContain('dispatch_ops');
    expect(html).toContain('anomaly_ops');
    expect(html).toContain('proposal_only');
    expect(html).toContain('运行中');
    expect(html).toContain('已完成');
  });

  it('renders an empty state when no nodes exist', () => {
    const html = renderToStaticMarkup(createElement(SubagentTree, { nodes: [] }));
    expect(html).toContain('暂无子代理');
  });
});
