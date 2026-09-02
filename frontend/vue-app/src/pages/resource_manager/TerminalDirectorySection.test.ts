// @vitest-environment jsdom
import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import TerminalDirectorySection from './TerminalDirectorySection.vue';
import type { FieldOverlay } from '@/composables/useFieldOverlays';
import type { DirectoryModal, Stand, StandFormData, TerminalDirectory } from '@/composables/useTerminalDirectory';

// 与 migrations/157（stand_use 码表 + Stand overlay）一致的六项字段定义。
const standOverlays: FieldOverlay[] = [
  {
    object_name: 'Stand', field_name: 'max_size_category', field_type: 'catalog_ref',
    catalog_code: 'icao_size', required: false, list_visible: false, filterable: false,
    visible_when: null, is_active: true,
  },
  {
    object_name: 'Stand', field_name: 'combined_stand', field_type: 'boolean',
    required: false, list_visible: false, filterable: false, visible_when: null, is_active: true,
  },
  {
    object_name: 'Stand', field_name: 'stand_use', field_type: 'catalog_ref',
    catalog_code: 'stand_use', required: false, list_visible: false, filterable: false,
    visible_when: null, is_active: true,
  },
  {
    object_name: 'Stand', field_name: 'composed_of', field_type: 'object_ref[]',
    object_name_target: 'Stand', required: false, list_visible: false, filterable: false,
    visible_when: { field: 'combined_stand', op: 'eq', value: true }, is_active: true,
  },
  {
    object_name: 'Stand', field_name: 'corresponding_gate', field_type: 'object_ref',
    object_name_target: 'Gate', required: false, list_visible: false, filterable: false,
    visible_when: { field: 'stand_use', op: 'neq', value: 'remote' }, is_active: true,
  },
];

const emptyDirectory: TerminalDirectory = {
  terminal: { terminal_id: 'term_t1', code: 'T1', name: '一号航站楼', is_active: true },
  stands: [],
  gates: [{ gate_id: 'gate_a12', code: 'A12', name: 'A12口', is_active: true }],
  carousels: [],
};

const stand316: Stand = {
  id: 'stand_316',
  code: '316',
  name: '316 号机位',
  terminal: 'T1',
  area: '近机位',
  stand_type: null,
  size_category: 'E',
  is_active: true,
  attributes: { combined_stand: false, stand_use: 'near_domestic' },
};

function baseProps() {
  return {
    active: true,
    canManage: true,
    terminals: [],
    loading: false,
    saving: false,
    terminalSearch: '',
    selectedTerminalId: 'term_t1',
    directory: emptyDirectory,
    contextLoading: false,
    attachableStands: [],
    attachStandId: '',
    modal: { kind: 'stand', item: stand316 } as DirectoryModal,
    terminalForm: { code: '', name: '', attributes: {} },
    gateForm: { code: '', name: '', attributes: {} },
    carouselForm: { code: '', name: '', attributes: {} },
    standForm: {
      code: '316',
      name: '316 号机位',
      area: '近机位',
      stand_type: '',
      size_category: 'E',
      attributes: { combined_stand: false, stand_use: 'near_domestic' },
    } as StandFormData,
    terminalFieldOverlays: [] as FieldOverlay[],
    gateFieldOverlays: [] as FieldOverlay[],
    carouselFieldOverlays: [] as FieldOverlay[],
    standFieldOverlays: standOverlays,
    fieldCatalogEntries: {
      icao_size: [
        { code: 'A', name: 'A' },
        { code: 'C', name: 'C' },
        { code: 'E', name: 'E' },
      ],
      stand_use: [
        { code: 'near_domestic', name: '近机位（国内）' },
        { code: 'near_international', name: '近机位（国际）' },
        { code: 'remote', name: '远机位' },
      ],
    },
    fieldReferenceEntries: {
      Stand: [
        { id: 'stand_316', code: '316', name: '316 号机位' },
        { id: 'stand_316l', code: '316L', name: '316L' },
        { id: 'stand_316r', code: '316R', name: '316R' },
      ],
      Gate: [{ id: 'gate_a12', code: 'A12', name: 'A12口' }],
      Terminal: [{ id: 'term_t1', code: 'T1', name: '一号航站楼' }],
      BaggageCarousel: [{ id: 'car_b1', code: 'B1', name: 'B1 转盘' }],
    },
  };
}

function mountSection(props: ReturnType<typeof baseProps>) {
  return mount(TerminalDirectorySection, {
    props,
    global: {
      // UiModal 走 Teleport to body；stub 掉让内容内联渲染便于断言。
      stubs: { teleport: true },
    },
  });
}

function groupStyle(wrapper: ReturnType<typeof mountSection>, controlId: string): string {
  const control = wrapper.find(`#${controlId}`);
  expect(control.exists()).toBe(true);
  return control.element.parentElement?.getAttribute('style') ?? '';
}

