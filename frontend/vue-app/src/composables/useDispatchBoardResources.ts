import { useApi } from '@/composables/useApi';
import type { DispatchOrder, TimelineMember } from './useDispatchBoardOrders';
import type { ViewMode, TimelineLane } from './useDispatchBoardGantt';

export interface ResourceFocus {
  resource_type: 'team' | 'employee';
  resource_id: string;
  resource_label: string;
  primary_resource_type: 'team' | 'employee';
  primary_resource_id: string;
  target_view_mode: ViewMode;
  lane_id: string;
  primary_lane_id: string;
  resource_ids: string[];
  lane_ids: string[];
  highlight_scope: 'single' | 'crew';
  related_order_ids: string[];
  source_panel: string;
  source_key: string;
  visible_resource_ids: string[];
  missing_resource_ids: string[];
  member_change_summary?: MemberChangeSummary | null;
}


export interface MemberChangeSummary {
  replaced_members: ReplaceMemberRecord[];
  added_members: MemberRecord[];
  removed_members: MemberRecord[];
  unchanged_members: MemberRecord[];
  changed_member_count: number;
}


export interface MemberRecord {
  slot_code: string;
  member: {
    user_id: string;
    username: string;
    slot_code: string;
    qualification_code: string;
    qualification_level_code: string;
  } | null;
}


export interface ReplaceMemberRecord {
  slot_code: string;
  before: {
    user_id: string;
    username: string;
    slot_code: string;
    qualification_code: string;
    qualification_level_code: string;
  } | null;
  after: {
    user_id: string;
    username: string;
    slot_code: string;
    qualification_code: string;
    qualification_level_code: string;
  } | null;
}


export const RESOURCE_FOCUS_PANEL_LABELS: Record<string, string> = Object.freeze({
  analytics: '运营分析',
  conflict: '冲突治理',
  scenario: '场景预览',
  replan: '冲突重排',
});


export function normalizeTimelineMemberUserId(member: TimelineMember | null | undefined): string {
  return String(member?.user_id ?? member?.id ?? '').trim();
}

/** Normalise a member's display name. */

export function normalizeTimelineMemberName(member: TimelineMember | null | undefined): string {
  return String(
    member?.username ?? member?.user_display_name ?? member?.name ?? normalizeTimelineMemberUserId(member),
  ).trim();
}

/** Normalise resource type string to canonical form. */

export function normalizeResourceType(resourceType: unknown): 'team' | 'employee' | '' {
  const normalized = String(resourceType ?? '').trim().toLowerCase();
  if (normalized === 'team') return 'team';
  if (normalized === 'employee' || normalized === 'individual' || normalized === 'user') return 'employee';
  return '';
}

/** Derive view mode from resource type. */

export function getResourceFocusViewMode(resourceType: unknown): ViewMode {
  return normalizeResourceType(resourceType) === 'employee' ? 'employee' : 'team';
}

/** Normalise crew members from raw array. */

export function normalizeTaskCrewMembers(rawMembers: unknown): ReadonlyArray<TimelineMember> {
  return (Array.isArray(rawMembers) ? rawMembers : [])
    .map((member: Record<string, unknown>) => ({
      user_id: String(member?.user_id ?? '').trim(),
      username: String(member?.username ?? '').trim(),
      slot_code: String(member?.slot_code ?? '').trim(),
      qualification_code: String(member?.qualification_code ?? '').trim(),
      qualification_level_code: String(member?.qualification_level_code ?? '').trim(),
    }))
    .filter((member) => member.user_id || member.username);
}

/** Normalise member change summary from API payload. */

