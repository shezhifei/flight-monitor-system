import { computed, ref, watch } from 'vue';
import {
  getWorkspaceModule,
  isWorkspaceModuleId,
  PINNED_MODULE_ID,
  WORKSPACE_MAX_TABS,
  WORKSPACE_MODULES,
  WORKSPACE_STORAGE_KEY,
  workspaceEmbedSrc,
  type WorkspaceModuleId,
} from '@/shared/workspace-modules';

export interface WorkspaceTab {
  id: WorkspaceModuleId;
  title: string;
  shortTitle: string;
  src: string;
  pinned: boolean;
}

interface PersistedState {
  openIds: WorkspaceModuleId[];
  activeId: WorkspaceModuleId;
}

function readQueryTab(): WorkspaceModuleId | null {
  if (typeof window === 'undefined') return null;
  const raw = new URLSearchParams(window.location.search).get('tab');
  return isWorkspaceModuleId(raw) ? raw : null;
}

function loadPersisted(): PersistedState | null {
  try {
    const raw = sessionStorage.getItem(WORKSPACE_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PersistedState;
    if (!parsed || !Array.isArray(parsed.openIds)) return null;
    const openIds = parsed.openIds.filter(isWorkspaceModuleId);
    if (!openIds.includes(PINNED_MODULE_ID)) {
      openIds.unshift(PINNED_MODULE_ID);
    }
    const activeId = isWorkspaceModuleId(parsed.activeId) && openIds.includes(parsed.activeId)
      ? parsed.activeId
      : PINNED_MODULE_ID;
    return { openIds, activeId };
  } catch {
    return null;
  }
}

function persist(openIds: WorkspaceModuleId[], activeId: WorkspaceModuleId): void {
  try {
    const payload: PersistedState = { openIds, activeId };
    sessionStorage.setItem(WORKSPACE_STORAGE_KEY, JSON.stringify(payload));
  } catch {
    // ignore quota / private mode
  }
}

function toTab(id: WorkspaceModuleId): WorkspaceTab {
  const mod = getWorkspaceModule(id)!;
  return {
    id,
    title: mod.title,
    shortTitle: mod.shortTitle,
    src: workspaceEmbedSrc(id),
    pinned: Boolean(mod.pinned),
  };
}

export function useWorkspaceTabs() {
  const queryTab = readQueryTab();
  const saved = loadPersisted();

  const initialOpen = (() => {
    const ids = new Set<WorkspaceModuleId>([PINNED_MODULE_ID]);
    if (saved) saved.openIds.forEach((id) => ids.add(id));
    if (queryTab) ids.add(queryTab);
    return Array.from(ids).slice(0, WORKSPACE_MAX_TABS);
  })();

  const initialActive: WorkspaceModuleId = queryTab && initialOpen.includes(queryTab)
    ? queryTab
    : saved?.activeId && initialOpen.includes(saved.activeId)
      ? saved.activeId
      : PINNED_MODULE_ID;

  const openIds = ref<WorkspaceModuleId[]>(initialOpen);
  const activeId = ref<WorkspaceModuleId>(initialActive);

  const openTabs = computed(() => openIds.value.map(toTab));
  const activeTab = computed(() => openTabs.value.find((t) => t.id === activeId.value) ?? openTabs.value[0]);
  const modules = WORKSPACE_MODULES;

  function syncUrl(id: WorkspaceModuleId): void {
    if (typeof window === 'undefined') return;
    const url = new URL(window.location.href);
    url.searchParams.set('tab', id);
    window.history.replaceState({}, '', url.toString());
  }

  function openTab(moduleId: string): boolean {
    if (!isWorkspaceModuleId(moduleId)) return false;

    if (openIds.value.includes(moduleId)) {
      activeId.value = moduleId;
      return true;
    }

    if (openIds.value.length >= WORKSPACE_MAX_TABS) {
      // 达到上限：优先关闭最左侧非固定标签，再打开新标签
      const victim = openIds.value.find((id) => id !== PINNED_MODULE_ID);
      if (!victim) return false;
      openIds.value = openIds.value.filter((id) => id !== victim);
    }

    openIds.value = [...openIds.value, moduleId];
    activeId.value = moduleId;
    return true;
  }

  function activateTab(moduleId: string): void {
    if (!isWorkspaceModuleId(moduleId)) return;
    if (!openIds.value.includes(moduleId)) return;
    activeId.value = moduleId;
  }

  function closeTab(moduleId: string): void {
    if (!isWorkspaceModuleId(moduleId)) return;
    if (moduleId === PINNED_MODULE_ID) return;
    if (!openIds.value.includes(moduleId)) return;

    const idx = openIds.value.indexOf(moduleId);
    const nextOpen = openIds.value.filter((id) => id !== moduleId);
    openIds.value = nextOpen;

    if (activeId.value === moduleId) {
      const fallback = nextOpen[Math.max(0, idx - 1)] ?? PINNED_MODULE_ID;
      activeId.value = fallback;
    }
  }

  watch(
    [openIds, activeId],
    () => {
      persist(openIds.value, activeId.value);
      syncUrl(activeId.value);
    },
    { deep: true },
  );

  // 初次也写一次 URL
  syncUrl(activeId.value);

  return {
    modules,
    openTabs,
    openIds,
    activeId,
    activeTab,
    openTab,
    activateTab,
    closeTab,
    maxTabs: WORKSPACE_MAX_TABS,
  };
}
