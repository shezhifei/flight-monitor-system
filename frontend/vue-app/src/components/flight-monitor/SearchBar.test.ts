import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import SearchBar from './SearchBar.vue';
import type { SearchFields } from '../../composables/useFlightData';

const defaultSearchFields: SearchFields = {
  searchFlightNo: true,
  searchDestination: true,
  searchDestinationName: true,
  searchOrigin: true,
  searchOriginName: true,
  searchStatus: true,
  searchAircraftType: false,
  searchStand: false,
  searchGate: false,
  searchMission: true,
  searchFlightType: true,
};

const defaultProps = {
  viewMode: 'card' as const,
  searchQuery: '',
  searchFields: defaultSearchFields,
  searchOptionsExpanded: false,
  businessFilterExpanded: false,
  visibleCount: 50,
  totalCount: 100,
  hasSelectedFlight: false,
  canClearFilters: false,
};

function mountSearchBar(overrides: Record<string, unknown> = {}) {
  return mount(SearchBar, { props: { ...defaultProps, ...overrides } });
}

describe('SearchBar', () => {
  it('renders search input with placeholder', () => {
    const wrapper = mountSearchBar();
    const input = wrapper.find('#searchInput');
    expect(input.attributes('placeholder')).toContain('搜索');
  });

  it('displays visible/total count pill', () => {
    const wrapper = mountSearchBar({ visibleCount: 25, totalCount: 80 });
    expect(wrapper.find('#flightResultPill').text()).toBe('当前显示 25/80');
  });

  it('emits update:searchQuery on input', async () => {
    const wrapper = mountSearchBar();
    await wrapper.find('#searchInput').setValue('CA1234');
    expect(wrapper.emitted('update:searchQuery')).toBeTruthy();
    expect(wrapper.emitted('update:searchQuery')![0]).toEqual(['CA1234']);
  });

  it('emits submit-search on Enter key', async () => {
    const wrapper = mountSearchBar();
    await wrapper.find('#searchInput').trigger('keydown.enter');
    expect(wrapper.emitted('submit-search')).toHaveLength(1);
  });

  it('emits submit-search when search button clicked', async () => {
    const wrapper = mountSearchBar();
    await wrapper.find('#searchBtn').trigger('click');
    expect(wrapper.emitted('submit-search')).toHaveLength(1);
  });

  it('emits clear-search when clear button clicked', async () => {
    const wrapper = mountSearchBar({ searchQuery: 'test' });
    await wrapper.find('#clearSearchBtn').trigger('click');
    expect(wrapper.emitted('clear-search')).toHaveLength(1);
  });

  it('emits update:viewMode when card view button clicked', async () => {
    const wrapper = mountSearchBar({ viewMode: 'table' });
    await wrapper.find('#viewCardBtn').trigger('click');
    expect(wrapper.emitted('update:viewMode')![0]).toEqual(['card']);
  });

  it('emits update:viewMode when table view button clicked', async () => {
    const wrapper = mountSearchBar({ viewMode: 'card' });
    await wrapper.find('#viewTableBtn').trigger('click');
    expect(wrapper.emitted('update:viewMode')![0]).toEqual(['table']);
  });

  it('emits toggle-search-options when options toggle clicked', async () => {
    const wrapper = mountSearchBar();
    await wrapper.find('#searchOptionsToggle').trigger('click');
    expect(wrapper.emitted('toggle-search-options')).toHaveLength(1);
  });

  it('emits toggle-business-filters when filter toggle clicked', async () => {
    const wrapper = mountSearchBar();
    await wrapper.find('#businessFilterToggle').trigger('click');
    expect(wrapper.emitted('toggle-business-filters')).toHaveLength(1);
  });

  it('disables focus-selected-flight button when no flight selected', () => {
    const wrapper = mountSearchBar({ hasSelectedFlight: false });
    expect(wrapper.find('#focusSelectedFlightBtn').attributes('disabled')).toBeDefined();
  });

  it('enables focus-selected-flight button when flight selected', () => {
    const wrapper = mountSearchBar({ hasSelectedFlight: true });
    expect(wrapper.find('#focusSelectedFlightBtn').attributes('disabled')).toBeUndefined();
  });

  it('emits focus-selected-flight when button clicked', async () => {
    const wrapper = mountSearchBar({ hasSelectedFlight: true });
    await wrapper.find('#focusSelectedFlightBtn').trigger('click');
    expect(wrapper.emitted('focus-selected-flight')).toHaveLength(1);
  });

  it('emits clear-all-filters when button clicked', async () => {
    const wrapper = mountSearchBar({ canClearFilters: true });
    await wrapper.find('#clearAllFiltersBtn').trigger('click');
    expect(wrapper.emitted('clear-all-filters')).toHaveLength(1);
  });

  it('shows search options panel when expanded', () => {
    const wrapper = mountSearchBar({ searchOptionsExpanded: true });
    expect(wrapper.find('.search-options-panel').classes()).toContain('expanded');
  });

  it('hides search options panel when collapsed', () => {
    const wrapper = mountSearchBar({ searchOptionsExpanded: false });
    expect(wrapper.find('.search-options-panel').classes()).not.toContain('expanded');
  });
});
