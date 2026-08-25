<script setup lang="ts">
import { onBeforeUnmount, ref, useId, watch } from 'vue';
import type { Stakeholder } from '../../composables/useMentionStakeholders';
import { useMentionPicker } from '../../composables/useMentionPicker';
import UiField from '../ui/UiField.vue';
import UiMenu from '../ui/UiMenu.vue';
import UiMenuItem from '../ui/UiMenuItem.vue';
import UiPill from '../ui/UiPill.vue';

/**
 * 提名输入（@某人）：一个输入器 + 一列可选的人。
 *
 * 输入器的形归 UiField（各页不要再自己写一套 .xxx-input）；
 * 展开的那一列归 UiMenu 的 listbox 档 —— 选择器展开的列表和菜单同一套形（§3.6），
 * 但项是「一个值」而不是「一个动作」，所以角色是 option，键盘游标用 aria-selected
 * 报持守（§2.5 判定第 1 条：CSS 绑 aria，不绑一次性 class）。
 *
 * 它开在弹窗的身里，身有自己的滚动口会把绝对定位的层裁掉，所以走定点落法：
 * Teleport 到 body + 视口坐标，层序吃 --z-menu（§3.5「菜单压在弹窗之上，
 * 因为弹窗里也要能开菜单」），不再自己发明一个 100000。
 */
const props = defineProps<{
  modelValue: string;
  stakeholders: Stakeholder[];
  placeholder?: string;
  maxlength?: number;
  rows?: number;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'update:mentionIds', ids: string[]): void;
}>();

const textareaRef = ref<HTMLTextAreaElement | null>(null);

const picker = useMentionPicker({
  getValue: () => props.modelValue,
  setValue: (v) => emit('update:modelValue', v),
  getTextarea: () => textareaRef.value,
  stakeholders: () => props.stakeholders,
  includeAll: false,
});

const { isOpen, filtered, selectedIndex, onKeydown, selectAt, close, mentionIds } = picker;

/** aria-activedescendant 要指得住，所以每一项都得有稳定且本实例唯一的 id */
const listId = useId();
function optionId(index: number): string {
  return `${listId}-opt-${index}`;
}

const MENU_MAX_H = 280;
const MENU_MIN_H = 160;
const MENU_GAP = 4;

/** 定点落法的视口坐标：贴着输入器落下，下面放不开就翻到上面去 */
const menuPos = ref({ x: 0, y: 0 });

function placeMenu(): void {
  const el = textareaRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const roomBelow = window.innerHeight - rect.bottom - MENU_GAP;
  menuPos.value = {
    x: Math.round(rect.left),
    y: roomBelow >= MENU_MIN_H
      ? Math.round(rect.bottom + MENU_GAP)
      : Math.round(Math.max(MENU_GAP, rect.top - MENU_GAP - MENU_MAX_H)),
  };
}

/* fixed 的层不跟着祖先滚，所以开着的时候得盯住滚动与改窗 */
watch(isOpen, (open) => {
  if (open) {
    placeMenu();
    window.addEventListener('scroll', placeMenu, true);
    window.addEventListener('resize', placeMenu);
  } else {
    window.removeEventListener('scroll', placeMenu, true);
    window.removeEventListener('resize', placeMenu);
  }
});

onBeforeUnmount(() => {
  window.removeEventListener('scroll', placeMenu, true);
  window.removeEventListener('resize', placeMenu);
});

function onInput(event: Event) {
  picker.onInput(event);
  if (isOpen.value) placeMenu();
}

watch(mentionIds, (ids) => {
  emit('update:mentionIds', ids);
});
</script>

<template>
  <div class="mention" @click.stop>
    <UiField>
      <textarea
        ref="textareaRef"
        :value="modelValue"
        :placeholder="placeholder || '填写回复内容，输入 @ 可提醒相关人员。'"
        :maxlength="maxlength || 2000"
        :rows="rows || 4"
        :aria-controls="isOpen ? listId : undefined"
        :aria-activedescendant="isOpen ? optionId(selectedIndex) : undefined"
        @input="onInput"
        @keydown="onKeydown"
        @blur="close"
      />
    </UiField>
    <Teleport to="body">
      <!-- 名给读屏，不在列上再写一行「选择提醒人员」（§3.6 不加小标题 / §4.4 不加教学小字） -->
      <UiMenu
        v-if="isOpen"
        :id="listId"
        class="mention__list"
        role="listbox"
        label="提醒人员"
        :x="menuPos.x"
        :y="menuPos.y"
        min-width="240px"
        @mousedown.prevent
      >
        <!--
          悬停不挪键盘游标：挪了，交感就和持守同形了（§2.5 判定第 3 条 / §4.2）。
          划过是 UiMenuItem 那一层淡墨，游标是 aria-selected 的行动衬。
        -->
        <UiMenuItem
          v-for="(s, i) in filtered"
          :id="optionId(i)"
          :key="s.user_id"
          role="option"
          :selected="i === selectedIndex"
          @mousedown.prevent="selectAt(s)"
        >
          <span class="mention__opt">
            <span class="mention__name">{{ s.username }}</span>
            <!-- 职责是属性，不是事态：不出声（§2.4 声只给行动与四类事态；§3.2 属性不带声） -->
            <UiPill v-if="s.is_dispatcher">调度</UiPill>
            <UiPill v-if="s.is_assignee">责任人</UiPill>
          </span>
        </UiMenuItem>
      </UiMenu>
    </Teleport>
  </div>
</template>

<style scoped>
.mention {
  width: 100%;
}

/* 面、线、影、层序全在 UiMenu 里；这里只给这一列一个滚动口，人多了不撑爆视口 */
.mention__list {
  max-height: 280px;
  overflow-y: auto;
}

.mention__opt {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.mention__name {
  font-weight: var(--fw-medium);
  color: var(--ink);
}
</style>
