import { computed, reactive, ref } from 'vue';
import { useApi } from '@/composables/useApi';
import { getUserPermissions, hasUserPermission, useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import { ONTOLOGY_BASE, ontologyGet, ontologyPost } from './ontologyApi';
import type {
  AircraftResourceView,
  AutoLinkScanResult,
  FlightResourceView,
  GateAssignmentResult,
  OntologyTabId,
  ReassignAppliedResult,
  ResourceAdjustmentSuggestion,
  StandOccupationResult,
  TurnaroundLink,
} from './types';
import { idField } from './types';

export function useOntologyWorkbench() {
  const api = useApi();
  const auth = useAuth();
  const toast = useToast();

  const activeTab = ref<OntologyTabId>('views');
  const contextMode = ref<'flight' | 'aircraft'>('flight');
  const contextKey = ref('');
  const busy = ref(false);
  const loadingView = ref(false);

  const flightView = ref<FlightResourceView | null>(null);
  const aircraftView = ref<AircraftResourceView | null>(null);
  const suggestions = ref<ResourceAdjustmentSuggestion[]>([]);
  const links = ref<TurnaroundLink[]>([]);
  const lastWarnings = ref<string[]>([]);
  const lastScan = ref<AutoLinkScanResult | null>(null);

  const reassignForm = reactive({
    flight_id: '',
    new_registration: '',
  });

  const standForm = reactive({
    registration: '',
    stand_code: '',
    starts_at: '',
    ends_at: '',
    flight_id: '',
    kind: 'normal',
    moving_to_stand: '',
    sync_flight_plan: true,
  });

  const gateForm = reactive({
    registration: '',
    gate_code: '',
    starts_at: '',
    ends_at: '',
    flight_id: '',
    sync_flight_plan: true,
  });

  const suggestionForm = reactive({
    flight_id: '',
    kind: 'stand',
    suggested_value: '',
    current_value: '',
    reason: '',
  });

  const linkForm = reactive({
    inbound_flight_id: '',
    outbound_flight_id: '',
    source: 'manual',
  });

  const canRead = computed(() => hasUserPermission(auth.getUser(), 'ontology.read'));
  const canReassign = computed(() => hasUserPermission(auth.getUser(), 'ontology.aircraft.reassign'));
  const canStand = computed(() => hasUserPermission(auth.getUser(), 'ontology.stand.manage'));
  const canGate = computed(() => hasUserPermission(auth.getUser(), 'ontology.gate.manage'));
  const canConfirm = computed(() => hasUserPermission(auth.getUser(), 'ontology.plan.confirm'));
  const canAcceptStand = computed(() =>
    hasUserPermission(auth.getUser(), 'ontology.suggestion.accept_stand'),
  );
  const canAcceptGate = computed(() =>
    hasUserPermission(auth.getUser(), 'ontology.suggestion.accept_gate'),
  );
  const canReject = computed(() => hasUserPermission(auth.getUser(), 'ontology.suggestion.reject'));

  function fillFromContext() {
    const key = contextKey.value.trim();
    if (!key) return;
    if (contextMode.value === 'flight') {
      reassignForm.flight_id = key;
      standForm.flight_id = key;
      gateForm.flight_id = key;
      suggestionForm.flight_id = key;
      linkForm.outbound_flight_id = key;
    } else {
      standForm.registration = key;
      gateForm.registration = key;
      reassignForm.new_registration = key;
    }
  }

  async function loadContextView() {
    const key = contextKey.value.trim();
    if (!key) {
      toast.showToast('error', '请输入航班 ID 或机号');
      return;
    }
    if (!canRead.value) {
      toast.showToast('error', '缺少权限 ontology.read');
      return;
    }
    loadingView.value = true;
    lastWarnings.value = [];
    try {
      fillFromContext();
      if (contextMode.value === 'flight') {
        const res = await ontologyGet<FlightResourceView>(
          api,
          `${ONTOLOGY_BASE}/flights/${encodeURIComponent(key)}/resources`,
        );
        if (!res.ok) {
          toast.showToast('error', res.error, { duration: 5000 });
          flightView.value = null;
          return;
        }
        flightView.value = res.data;
        aircraftView.value = null;
        if (res.data.registration) {
          standForm.registration = res.data.registration;
          gateForm.registration = res.data.registration;
        }
        await loadLinksForFlight(key);
      } else {
        const res = await ontologyGet<AircraftResourceView>(
          api,
          `${ONTOLOGY_BASE}/aircraft/${encodeURIComponent(key)}/resources`,
        );
        if (!res.ok) {
          toast.showToast('error', res.error, { duration: 5000 });
          aircraftView.value = null;
          return;
        }
        aircraftView.value = res.data;
        flightView.value = null;
      }
      await loadSuggestions();
      toast.showToast('success', '资源视图已刷新');
    } finally {
      loadingView.value = false;
    }
  }

  async function loadSuggestions(flightId?: string) {
    if (!canRead.value) return;
    const q = new URLSearchParams();
    const fid = (
      flightId
      || suggestionForm.flight_id
      || reassignForm.flight_id
      || contextKey.value
      || ''
    ).trim();
    if (fid && contextMode.value === 'flight') q.set('flight_id', fid);
    q.set('limit', '50');
    const res = await ontologyGet<ResourceAdjustmentSuggestion[]>(
      api,
      `${ONTOLOGY_BASE}/suggestions?${q.toString()}`,
    );
    if (res.ok) {
      suggestions.value = Array.isArray(res.data) ? res.data : [];
    }
  }

  async function loadLinksForFlight(flightId: string) {
    const res = await ontologyGet<TurnaroundLink[]>(
      api,
      `${ONTOLOGY_BASE}/flights/${encodeURIComponent(flightId)}/turnaround-links`,
    );
    if (res.ok) {
      links.value = Array.isArray(res.data) ? res.data : [];
    } else {
      links.value = [];
    }
  }

  async function submitReassign() {
    if (!canReassign.value) {
      toast.showToast('error', '缺少权限 ontology.aircraft.reassign');
      return;
    }
    const flight_id = reassignForm.flight_id.trim();
    const new_registration = reassignForm.new_registration.trim();
    if (!flight_id || !new_registration) {
      toast.showToast('error', '请填写航班 ID 与新机号');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost<{ applied: ReassignAppliedResult[] }>(
        api,
        `${ONTOLOGY_BASE}/aircraft/reassign`,
        { changes: [{ flight_id, new_registration }] },
      );
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      const applied = res.data?.applied?.[0];
      toast.showToast(
        'success',
        applied
          ? `已换机 ${applied.old_registration ?? '—'} → ${applied.new_registration}`
          : '换机完成',
      );
      if (contextMode.value === 'flight') {
        contextKey.value = flight_id;
        await loadContextView();
      }
    } finally {
      busy.value = false;
    }
  }

  function toIsoLocal(local: string): string | null {
    if (!local.trim()) return null;
    const d = new Date(local);
    if (Number.isNaN(d.getTime())) return null;
    return d.toISOString();
  }

  async function submitAllocateStand() {
    if (!canStand.value) {
      toast.showToast('error', '缺少权限 ontology.stand.manage');
      return;
    }
    const starts = toIsoLocal(standForm.starts_at);
    const ends = toIsoLocal(standForm.ends_at);
    if (!standForm.registration.trim() || !standForm.stand_code.trim() || !starts || !ends) {
      toast.showToast('error', '请填写机号、机位与时段');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost<StandOccupationResult>(api, `${ONTOLOGY_BASE}/stands/occupations`, {
        registration: standForm.registration.trim(),
        stand_code: standForm.stand_code.trim(),
        starts_at: starts,
        ends_at: ends,
        kind: standForm.kind,
        moving_to_stand: standForm.kind === 'moving' ? standForm.moving_to_stand.trim() || null : null,
        flight_id: standForm.flight_id.trim() || null,
        sync_flight_plan: standForm.sync_flight_plan,
      });
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      lastWarnings.value = res.data?.overlap_warnings ?? [];
      toast.showToast(
        lastWarnings.value.length ? 'warning' : 'success',
        lastWarnings.value.length ? `机位已分配（${lastWarnings.value.length} 条重叠告警）` : '机位已分配',
      );
      await loadContextView();
    } finally {
      busy.value = false;
    }
  }

  async function submitAllocateGate() {
    if (!canGate.value) {
      toast.showToast('error', '缺少权限 ontology.gate.manage');
      return;
    }
    const starts = toIsoLocal(gateForm.starts_at);
    const ends = toIsoLocal(gateForm.ends_at);
    if (!gateForm.registration.trim() || !gateForm.gate_code.trim() || !starts || !ends) {
      toast.showToast('error', '请填写机号、登机口与时段');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost<GateAssignmentResult>(api, `${ONTOLOGY_BASE}/gates/assignments`, {
        registration: gateForm.registration.trim(),
        gate_code: gateForm.gate_code.trim(),
        starts_at: starts,
        ends_at: ends,
        flight_id: gateForm.flight_id.trim() || null,
        sync_flight_plan: gateForm.sync_flight_plan,
      });
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      lastWarnings.value = res.data?.consistency_warnings ?? [];
      toast.showToast(
        lastWarnings.value.length ? 'warning' : 'success',
        lastWarnings.value.length ? `登机口已分配（${lastWarnings.value.length} 条一致性提示）` : '登机口已分配',
      );
      await loadContextView();
    } finally {
      busy.value = false;
    }
  }

  async function submitCreateSuggestion() {
    if (!canStand.value && !canGate.value && !canAcceptStand.value && !canAcceptGate.value) {
      toast.showToast('error', '缺少创建建议权限');
      return;
    }
    if (!suggestionForm.flight_id.trim() || !suggestionForm.suggested_value.trim()) {
      toast.showToast('error', '请填写航班与建议值');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost(api, `${ONTOLOGY_BASE}/suggestions`, {
        flight_id: suggestionForm.flight_id.trim(),
        kind: suggestionForm.kind,
        suggested_value: suggestionForm.suggested_value.trim(),
        current_value: suggestionForm.current_value.trim() || null,
        reason: suggestionForm.reason.trim() || null,
      });
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      toast.showToast('success', '建议已创建');
      suggestionForm.suggested_value = '';
      suggestionForm.reason = '';
      await loadSuggestions(suggestionForm.flight_id.trim());
    } finally {
      busy.value = false;
    }
  }

  async function acceptSuggestion(item: ResourceAdjustmentSuggestion) {
    const need = item.kind === 'gate' ? canAcceptGate.value : canAcceptStand.value;
    if (!need) {
      toast.showToast('error', `缺少接受权限（${item.kind}）`);
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost(api, `${ONTOLOGY_BASE}/suggestions/${encodeURIComponent(item.id)}/accept`, {
        accepted_by: auth.getUser()?.username ?? 'operator',
        actor_permissions: getUserPermissions(auth.getUser()),
      });
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      toast.showToast('success', '建议已接受并执行');
      await loadSuggestions(idField(item.flight_id));
      await loadContextView();
    } finally {
      busy.value = false;
    }
  }

  async function rejectSuggestion(item: ResourceAdjustmentSuggestion) {
    if (!canReject.value) {
      toast.showToast('error', '缺少权限 ontology.suggestion.reject');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost(api, `${ONTOLOGY_BASE}/suggestions/${encodeURIComponent(item.id)}/reject`, {
        rejected_by: auth.getUser()?.username ?? 'operator',
        reason: null,
      });
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      toast.showToast('success', '建议已驳回');
      await loadSuggestions(idField(item.flight_id));
    } finally {
      busy.value = false;
    }
  }

  async function submitCreateLink() {
    if (!canReassign.value && !canStand.value && !canConfirm.value) {
      toast.showToast('error', '缺少建链权限');
      return;
    }
    if (!linkForm.inbound_flight_id.trim() || !linkForm.outbound_flight_id.trim()) {
      toast.showToast('error', '请填写进港与出港航班');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost(api, `${ONTOLOGY_BASE}/turnaround-links`, {
        inbound_flight_id: linkForm.inbound_flight_id.trim(),
        outbound_flight_id: linkForm.outbound_flight_id.trim(),
        source: linkForm.source,
      });
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      toast.showToast('success', '周转链接已创建');
      await loadLinksForFlight(linkForm.outbound_flight_id.trim());
    } finally {
      busy.value = false;
    }
  }

  async function breakLink(link: TurnaroundLink) {
    if (!canReassign.value && !canStand.value && !canConfirm.value) {
      toast.showToast('error', '缺少拆链权限');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost(
        api,
        `${ONTOLOGY_BASE}/turnaround-links/${encodeURIComponent(link.id)}/break`,
        { reason: 'manual break from ontology center' },
      );
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      toast.showToast('success', '链接已拆');
      const flightId = idField(link.outbound_flight_id) || contextKey.value.trim();
      if (flightId) await loadLinksForFlight(flightId);
    } finally {
      busy.value = false;
    }
  }

  async function runAutoScan() {
    if (!canConfirm.value) {
      toast.showToast('error', '缺少权限 ontology.plan.confirm');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost<AutoLinkScanResult>(api, `${ONTOLOGY_BASE}/turnaround-links/auto-scan`, {
        window_minutes: 360,
        limit: 100,
      });
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      lastScan.value = res.data;
      toast.showToast(
        'success',
        `扫描完成：评估 ${res.data.evaluated}，新建 ${res.data.created.length}，跳过 ${res.data.skipped}`,
      );
      if (contextMode.value === 'flight' && contextKey.value.trim()) {
        await loadLinksForFlight(contextKey.value.trim());
      }
    } finally {
      busy.value = false;
    }
  }

  async function confirmDrafts() {
    if (!canConfirm.value) {
      toast.showToast('error', '缺少权限 ontology.plan.confirm');
      return;
    }
    const flight_id = (reassignForm.flight_id || contextKey.value).trim();
    if (!flight_id) {
      toast.showToast('error', '请指定要确认的航班 ID');
      return;
    }
    busy.value = true;
    try {
      const res = await ontologyPost(api, `${ONTOLOGY_BASE}/flights/confirm-drafts`, {
        flight_ids: [flight_id],
        confirmed_by: auth.getUser()?.username ?? 'operator',
      });
      if (!res.ok) {
        toast.showToast('error', res.error, { duration: 6000 });
        return;
      }
      toast.showToast('success', 'draft 批确认已提交');
      await loadContextView();
    } finally {
      busy.value = false;
    }
  }

  function defaultDateTimeLocal(hoursOffset: number): string {
    const d = new Date(Date.now() + hoursOffset * 3600_000);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  // sensible defaults for allocate forms
  if (!standForm.starts_at) standForm.starts_at = defaultDateTimeLocal(0);
  if (!standForm.ends_at) standForm.ends_at = defaultDateTimeLocal(2);
  if (!gateForm.starts_at) gateForm.starts_at = defaultDateTimeLocal(0);
  if (!gateForm.ends_at) gateForm.ends_at = defaultDateTimeLocal(2);

  return {
    activeTab,
    contextMode,
    contextKey,
    busy,
    loadingView,
    flightView,
    aircraftView,
    suggestions,
    links,
    lastWarnings,
    lastScan,
    reassignForm,
    standForm,
    gateForm,
    suggestionForm,
    linkForm,
    canRead,
    canReassign,
    canStand,
    canGate,
    canConfirm,
    canAcceptStand,
    canAcceptGate,
    canReject,
    loadContextView,
    loadSuggestions,
    submitReassign,
    submitAllocateStand,
    submitAllocateGate,
    submitCreateSuggestion,
    acceptSuggestion,
    rejectSuggestion,
    submitCreateLink,
    breakLink,
    runAutoScan,
    confirmDrafts,
    idField,
  };
}
