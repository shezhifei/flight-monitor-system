import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import FieldOverlayForm from './FieldOverlayForm.vue';
import type { FieldOverlay } from '@/composables/useFieldOverlays';

const overlays: FieldOverlay[] = [
  {
    object_name: 'DispatchOrder', field_name: 'priority_band', field_type: 'catalog_ref',
    catalog_code: 'priority_band', required: true, list_visible: true, filterable: true,
    visible_when: null, is_active: true,
  },
  {
    object_name: 'DispatchOrder', field_name: 'requires_escort', field_type: 'boolean',
    required: false, list_visible: false, filterable: false, visible_when: null, is_active: true,
  },
  {
    object_name: 'DispatchOrder', field_name: 'escort_count', field_type: 'number',
    required: false, list_visible: false, filterable: false,
    // Server shape: field_overlay_service::validate_visible_when /
    // attribute_validation::field_is_visible only accept { field, op, value }.
    visible_when: { field: 'requires_escort', op: 'eq', value: true }, is_active: true,
  },
  {
    object_name: 'DispatchOrder', field_name: 'related_equipment', field_type: 'object_ref[]',
    object_name_target: 'Equipment', required: false, list_visible: false, filterable: false,
    visible_when: null, is_active: true,
  },
  {
    object_name: 'DispatchOrder', field_name: 'supervisor', field_type: 'object_ref',
    object_name_target: 'Personnel', required: false, list_visible: false, filterable: false,
    visible_when: null, is_active: true,
  },
];

const standOverlays: FieldOverlay[] = [
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
  {
    object_name: 'Stand', field_name: 'backup_stands', field_type: 'object_ref',
    object_name_target: 'Stand', required: false, list_visible: false, filterable: false,
    visible_when: null, is_active: true,
  },
];

function formGroupStyle(wrapper: ReturnType<typeof mount>, controlId: string): string {
  const control = wrapper.find(`#${controlId}`);
  return control.element.parentElement?.getAttribute('style') ?? '';
}

