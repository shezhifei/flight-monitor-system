import { describe, it, expect } from 'vitest';
import aiConfigCenterRaw from '../AiConfigCenter.vue?raw';
import entityListRaw from '../sections/EntityListSection.vue?raw';
import capabilityEditorRaw from '../sections/CapabilityEditorSection.vue?raw';
import toolConfigRaw from '../sections/ToolConfigSection.vue?raw';
import modelRoutingRaw from '../sections/ModelRoutingSection.vue?raw';
import promptTemplateRaw from '../sections/PromptTemplateSection.vue?raw';

describe('AiConfigCenter split', () => {
  it('main file should be under 300 lines', () => {
    const lines = aiConfigCenterRaw.split('\n').length;
    expect(lines).toBeLessThan(300);
  });

  it('all section components exist', () => {
    expect(entityListRaw).toBeTruthy();
    expect(capabilityEditorRaw).toBeTruthy();
    expect(toolConfigRaw).toBeTruthy();
    expect(modelRoutingRaw).toBeTruthy();
    expect(promptTemplateRaw).toBeTruthy();
  });
});