describe('TerminalDirectorySection stand overlay schema', () => {
  it('renders the six-field overlay in the stand modal', () => {
    const wrapper = mountSection(baseProps());
    for (const field of standOverlays) {
      expect(wrapper.find(`#overlay-Stand-${field.field_name}`).exists()).toBe(true);
    }
    // max_size_category 下拉取 icao_size 码表项
    const sizeSelect = wrapper.find('#overlay-Stand-max_size_category');
    expect(sizeSelect.find('option[value="E"]').exists()).toBe(true);
  });

  it('hides composed_of unless combined_stand and hides the gate for remote stands', async () => {
    const props = baseProps();
    const wrapper = mountSection(props);

    // 种子：combined_stand=false → composed_of 隐藏；近机位 → 登机口可见。
    expect(groupStyle(wrapper, 'overlay-Stand-composed_of')).toContain('display: none');
    expect(groupStyle(wrapper, 'overlay-Stand-corresponding_gate')).not.toContain('display: none');

    await wrapper.setProps({
      standForm: { ...props.standForm, attributes: { combined_stand: true, stand_use: 'near_domestic' } },
    });
    expect(groupStyle(wrapper, 'overlay-Stand-composed_of')).not.toContain('display: none');

    await wrapper.setProps({
      standForm: { ...props.standForm, attributes: { combined_stand: true, stand_use: 'remote' } },
    });
    expect(groupStyle(wrapper, 'overlay-Stand-composed_of')).not.toContain('display: none');
    expect(groupStyle(wrapper, 'overlay-Stand-corresponding_gate')).toContain('display: none');
  });

  it('emits update:standForm with overlay attributes and uses code as reference value', async () => {
    const wrapper = mountSection(baseProps());

    const checkbox = wrapper.find('#overlay-Stand-combined_stand');
    await checkbox.setValue(true);
    const emittedForm = wrapper.emitted('update:standForm')?.at(-1)?.[0] as StandFormData;
    expect(emittedForm.attributes).toMatchObject({ combined_stand: true, stand_use: 'near_domestic' });
    // 核心字段原样带回，attributes 整体替换。
    expect(emittedForm.code).toBe('316');
    expect(emittedForm.size_category).toBe('E');

    // object_ref 值是业务键（Gate.code），不是内部 id。
    const gateSelect = wrapper.find('#overlay-Stand-corresponding_gate');
    expect(gateSelect.find('option[value="gate_a12"]').exists()).toBe(false);
    await gateSelect.setValue('A12');
    const gateForm = wrapper.emitted('update:standForm')?.at(-1)?.[0] as StandFormData;
    expect(gateForm.attributes).toMatchObject({ corresponding_gate: 'A12' });
  });

  it('shows size_category read-only when editing and omits it for new stands', async () => {
    const props = baseProps();
    const wrapper = mountSection(props);
    const sizeInput = wrapper.find('#stand-size');
    expect(sizeInput.exists()).toBe(true);
    expect((sizeInput.element as HTMLInputElement).disabled).toBe(true);
    expect((sizeInput.element as HTMLInputElement).value).toBe('E');

    await wrapper.setProps({
      modal: { kind: 'stand' } as DirectoryModal,
      standForm: { ...props.standForm, code: '', size_category: '', attributes: { combined_stand: false } },
    });
    expect(wrapper.find('#stand-size').exists()).toBe(false);
  });

  it('mounts the shared overlay form in the terminal / gate / carousel modals', async () => {
    const props = baseProps();
    props.terminalFieldOverlays = [
      {
        object_name: 'Terminal', field_name: 'remark', field_type: 'string',
        required: false, list_visible: false, filterable: false, visible_when: null, is_active: true,
      },
    ];
    props.gateFieldOverlays = [
      {
        object_name: 'Gate', field_name: 'bridge_type', field_type: 'string',
        required: false, list_visible: false, filterable: false, visible_when: null, is_active: true,
      },
    ];
    props.carouselFieldOverlays = [
      {
        object_name: 'BaggageCarousel', field_name: 'remark', field_type: 'string',
        required: false, list_visible: false, filterable: false, visible_when: null, is_active: true,
      },
    ];
    const wrapper = mountSection(props);

    await wrapper.setProps({ modal: { kind: 'terminal' } as DirectoryModal });
    expect(wrapper.find('#overlay-Terminal-remark').exists()).toBe(true);

    await wrapper.setProps({ modal: { kind: 'gate' } as DirectoryModal });
    expect(wrapper.find('#overlay-Gate-bridge_type').exists()).toBe(true);

    await wrapper.setProps({ modal: { kind: 'carousel' } as DirectoryModal });
    expect(wrapper.find('#overlay-BaggageCarousel-remark').exists()).toBe(true);
  });
});
