import { describe, it, expect } from 'vitest';
import {
  resolveCaseTypeConfig,
  resolveExtraInfoFields,
} from './helpers';
import type {
  BusinessCaseAiExtractionConfig,
  BusinessCaseProperties,
} from '../../types/backend';

function makeAiConfig(
  overrides: Partial<BusinessCaseAiExtractionConfig> = {},
): BusinessCaseAiExtractionConfig {
  return {
    enabled: true,
    utterance_session: null,
    aliases: [],
    trigger_phrases: [],
    leg_binding: { allowed: ['outbound'], default: 'outbound', required: false },
    flight_matching: {},
    fields: {
      reason: {
        type: 'text',
        label: '原因',
        required: true,
        aliases: ['原因'],
        examples: ['天气'],
        enum_values: [],
      },
    },
    forbidden_fields: [],
    examples: [],
    ...overrides,
  };
}

function makeCaseProperties(
  overrides: Partial<BusinessCaseProperties> = {},
): BusinessCaseProperties {
  return {
    auto_copilot: { utterance_final_grace_ms: null },
    ...overrides,
  };
}

describe('resolveCaseTypeConfig (Task 12c / F4)', () => {
  it('returns null when ai_extraction_config is null', () => {
    const result = resolveCaseTypeConfig(makeCaseProperties(), null);
    expect(result).toBeNull();
  });

  it('uses extra_info_schema.fields when present and non-empty', () => {
    const aiConfig = makeAiConfig();
    const caseProperties = makeCaseProperties({
      extra_info_schema: {
        fields: {
          delay_reason: {
            type: 'text',
            label: '延误原因',
            required: true,
            enum_values: ['weather', 'mechanical'],
            display_in_notification: true,
          },
        },
        summary_template: '延误：{{delay_reason}}',
      },
    });

    const result = resolveCaseTypeConfig(caseProperties, aiConfig);

    expect(result).not.toBeNull();
    expect(result!.fields['delay_reason']).toEqual({
      type: 'text',
      label: '延误原因',
      required: true,
      enum_values: ['weather', 'mechanical'],
      display_in_notification: true,
    });
    // Should NOT include the ai_extraction_config.fields entry
    expect(result!.fields['reason']).toBeUndefined();
  });

  it('falls back to ai_extraction_config.fields when extra_info_schema is absent', () => {
    const aiConfig = makeAiConfig();
    const caseProperties = makeCaseProperties();

    const result = resolveCaseTypeConfig(caseProperties, aiConfig);

    expect(result).not.toBeNull();
    expect(result!.fields['reason']).toEqual({
      type: 'text',
      label: '原因',
      required: true,
      aliases: ['原因'],
      examples: ['天气'],
      enum_values: [],
    });
  });

  it('falls back to ai_extraction_config.fields when extra_info_schema.fields is empty', () => {
    const aiConfig = makeAiConfig();
    const caseProperties = makeCaseProperties({
      extra_info_schema: { fields: {} },
    });

    const result = resolveCaseTypeConfig(caseProperties, aiConfig);

    expect(result).not.toBeNull();
    expect(result!.fields['reason']).toBeDefined();
  });

  it('preserves leg_binding from ai_extraction_config', () => {
    const aiConfig = makeAiConfig({
      leg_binding: { allowed: ['inbound', 'outbound'], default: null, required: true },
    });

    const result = resolveCaseTypeConfig(makeCaseProperties(), aiConfig);

    expect(result).not.toBeNull();
    expect(result!.leg_binding).toEqual({
      allowed: ['inbound', 'outbound'],
      default: null,
      required: true,
    });
  });

  it('spreads ai_extraction_config properties into result', () => {
    const aiConfig = makeAiConfig({
      description_template: '模板 {{reason}}',
      remarks_template: '备注',
    });

    const result = resolveCaseTypeConfig(makeCaseProperties(), aiConfig);

    expect(result).not.toBeNull();
    expect(result!.description_template).toBe('模板 {{reason}}');
    expect(result!.remarks_template).toBe('备注');
  });
});

describe('resolveExtraInfoFields (Task 12c / F4)', () => {
  it('returns extra_info_schema.fields when present and non-empty', () => {
    const caseProperties = makeCaseProperties({
      extra_info_schema: {
        fields: {
          delay_reason: { type: 'text', label: '延误原因', required: true },
        },
      },
    });

    const result = resolveExtraInfoFields(caseProperties, makeAiConfig());

    expect(result).toEqual({
      delay_reason: { type: 'text', label: '延误原因', required: true },
    });
  });

  it('falls back to ai_extraction_config.fields when extra_info_schema absent', () => {
    const aiConfig = makeAiConfig();

    const result = resolveExtraInfoFields(makeCaseProperties(), aiConfig);

    expect(result).toHaveProperty('reason');
    expect(result['reason'].required).toBe(true);
  });

  it('returns empty record when both sources are absent', () => {
    const result = resolveExtraInfoFields(null, null);
    expect(result).toEqual({});
  });

  it('returns empty record when extra_info_schema.fields is empty and ai_config is null', () => {
    const caseProperties = makeCaseProperties({
      extra_info_schema: { fields: {} },
    });

    const result = resolveExtraInfoFields(caseProperties, null);
    expect(result).toEqual({});
  });
});