export function normalizeMemberChangeSummary(summary: unknown): MemberChangeSummary {
  if (!summary || typeof summary !== 'object') {
    return {
      replaced_members: [],
      added_members: [],
      removed_members: [],
      unchanged_members: [],
      changed_member_count: 0,
    };
  }

  const s = summary as Record<string, unknown>;

  const normalizeMemberRecord = (item: unknown, memberKey: string): MemberRecord | null => {
    if (!item || typeof item !== 'object') return null;
    const rec = item as Record<string, unknown>;
    const slotCode = String(rec.slot_code ?? '').trim();
    const member = rec[memberKey];
    const normalizedMember = member && typeof member === 'object'
      ? {
          user_id: String((member as Record<string, unknown>).user_id ?? '').trim(),
          username: String((member as Record<string, unknown>).username ?? '').trim(),
          slot_code: String((member as Record<string, unknown>).slot_code ?? slotCode ?? '').trim(),
          qualification_code: String((member as Record<string, unknown>).qualification_code ?? '').trim(),
          qualification_level_code: String((member as Record<string, unknown>).qualification_level_code ?? '').trim(),
        }
      : null;
    return { slot_code: slotCode, member: normalizedMember };
  };

  const normalizeReplaceRecord = (item: unknown): ReplaceMemberRecord | null => {
    if (!item || typeof item !== 'object') return null;
    const rec = item as Record<string, unknown>;
    const slotCode = String(rec.slot_code ?? '').trim();

    const normalizeBeforeAfter = (obj: unknown): ReplaceMemberRecord['before'] => {
      if (!obj || typeof obj !== 'object') return null;
      const o = obj as Record<string, unknown>;
      return {
        user_id: String(o.user_id ?? '').trim(),
        username: String(o.username ?? '').trim(),
        slot_code: String(o.slot_code ?? slotCode ?? '').trim(),
        qualification_code: String(o.qualification_code ?? '').trim(),
        qualification_level_code: String(o.qualification_level_code ?? '').trim(),
      };
    };

    return {
      slot_code: slotCode,
      before: normalizeBeforeAfter(rec.before),
      after: normalizeBeforeAfter(rec.after),
    };
  };

  const replacedMembers = (Array.isArray(s.replaced_members) ? s.replaced_members : [])
    .map(normalizeReplaceRecord)
    .filter(Boolean) as ReplaceMemberRecord[];
  const addedMembers = (Array.isArray(s.added_members) ? s.added_members : [])
    .map((item) => normalizeMemberRecord(item, 'member'))
    .filter(Boolean) as MemberRecord[];
  const removedMembers = (Array.isArray(s.removed_members) ? s.removed_members : [])
    .map((item) => normalizeMemberRecord(item, 'member'))
    .filter(Boolean) as MemberRecord[];
  const unchangedMembers = (Array.isArray(s.unchanged_members) ? s.unchanged_members : [])
    .map((item) => normalizeMemberRecord(item, 'member'))
    .filter(Boolean) as MemberRecord[];

  return {
    replaced_members: replacedMembers,
    added_members: addedMembers,
    removed_members: removedMembers,
    unchanged_members: unchangedMembers,
    changed_member_count:
      Number(s.changed_member_count ?? (replacedMembers.length + addedMembers.length + removedMembers.length)) || 0,
  };
}

/** Collect prioritised crew members for resource focus from a payload. */

export function collectCrewFocusMembers(payload: Record<string, unknown> = {}): {
  members: Array<{
    resource_type: string;
    resource_id: string;
    resource_label: string;
    slot_code: string;
    qualification_code: string;
    qualification_level_code: string;
  }>;
  member_change_summary: MemberChangeSummary;
} {
  const memberChangeSummary = normalizeMemberChangeSummary(
    payload.member_change_summary ?? payload.memberChangeSummary,
  );
  const taskCrew = payload.task_crew ?? payload.taskCrew ?? {};
  const taskCrewMembers = normalizeTaskCrewMembers(
    (taskCrew as Record<string, unknown>)?.members,
  );

  const prioritized: Array<{
    resource_type: string;
    resource_id: string;
    resource_label: string;
    slot_code: string;
    qualification_code: string;
    qualification_level_code: string;
  }> = [];
  const seen = new Set<string>();

  const pushMember = (member: TimelineMember | null, resourceType = 'employee') => {
    if (!member) return;
    const userId = String(member.user_id ?? '').trim();
    const username = String(member.username ?? '').trim();
    if (!userId && !username) return;
    const key = `${resourceType}:${userId || username}`;
    if (seen.has(key)) return;
    seen.add(key);
    prioritized.push({
      resource_type: resourceType,
      resource_id: userId,
      resource_label: username || userId,
      slot_code: String(member.slot_code ?? '').trim(),
      qualification_code: String(member.qualification_code ?? '').trim(),
      qualification_level_code: String(member.qualification_level_code ?? '').trim(),
    });
  };

  if (memberChangeSummary.replaced_members[0]?.after) {
    pushMember(memberChangeSummary.replaced_members[0].after);
  }
  if (memberChangeSummary.replaced_members[0]?.before) {
    pushMember(memberChangeSummary.replaced_members[0].before);
  }
  if (memberChangeSummary.added_members[0]?.member) {
    pushMember(memberChangeSummary.added_members[0].member);
  }
  taskCrewMembers.forEach((member) => pushMember(member));
  memberChangeSummary.added_members.forEach((item) => pushMember(item.member));
  memberChangeSummary.replaced_members.forEach((item) => {
    pushMember(item.after);
    pushMember(item.before);
  });
  memberChangeSummary.removed_members.forEach((item) => pushMember(item.member));

  if (prioritized.length === 0) {
    const individualUserId = String(payload.individual_user_id ?? payload.individualUserId ?? '').trim();
    const individualUsername = String(payload.individual_username ?? payload.individualUsername ?? '').trim();
    if (individualUserId || individualUsername) {
      pushMember({ user_id: individualUserId, username: individualUsername });
    }
  }

  return { members: prioritized, member_change_summary: memberChangeSummary };
}

