import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import BusinessFilter from './BusinessFilter.vue';

import type { BusinessFilters } from '../../composables/useFlightData';

const defaultFilters: BusinessFilters = {
  anomalyFilter: 'all',
  delayFilter: 'all',
  vipFilter: 'all',
  aircraftBodyFilter: 'all',
  commercialSignedFilter: 'all',
  quickTurnFilter: 'all',
};

function mountFilter(overrides: Record<string, unknown> = {}) {
  return mount(BusinessFilter, {
    props: {
      filters: { ...defaultFilters },
      expanded: true,
      anomalyCount: 3,
      delayCount: 1,
      vipCount: 2,
      quickTurnCount: 4,
      resetVisible: false,
      ...overrides,
    },
  });
}

describe('BusinessFilter', () => {
  it('renders all six filter selects', () => {
    const wrapper = mountFilter();
    expect(wrapper.findAll('select')).toHaveLength(6);
  });

  it('displays counts next to labels', () => {
    const wrapper = mountFilter();
    expect(wrapper.find('#anomalyFilterCount').text()).toBe('3');
    expect(wrapper.find('#delayFilterCount').text()).toBe('1');
    expect(wrapper.find('#vipFilterCount').text()).toBe('2');
    expect(wrapper.find('#quickTurnFilterCount').text()).toBe('4');
  });

  it('emits set-filter when anomaly select changes', async () => {
    const wrapper = mountFilter();
    await wrapper.find('#anomalyFilter').setValue('only');
    expect(wrapper.emitted('set-filter')).toBeTruthy();
    expect(wrapper.emitted('set-filter')![0]).toEqual(['anomalyFilter', 'only']);
  });

  it('emits set-filter when delay select changes', async () => {
    const wrapper = mountFilter();
    await wrapper.find('#delayFilter').setValue('only');
    expect(wrapper.emitted('set-filter')![0]).toEqual(['delayFilter', 'only']);
  });

  it('emits set-filter when aircraft body select changes', async () => {
    const wrapper = mountFilter();
    await wrapper.find('#aircraftBodyFilter').setValue('wide');
    expect(wrapper.emitted('set-filter')![0]).toEqual(['aircraftBodyFilter', 'wide']);
  });

  it('shows reset button when resetVisible is true', () => {
    const wrapper = mountFilter({ resetVisible: true });
    const btn = wrapper.find('#resetBusinessFiltersBtn');
    expect(btn.classes()).not.toContain('is-hidden');
  });

  it('hides reset button when resetVisible is false', () => {
    const wrapper = mountFilter({ resetVisible: false });
    const btn = wrapper.find('#resetBusinessFiltersBtn');
    expect(btn.classes()).toContain('is-hidden');
  });

  it('emits reset when reset button clicked', async () => {
    const wrapper = mountFilter({ resetVisible: true });
    await wrapper.find('#resetBusinessFiltersBtn').trigger('click');
    expect(wrapper.emitted('reset')).toHaveLength(1);
  });

  it('applies expanded class when expanded prop is true', () => {
    const wrapper = mountFilter({ expanded: true });
    expect(wrapper.find('.business-filter-bar').classes()).toContain('expanded');
  });

  it('does not apply expanded class when expanded prop is false', () => {
    const wrapper = mountFilter({ expanded: false });
    expect(wrapper.find('.business-filter-bar').classes()).not.toContain('expanded');
  });
});