describe('FieldOverlayForm', () => {
  it('renders catalog and boolean controls and emits typed updates', async () => {
    const wrapper = mount(FieldOverlayForm, {
      props: {
        modelValue: {},
        overlays,
        catalogEntries: { priority_band: [{ code: 'urgent', name: '紧急' }] },
      },
    });

    const select = wrapper.get('select');
    expect(select.find('option[value="urgent"]').exists()).toBe(true);
    await select.setValue('urgent');
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toEqual({ priority_band: 'urgent' });

    const checkbox = wrapper.get('input[type="checkbox"]');
    await checkbox.setValue(true);
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toEqual({ requires_escort: true });
  });

  it('honors { field, op, value } conditions and converts number input', async () => {
    const wrapper = mount(FieldOverlayForm, {
      props: { modelValue: { requires_escort: true }, overlays },
    });

    const number = wrapper.get('input[type="number"]');
    await number.setValue('3');
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toEqual({ requires_escort: true, escort_count: 3 });

    await wrapper.setProps({ modelValue: { requires_escort: false } });
    expect(formGroupStyle(wrapper, 'overlay-DispatchOrder-escort_count')).toContain('display: none');
  });

  it('treats a legacy flat-map visible_when as always visible', () => {
    // 非法形状：服务端 validate_visible_when 只接受 { field, op, value }，
    // 旧扁平 map 不再是契约的一部分。前端将其视为始终可见（与服务端
    // field_is_visible 对无法解析条件的宽容行为一致），而不是报错或隐藏。
    const legacy: FieldOverlay[] = [
      {
        object_name: 'DispatchOrder', field_name: 'escort_count', field_type: 'number',
        required: false, list_visible: false, filterable: false,
        visible_when: { requires_escort: true } as unknown as FieldOverlay['visible_when'],
        is_active: true,
      },
    ];
    const wrapper = mount(FieldOverlayForm, { props: { modelValue: { requires_escort: false }, overlays: legacy } });

    expect(formGroupStyle(wrapper, 'overlay-DispatchOrder-escort_count')).not.toContain('display: none');
  });

  it('hides composed_of unless combined_stand is true and hides the gate for remote stands', async () => {
    const wrapper = mount(FieldOverlayForm, {
      props: {
        modelValue: { combined_stand: false, stand_use: 'near_domestic' },
        overlays: standOverlays,
        catalogEntries: { stand_use: [{ code: 'remote', name: '远机位' }, { code: 'near_domestic', name: '近机位国内' }] },
        referenceEntries: {
          Stand: [{ id: 'stand-uuid-1', code: '316L', name: '316L' }],
          Gate: [{ id: 'gate-uuid-1', code: 'A12', name: 'A12' }],
        },
      },
    });

    expect(formGroupStyle(wrapper, 'overlay-Stand-composed_of')).toContain('display: none');
    expect(formGroupStyle(wrapper, 'overlay-Stand-corresponding_gate')).not.toContain('display: none');

    await wrapper.setProps({ modelValue: { combined_stand: true, stand_use: 'remote' } });
    expect(formGroupStyle(wrapper, 'overlay-Stand-composed_of')).not.toContain('display: none');
    expect(formGroupStyle(wrapper, 'overlay-Stand-corresponding_gate')).toContain('display: none');
  });

  it('renders catalog_ref[] as a multi-select and emits a deduped array', async () => {
    const multi: FieldOverlay[] = [
      {
        object_name: 'Stand', field_name: 'standby_codes', field_type: 'catalog_ref[]',
        catalog_code: 'stand_use', required: false, list_visible: false, filterable: false,
        visible_when: null, is_active: true,
      },
    ];
    const wrapper = mount(FieldOverlayForm, {
      props: {
        modelValue: {},
        overlays: multi,
        catalogEntries: { stand_use: [{ code: 'remote', name: '远机位' }, { code: 'near_domestic', name: '近机位国内' }] },
      },
    });

    const select = wrapper.get('select');
    expect(select.attributes('multiple')).toBeDefined();
    await select.setValue(['remote', 'near_domestic']);
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toEqual({
      standby_codes: ['remote', 'near_domestic'],
    });
  });

  it('normalizes object reference arrays and shows the target object', async () => {
    const wrapper = mount(FieldOverlayForm, {
      props: { modelValue: {}, overlays },
    });

    expect(wrapper.text()).toContain('引用对象：Equipment');
    const textarea = wrapper.get('textarea');
    await textarea.setValue('eq-1, eq-2\neq-1');
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toEqual({
      related_equipment: ['eq-1', 'eq-2'],
    });
  });

  it('removes a cleared optional singular object reference', async () => {
    const wrapper = mount(FieldOverlayForm, {
      props: { modelValue: { supervisor: 'user-1' }, overlays },
    });

    const inputs = wrapper.findAll('input[type="text"]');
    const supervisor = inputs.at(-1);
    await supervisor?.setValue('');
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toEqual({});
  });

  it('uses the business key (code ?? id) as the reference option value', async () => {
    const wrapper = mount(FieldOverlayForm, {
      props: {
        modelValue: {},
        overlays,
        referenceEntries: {
          Personnel: [{ id: 'user-1', name: '值班主管' }],
        },
      },
    });

    const selects = wrapper.findAll('select');
    const personnel = selects.find(select => select.find('option[value="user-1"]').exists());
    expect(personnel).toBeDefined();
    await personnel?.setValue('user-1');
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toMatchObject({ supervisor: 'user-1' });
  });

  it('prefers the code over the id for objects that have one', async () => {
    const wrapper = mount(FieldOverlayForm, {
      props: {
        modelValue: {},
        overlays: standOverlays,
        referenceEntries: {
          Stand: [{ id: 'stand-uuid-1', code: '316', name: '316 号机位' }],
          Gate: [{ id: 'gate-uuid-1', code: 'A12', name: 'A12' }],
        },
      },
    });

    const gate = wrapper.find('select#overlay-Stand-corresponding_gate');
    expect(gate.find('option[value="gate-uuid-1"]').exists()).toBe(false);
    await gate.setValue('A12');
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toMatchObject({ corresponding_gate: 'A12' });

    const backup = wrapper.find('select#overlay-Stand-backup_stands');
    await backup.setValue('316');
    expect(wrapper.emitted('update:modelValue')?.at(-1)?.[0]).toMatchObject({ backup_stands: '316' });
  });
});
