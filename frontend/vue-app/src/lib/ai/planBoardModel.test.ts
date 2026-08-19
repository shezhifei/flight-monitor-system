// 搬运自 frontend/ai-react/src/components/chat/planBoardModel.test.ts。
// 适配点：源文件末尾的 'PlanBoard component' describe 渲染的是 PlanBoard.tsx
// （React 组件，不在搬运范围），已剔除；其余用例不变，仅改 import 路径。
import { describe, expect, it } from 'vitest';

import {
  applyPlanToolEvent,
  planIncompleteCount,
  type PlanBoardModel,
} from './planBoardModel';

describe('applyPlanToolEvent', () => {
  it('ignores non-plan tool events', () => {
    expect(
      applyPlanToolEvent(null, { toolName: 'flight_status_lookup', phase: 'call' }),
    ).toBeUndefined();
  });

  it('creates a board from update_plan call arguments', () => {
    const next = applyPlanToolEvent(null, {
      toolName: 'update_plan',
      phase: 'call',
      args: {
        plan_description: '调查延误并通知相关团队',
        steps: [
          { id: 's1', description: '查询航班状态', assigned_to: 'llm' },
          { id: 's2', description: '生成换机建议', assigned_to: 'subagent' },
        ],
      },
    });

    expect(next).not.toBeNull();
    expect(next?.description).toBe('调查延误并通知相关团队');
    expect(next?.steps.map((step) => step.id)).toEqual(['s1', 's2']);
    // first pending step is promoted to in_progress once the plan is set
    expect(next?.steps[0].status).toBe('in_progress');
    expect(next?.steps[1].status).toBe('pending');
    expect(next?.steps[1].assignedTo).toBe('subagent');
  });

  it('parses JSON-string arguments', () => {
    const next = applyPlanToolEvent(null, {
      toolName: 'update_plan',
      phase: 'call',
      args: JSON.stringify({
        plan_description: 'p',
        steps: [{ id: 'a', description: 'step a' }],
      }),
    });
    expect(next?.steps).toHaveLength(1);
  });

  it('marks a step in_progress on complete_plan_step call and done on success result', () => {
    let board = applyPlanToolEvent(null, {
      toolName: 'update_plan',
      phase: 'call',
      args: {
        plan_description: 'p',
        steps: [
          { id: 's1', description: 'one' },
          { id: 's2', description: 'two' },
        ],
      },
    }) as PlanBoardModel;

    board = applyPlanToolEvent(board, {
      toolName: 'complete_plan_step',
      phase: 'call',
      args: { step_id: 's1' },
    }) as PlanBoardModel;
    expect(board.steps[0].status).toBe('in_progress');

    board = applyPlanToolEvent(board, {
      toolName: 'complete_plan_step',
      phase: 'result',
      status: 'succeeded',
      args: { step_id: 's1' },
    }) as PlanBoardModel;
    expect(board.steps[0].status).toBe('done');
    // completing s1 promotes the next pending step
    expect(board.steps[1].status).toBe('in_progress');
    expect(planIncompleteCount(board)).toBe(1);
  });

  it('marks a step blocked on failed complete_plan_step result', () => {
    let board = applyPlanToolEvent(null, {
      toolName: 'update_plan',
      phase: 'call',
      args: { plan_description: 'p', steps: [{ id: 's1', description: 'one' }] },
    }) as PlanBoardModel;

    board = applyPlanToolEvent(board, {
      toolName: 'complete_plan_step',
      phase: 'result',
      status: 'failed',
      args: { step_id: 's1' },
    }) as PlanBoardModel;
    expect(board.steps[0].status).toBe('blocked');
  });

  it('replaces steps from list_plan_steps result payload', () => {
    const board = applyPlanToolEvent(null, {
      toolName: 'list_plan_steps',
      phase: 'result',
      result: {
        steps: [
          { id: 's1', description: 'one', status: 'completed' },
          { id: 's2', description: 'two', status: 'pending' },
        ],
      },
    }) as PlanBoardModel;
    expect(board.steps[0].status).toBe('done');
    expect(board.steps[1].status).toBe('pending');
  });
});