/** Build a ResourceFocus object from a payload. */

export function buildResourceFocus(payload: Record<string, unknown> = {}): ResourceFocus | null {
  const resourceType = normalizeResourceType(payload.resource_type ?? payload.resourceType);
  if (!resourceType) return null;

  const resourceId = String(payload.resource_id ?? payload.resourceId ?? '').trim();
  const resourceLabel = String(payload.resource_label ?? payload.resourceLabel ?? '').trim();
  if (!resourceId && !resourceLabel) return null;

  const targetViewMode = String(
    payload.target_view_mode ?? payload.targetViewMode ?? getResourceFocusViewMode(resourceType),
  ).trim() || getResourceFocusViewMode(resourceType);

  const sourceKey = String(payload.source_key ?? payload.sourceKey ?? '').trim() || resourceId || resourceLabel;
  const primaryResourceType = normalizeResourceType(
    payload.primary_resource_type ?? payload.primaryResourceType ?? resourceType,
  ) || resourceType;
  const primaryResourceId = String(
    payload.primary_resource_id ?? payload.primaryResourceId ?? resourceId,
  ).trim();

  const normalizeIds = (raw: unknown, fallback: string): string[] => {
    const values = Array.isArray(raw) ? raw : [];
    const normalized = values.map((item) => String(item ?? '').trim()).filter(Boolean);
    const fb = String(fallback ?? '').trim();
    if (fb) normalized.unshift(fb);
    return Array.from(new Set(normalized));
  };

  const resourceIds = normalizeIds(payload.resource_ids ?? payload.resourceIds, primaryResourceId || resourceId);

  const highlightScope = String(payload.highlight_scope ?? payload.highlightScope ?? (resourceIds.length > 1 ? 'crew' : 'single')).trim() === 'crew'
    ? 'crew'
    : 'single';

  const memberChangeSummary = normalizeMemberChangeSummary(
    payload.member_change_summary ?? payload.memberChangeSummary,
  );

  const normalizeConflictOrderIds = (raw: unknown): string[] => {
    return (Array.isArray(raw) ? raw : [])
      .map((id) => String(id ?? '').trim())
      .filter(Boolean);
  };

  const resolvedLaneIds = normalizeIds(
    payload.lane_ids ?? payload.laneIds,
    String(payload.primary_lane_id ?? payload.primaryLaneId ?? payload.lane_id ?? payload.laneId ?? ''),
  );

  return {
    resource_type: resourceType,
    resource_id: resourceId,
    resource_label: resourceLabel || resourceId,
    primary_resource_type: primaryResourceType as 'team' | 'employee',
    primary_resource_id: primaryResourceId || resourceId,
    target_view_mode: targetViewMode as ViewMode,
    lane_id: String(payload.lane_id ?? payload.laneId ?? payload.primary_lane_id ?? payload.primaryLaneId ?? '').trim(),
    primary_lane_id: String(payload.primary_lane_id ?? payload.primaryLaneId ?? payload.lane_id ?? payload.laneId ?? '').trim(),
    resource_ids: resourceIds,
    lane_ids: resolvedLaneIds,
    highlight_scope: highlightScope,
    member_change_summary: memberChangeSummary,
    related_order_ids: normalizeConflictOrderIds(payload.related_order_ids ?? payload.relatedOrderIds ?? []),
    source_panel: String(payload.source_panel ?? payload.sourcePanel ?? '').trim(),
    source_key: sourceKey,
    visible_resource_ids: normalizeIds(payload.visible_resource_ids ?? payload.visibleResourceIds, ''),
    missing_resource_ids: normalizeIds(payload.missing_resource_ids ?? payload.missingResourceIds, ''),
  };
}

/** Build crew-scoped resource focus. */

export function buildCrewResourceFocus(payload: Record<string, unknown> = {}, overrides: Record<string, unknown> = {}): ResourceFocus | null {
  const crewInfo = collectCrewFocusMembers({ ...payload, ...overrides });
  const members = crewInfo.members ?? [];
  if (members.length === 0) return null;

  const primaryMember = members[0];
  return buildResourceFocus({
    ...payload,
    ...overrides,
    resource_type: 'employee',
    resource_id: primaryMember.resource_id,
    resource_label: primaryMember.resource_label,
    primary_resource_type: 'employee',
    primary_resource_id: primaryMember.resource_id,
    resource_ids: members.map((m) => m.resource_id || m.resource_label).filter(Boolean),
    highlight_scope: members.length > 1 ? 'crew' : 'single',
    target_view_mode: 'employee',
    member_change_summary: crewInfo.member_change_summary,
  });
}

