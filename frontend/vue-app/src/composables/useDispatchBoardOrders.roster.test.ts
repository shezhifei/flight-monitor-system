import { describe, expect, it } from 'vitest';
import { rosterTeamIds, rosterTeamLabel, type DispatchOrder } from './useDispatchBoardOrders';
import { doesItemMatchResourceFocus, type ResourceFocus } from './useDispatchBoardResources';

function order(partial: Partial<DispatchOrder>): DispatchOrder {
  return { order_id: 'o1', ...partial };
}

function teamFocus(resourceId: string, resourceLabel: string): ResourceFocus {
  return {
    resource_type: 'team',
    resource_id: resourceId,
    resource_label: resourceLabel,
    primary_resource_type: 'team',
    primary_resource_id: resourceId,
    target_view_mode: 'team',
    lane_id: '',
    primary_lane_id: '',
    resource_ids: [],
    lane_ids: [],
    highlight_scope: 'single',
    related_order_ids: [],
    source_panel: 'test',
    source_key: 'test',
    visible_resource_ids: [],
    missing_resource_ids: [],
  };
}

describe('roster team projection', () => {
  it('joins unique source team names from members and task_crew', () => {
    const item = order({
      members: [{ source_team_name: '地服一组', source_team_id: 't1' }],
      task_crew: { source_team_names: ['地服一组', '机务二组'], source_team_ids: ['t1', 't2'] },
    });
    expect(rosterTeamLabel(item)).toBe('地服一组 / 机务二组');
    expect(rosterTeamIds(item)).toEqual(['t1', 't2']);
  });

  it('matches team resource focus by member source_team_id, not missing order.team_id', () => {
    const item = order({
      members: [{ source_team_id: 't1', source_team_name: '地服一组' }],
    });
    expect(doesItemMatchResourceFocus(item, teamFocus('t1', '地服一组'))).toBe(true);
    expect(doesItemMatchResourceFocus(item, teamFocus('other', '别的班组'))).toBe(false);
  });
});
