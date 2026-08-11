import { computed, ref, watch } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import { useToast } from '../../../composables/useToast';
import {
  type BusinessCaseCreatePayload,
  type Flight,
  type LegType,
} from '../../../composables/useFlightData';
import type {
  BusinessCaseTypeDefinition,
} from '../../../types/backend';
import type { UseFlightDataReturn } from '../../../composables/useFlightData';

export interface BoundFlightBindingOption {
  value: string;
  legType: LegType;
  flightNo: string;
  label: string;
}

export interface UseFlightMonitorModalsOptions {
  flightData: UseFlightDataReturn;
  selectedFlightId: Ref<string | null>;
  selectedFlight: ComputedRef<Flight | null>;
  isAuthenticated: ComputedRef<boolean>;
  refreshFlights: () => Promise<void>;
  announce: (message: string) => void;
}

export interface UseFlightMonitorModalsReturn {
  eventCreationState: Ref<{
    isOpen: boolean;
    types: BusinessCaseTypeDefinition[];
    submitting: boolean;
    form: {
      eventType: string;
      eventStatus: string;
      description: string;
      gate: string;
      triggerReason: string;
      boundFlightValue: string;
    };
  }>;
  boundFlightBindingOptions: ComputedRef<BoundFlightBindingOption[]>;
  canSubmitEventCreation: ComputedRef<boolean>;
  openEventModal: () => void;
  closeEventModal: () => void;
  handleEventCreationSubmit: () => Promise<void>;
  handleAutoCopilotCreated: (payload: { caseIds: string[]; notificationGroupCount: number }) => Promise<void>;
  fieldEditState: Ref<{
    isOpen: boolean;
    flightId: string;
    field: string;
    label: string;
    type: string;
    value: string;
    saving: boolean;
  }>;
  remarkEditState: Ref<{
    isOpen: boolean;
    flightId: string;
    field: string;
    label: string;
    value: string;
    saving: boolean;
  }>;
  handleEditField: (flightId: string, field: string, type: string, value: string) => void;
  openRemarkEdit: (flightId: string, field: string, value: string) => void;
  saveFieldEdit: () => Promise<void>;
  saveRemarkEdit: () => Promise<void>;
  contextMenuState: Ref<{
    isOpen: boolean;
    x: number;
    y: number;
    flightId: string;
    field: string;
    type: string;
    value: string;
  }>;
  handleContextMenu: (event: MouseEvent, flightId: string, field: string, type: string, value: string) => void;
  closeContextMenu: () => void;
  handleContextModify: () => void;
  handleContextRevoke: () => Promise<void>;
}

const FIELD_LABELS: Record<string, string> = {
  scheduled_departure: '计划起飞',
  scheduled_arrival: '计划到达',
  cobt_time: 'COBT',
  boarding_allowed_time: '允许登机',
  start_boarding_time: '开始登机',
  end_boarding_time: '结束登机',
  on_blocks_time: '上轮挡',
  off_blocks_time: '撤轮挡',
  stand: '机位',
  baggage_carousel: '行李转盘',
  flight_remarks: '航班备注',
  aircraft_check_remarks: '复核备注',
};

function buildBoundFlightBindingOptions(flight: Flight | null): BoundFlightBindingOption[] {
  if (!flight) return [];
  const options: BoundFlightBindingOption[] = [];
  const inboundFlightNo = String(flight.inbound_leg?.flight_no || '').trim();
  const outboundFlightNo = String(flight.outbound_leg?.flight_no || '').trim();
  if (inboundFlightNo) {
    options.push({ value: `inbound::${inboundFlightNo}`, legType: 'inbound', flightNo: inboundFlightNo, label: `进港 ${inboundFlightNo}` });
  }
  if (outboundFlightNo) {
    options.push({ value: `outbound::${outboundFlightNo}`, legType: 'outbound', flightNo: outboundFlightNo, label: `出港 ${outboundFlightNo}` });
  }
  return options;
}

function resolveDefaultBoundFlightValue(options: BoundFlightBindingOption[]): string {
  return options.find((option) => option.legType === 'outbound')?.value || options[0]?.value || '';
}

