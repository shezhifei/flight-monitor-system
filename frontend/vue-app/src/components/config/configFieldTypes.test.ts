import { describe, expect, it } from 'vitest';
import {
  cleanConfigDescription,
  humanizeConfigPath,
  isSensitiveConfigPath,
} from './configFieldTypes';

describe('configFieldTypes', () => {
  it('humanizes path tail', () => {
    expect(humanizeConfigPath('ai.query.db.host')).toBe('Host');
    expect(humanizeConfigPath('ai.config.encryption.key')).toBe('Key');
  });

  it('drops template descriptions', () => {
    expect(cleanConfigDescription('Configuration for ai.query.db.host', 'ai.query.db.host')).toBeUndefined();
    expect(cleanConfigDescription('真实说明', 'x')).toBe('真实说明');
  });

  it('detects sensitive paths', () => {
    expect(isSensitiveConfigPath('ai.query.db.password')).toBe(true);
    expect(isSensitiveConfigPath('ai.config.encryption.key')).toBe(true);
    expect(isSensitiveConfigPath('ai.query.db.host')).toBe(false);
  });
});