/** Get display text for a resource focus. */

export function getResourceFocusDisplayText(resourceFocus: ResourceFocus | null): string {
  if (!resourceFocus) return '';
  const resourceCount = Array.isArray(resourceFocus.resource_ids) ? resourceFocus.resource_ids.length : 0;
  if (resourceFocus.highlight_scope === 'crew' || resourceCount > 1) {
    return `执行编组 ${Math.max(resourceCount, 1)} 人`;
  }
  const typeLabel = resourceFocus.resource_type === 'employee' ? '个人' : '班组';
  return `${typeLabel} ${resourceFocus.resource_label || resourceFocus.resource_id || '-'}`;
}

/** Check if a timeline item matches resource focus. */

export function doesItemMatchResourceFocus(item: DispatchOrder | null, resourceFocus: ResourceFocus | null): boolean {
  if (!item || item.is_flight_summary || !resourceFocus) return false;

  if (resourceFocus.resource_type === 'team') {
    if (resourceFocus.resource_id && String(item.team_id ?? '').trim() === resourceFocus.resource_id) return true;
    if (resourceFocus.resource_label) {
      return String(item.team_name ?? item.lane_label ?? '').trim() === resourceFocus.resource_label;
    }
    return false;
  }

  const focusResourceIds = Array.isArray(resourceFocus.resource_ids) && resourceFocus.resource_ids.length > 0
    ? resourceFocus.resource_ids
    : (resourceFocus.resource_id ? [resourceFocus.resource_id] : []);

  if (focusResourceIds.length > 0) {
    if (focusResourceIds.includes(String(item.focus_user_id ?? '').trim())) return true;
    if (focusResourceIds.includes(String(item.individual_user_id ?? '').trim())) return true;
    const members = Array.isArray(item.members) ? item.members : [];
    return members.some((member) => focusResourceIds.includes(normalizeTimelineMemberUserId(member)));
  }

  const label = resourceFocus.resource_label;
  if (!label) return false;
  if (String(item.focus_user_name ?? '').trim() === label) return true;
  if (String(item.individual_username ?? '').trim() === label) return true;
  const members = Array.isArray(item.members) ? item.members : [];
  return members.some((member) => normalizeTimelineMemberName(member) === label);
}

/** Check if a lane matches resource focus. */

export function doesLaneMatchResourceFocus(lane: TimelineLane | null, resourceFocus: ResourceFocus | null): boolean {
  if (!lane || !resourceFocus) return false;
  const resourceType = normalizeResourceType(lane.resource_type);
  if (resourceType && resourceType !== resourceFocus.resource_type) return false;

  const laneId = String(lane.id ?? '').trim();
  const normalizedLaneIds = Array.isArray(resourceFocus.lane_ids) ? resourceFocus.lane_ids : [];
  if (laneId && normalizedLaneIds.includes(laneId)) return true;

  const laneResourceId = String(lane.resource_id ?? '').trim();
  const focusResourceIds = Array.isArray(resourceFocus.resource_ids) ? resourceFocus.resource_ids : [];
  if (laneResourceId) {
    if (resourceFocus.resource_id && laneResourceId === resourceFocus.resource_id) return true;
    if (focusResourceIds.includes(laneResourceId)) return true;
  }

  const laneResourceLabel = String(lane.resource_label ?? lane.label ?? '').trim();
  return Boolean(resourceFocus.resource_label) && laneResourceLabel === resourceFocus.resource_label;
}

/** Normalise search query string. */

export interface TerminalInfo {
  terminal: string;
  label: string;
  active: boolean;
}


export async function loadTerminalInfoList(): Promise<TerminalInfo[]> {
  try {
    const { get } = useApi();
    const result = await get<unknown>('/api/v2/dispatch/resources/stands');
    if (!result.ok || !result.data) return [];

    const payload = result.data as Record<string, unknown> | unknown[];
    const stands = Array.isArray(payload) ? payload : [];

    const terminalSet = new Set<string>();
    for (const stand of stands) {
      if (stand && typeof stand === 'object') {
        const terminal = (stand as Record<string, unknown>).terminal as string | undefined;
        if (terminal) terminalSet.add(String(terminal));
      }
    }

    return Array.from(terminalSet).map(t => ({
      terminal: t,
      label: t.startsWith('T') ? t : `T${t}`,
      active: false,
    }));
  } catch (error) {
    console.warn('Failed to load terminal info list:', error);
    return [];
  }
}