export function useFlightMonitorModals(options: UseFlightMonitorModalsOptions): UseFlightMonitorModalsReturn {
  const { flightData, selectedFlightId, selectedFlight, isAuthenticated, refreshFlights, announce } = options;
  const toast = useToast();

  const eventCreationState = ref({
    isOpen: false,
    types: [] as BusinessCaseTypeDefinition[],
    submitting: false,
    form: {
      eventType: '',
      eventStatus: 'INITIAL',
      description: '',
      gate: '',
      triggerReason: '',
      boundFlightValue: '',
    },
  });

  const boundFlightBindingOptions = computed<BoundFlightBindingOption[]>(() => buildBoundFlightBindingOptions(selectedFlight.value));

  const canSubmitEventCreation = computed(() => Boolean(
    isAuthenticated.value
    && selectedFlightId.value
    && eventCreationState.value.form.eventType
    && eventCreationState.value.form.boundFlightValue
    && boundFlightBindingOptions.value.length > 0,
  ));

  function syncEventBindingSelection(): void {
    const options = boundFlightBindingOptions.value;
    const currentValue = eventCreationState.value.form.boundFlightValue;
    if (options.some((option) => option.value === currentValue)) return;
    eventCreationState.value.form.boundFlightValue = resolveDefaultBoundFlightValue(options);
  }

  function resetEventCreationForm(): void {
    eventCreationState.value.form = {
      eventType: '',
      eventStatus: 'INITIAL',
      description: '',
      gate: '',
      triggerReason: '',
      boundFlightValue: resolveDefaultBoundFlightValue(boundFlightBindingOptions.value),
    };
  }

  watch(boundFlightBindingOptions, () => {
    syncEventBindingSelection();
  });

  function openEventModal(): void {
    if (!selectedFlight.value) {
      toast.showToast('warning', '请先选择一个航班', { duration: 4000 });
      return;
    }
    if (!isAuthenticated.value) {
      toast.showToast('warning', '请先登录后再创建事项', { duration: 4000 });
      return;
    }
    resetEventCreationForm();
    eventCreationState.value.isOpen = true;
  }

  function closeEventModal(): void {
    eventCreationState.value.isOpen = false;
    resetEventCreationForm();
  }

  async function handleEventCreationSubmit(): Promise<void> {
    const { eventType, eventStatus, description, gate, triggerReason, boundFlightValue } = eventCreationState.value.form;
    const boundFlight = boundFlightBindingOptions.value.find((option) => option.value === boundFlightValue);
    if (!selectedFlightId.value) {
      toast.showToast('warning', '请先选择一个航班', { duration: 4000 });
      return;
    }
    if (!isAuthenticated.value) {
      toast.showToast('warning', '请先登录后再创建事项', { duration: 4000 });
      return;
    }
    if (!boundFlight) {
      toast.showToast('warning', '请选择要绑定的航班号', { duration: 4000 });
      return;
    }

    let caseData: BusinessCaseCreatePayload;
    if (eventType === 'gate_baggage_check') {
      caseData = {
        case_type: eventType,
        flight_id: selectedFlightId.value,
        description: triggerReason ? `[${triggerReason}] ${description}` : description,
        status: eventStatus,
        context: {
          bound_leg_type: boundFlight.legType,
          bound_flight_no: boundFlight.flightNo,
          gate: gate || null,
          trigger_reason: triggerReason,
          extra_info: description || null,
        },
      };
    } else {
      caseData = {
        case_type: eventType,
        flight_id: selectedFlightId.value,
        description: description,
        status: eventStatus,
        context: {
          bound_leg_type: boundFlight.legType,
          bound_flight_no: boundFlight.flightNo,
        },
      };
    }

    try {
      eventCreationState.value.submitting = true;
      await flightData.submitBusinessCase(caseData);
      closeEventModal();
      announce('业务事项创建成功');
    } catch (err: unknown) {
      toast.showToast('error', `创建业务事项失败: ${(err as { message?: string }).message}`, { duration: 5000 });
    } finally {
      eventCreationState.value.submitting = false;
    }
  }

  async function handleAutoCopilotCreated(payload: { caseIds: string[]; notificationGroupCount: number }): Promise<void> {
    const message = `Auto Copilot 已创建 ${payload.caseIds.length} 条事项，通知组 ${payload.notificationGroupCount} 组`;
    announce(message);
    toast.showToast('success', message, { duration: 4000 });
    await refreshFlights();
  }

  const fieldEditState = ref({
    isOpen: false,
    flightId: '',
    field: '',
    label: '',
    type: 'text' as string,
    value: '',
    saving: false,
  });

  const remarkEditState = ref({
    isOpen: false,
    flightId: '',
    field: '',
    label: '',
    value: '',
    saving: false,
  });

  function openFieldEdit(flightId: string, field: string, type: string, value: string): void {
    fieldEditState.value = { isOpen: true, flightId, field, label: FIELD_LABELS[field] || '数据', type, value, saving: false };
  }

  function openRemarkEdit(flightId: string, field: string, value: string): void {
    remarkEditState.value = { isOpen: true, flightId, field, label: FIELD_LABELS[field] || '备注', value, saving: false };
  }

  function handleEditField(flightId: string, field: string, type: string, value: string): void {
    if (field === 'flight_remarks' || field === 'aircraft_check_remarks') {
      openRemarkEdit(flightId, field, value);
    } else {
      openFieldEdit(flightId, field, type, value);
    }
  }

  async function saveFieldEdit(): Promise<void> {
    if (!fieldEditState.value.flightId || !fieldEditState.value.field) return;
    fieldEditState.value.saving = true;
    try {
      let finalValue = fieldEditState.value.value;
      if (fieldEditState.value.type === 'datetime-local') {
        finalValue = new Date(finalValue).toISOString();
      }
      await flightData.updateFlightField(fieldEditState.value.flightId, fieldEditState.value.field, finalValue);
      fieldEditState.value.isOpen = false;
      announce(`${fieldEditState.value.label}修改成功`);
    } catch (err: unknown) {
      toast.showToast('error', `修改失败: ${(err as { message?: string }).message}`, { duration: 5000 });
    } finally {
      fieldEditState.value.saving = false;
    }
  }

  async function saveRemarkEdit(): Promise<void> {
    remarkEditState.value.saving = true;
    try {
      await flightData.updateFlightField(remarkEditState.value.flightId, remarkEditState.value.field, remarkEditState.value.value);
      remarkEditState.value.isOpen = false;
      announce(`${remarkEditState.value.label}修改成功`);
    } catch (err: unknown) {
      toast.showToast('error', `备注修改失败: ${(err as { message?: string }).message}`, { duration: 5000 });
    } finally {
      remarkEditState.value.saving = false;
    }
  }

  const contextMenuState = ref({
    isOpen: false,
    x: 0,
    y: 0,
    flightId: '',
    field: '',
    type: 'datetime-local',
    value: '',
  });

  function handleContextMenu(event: MouseEvent, flightId: string, field: string, type: string, value: string): void {
    const menuWidth = 220;
    const menuHeight = 96;
    const viewportPadding = 12;
    const maxX = Math.max(viewportPadding, window.innerWidth - menuWidth - viewportPadding);
    const maxY = Math.max(viewportPadding, window.innerHeight - menuHeight - viewportPadding);
    contextMenuState.value = {
      isOpen: true,
      x: Math.min(Math.max(event.clientX, viewportPadding), maxX),
      y: Math.min(Math.max(event.clientY, viewportPadding), maxY),
      flightId,
      field,
      type,
      value,
    };
  }

  function closeContextMenu(): void {
    contextMenuState.value.isOpen = false;
  }

  function handleContextModify(): void {
    const { flightId, field, type, value } = contextMenuState.value;
    closeContextMenu();
    handleEditField(flightId, field, type as 'text' | 'datetime-local', value);
  }

  async function handleContextRevoke(): Promise<void> {
    const { flightId, field } = contextMenuState.value;
    closeContextMenu();
    if (!flightId || !field) return;
    const confirmed = window.confirm('确定要撤销此时间吗？撤销后将变更为 --。');
    if (!confirmed) return;
    try {
      await flightData.updateFlightField(flightId, field, null);
      announce(`已撤销航班 ${flightId} 的 ${FIELD_LABELS[field] || field}`);
      toast.showToast('success', `${FIELD_LABELS[field] || field} 已撤销`);
    } catch (err) {
      const message = err instanceof Error ? err.message : '未知错误';
      toast.showToast('error', `撤销失败: ${message}`);
      announce(`撤销失败：${message}`);
    }
  }

  return {
    eventCreationState,
    boundFlightBindingOptions,
    canSubmitEventCreation,
    openEventModal,
    closeEventModal,
    handleEventCreationSubmit,
    handleAutoCopilotCreated,
    fieldEditState,
    remarkEditState,
    handleEditField,
    openRemarkEdit,
    saveFieldEdit,
    saveRemarkEdit,
    contextMenuState,
    handleContextMenu,
    closeContextMenu,
    handleContextModify,
    handleContextRevoke,
  };
}
