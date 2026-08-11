import { describe, expect, it } from 'vitest';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

const schemaRoot = path.resolve(process.cwd(), 'parity/fixtures/schema');

function readJson(filename: string): unknown {
  return JSON.parse(readFileSync(path.join(schemaRoot, filename), 'utf8'));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function unwrapData(body: unknown): unknown {
  if (!isRecord(body)) return body;
  if ('success' in body && 'data' in body) return body.data;
  return body;
}

describe('api contract parity schema fixtures', () => {
  it('keeps a schema catalog generated from page fixtures', () => {
    expect(existsSync(path.join(schemaRoot, 'catalog.json'))).toBe(true);
    expect(existsSync(path.join(schemaRoot, 'README.md'))).toBe(true);
    const catalog = readJson('catalog.json') as { count: number; routes: Array<{ method: string; pathname: string }> };
    expect(catalog.count).toBeGreaterThan(20);
    expect(catalog.routes.length).toBe(catalog.count);
  });

  it('uses Rust AnomalyResponse field names in the anomalies list fixture', () => {
    const fixture = readJson('get__api__v2__anomalies.json') as {
      body: { data?: { items?: Array<Record<string, unknown>> } };
    };
    const items = fixture.body?.data?.items ?? [];
    expect(items.length).toBeGreaterThan(0);
    const item = items[0];
    for (const key of [
      'anomaly_id',
      'flight_id',
      'anomaly_type',
      'severity',
      'status',
      'title',
      'detected_at',
      'escalation_level',
      'created_at',
      'updated_at',
    ]) {
      expect(item).toHaveProperty(key);
    }
    expect(item).not.toHaveProperty('id');
    expect(item).not.toHaveProperty('detection_time');
    expect(item).not.toHaveProperty('type');
  });

  it('uses Rust system flag path/label/masked fields', () => {
    const fixture = readJson('get__api__v2__system__flags.json') as {
      body: { data?: { flags?: Array<Record<string, unknown>> } };
    };
    const flags = fixture.body?.data?.flags ?? [];
    expect(flags.length).toBeGreaterThan(0);
    for (const flag of flags) {
      expect(flag).toHaveProperty('path');
      expect(flag).toHaveProperty('label');
      expect(flag).toHaveProperty('value');
      expect(flag).toHaveProperty('type');
      expect(flag).toHaveProperty('category');
      expect(flag).toHaveProperty('masked');
      expect(flag).not.toHaveProperty('key');
      expect(flag).not.toHaveProperty('name');
    }
  });

  it('uses Rust UserResponse / RoleResponse / PermissionResponse field names', () => {
    const users = unwrapData((readJson('get__api__v2__auth__users.json') as { body: unknown }).body);
    const roles = unwrapData((readJson('get__api__v2__auth__roles.json') as { body: unknown }).body);
    const permissions = unwrapData((readJson('get__api__v2__auth__permissions.json') as { body: unknown }).body);

    const userList = Array.isArray(users) ? users : [];
    const roleList = Array.isArray(roles) ? roles : [];
    const permissionList = Array.isArray(permissions) ? permissions : [];

    expect(userList.length).toBeGreaterThan(0);
    expect(roleList.length).toBeGreaterThan(0);
    expect(permissionList.length).toBeGreaterThan(0);

    const user = userList[0] as Record<string, unknown>;
    expect(user).toHaveProperty('last_login_at');
    expect(Array.isArray(user.roles)).toBe(true);
    expect(user).not.toHaveProperty('last_login');

    const role = roleList[0] as Record<string, unknown>;
    expect(role).toHaveProperty('name');
    expect(Array.isArray(role.permissions)).toBe(true);

    const permission = permissionList[0] as Record<string, unknown>;
    expect(permission).toHaveProperty('name');
    expect(permission).toHaveProperty('is_active');
    expect(permission).not.toHaveProperty('code');
  });

  it('rejects empty-object/empty-array silent success as coverage for required list endpoints', () => {
    const required = [
      'get__api__v2__anomalies.json',
      'get__api__v2__system__flags.json',
      'get__api__v2__auth__users.json',
    ];
    for (const filename of required) {
      const fixture = readJson(filename) as { body: unknown };
      const data = unwrapData(fixture.body);
      if (isRecord(data) && Array.isArray(data.items)) {
        expect(data.items.length).toBeGreaterThan(0);
        continue;
      }
      if (isRecord(data) && Array.isArray(data.flags)) {
        expect(data.flags.length).toBeGreaterThan(0);
        continue;
      }
      expect(Array.isArray(data) && data.length > 0).toBe(true);
    }
  });

  it('stores only JSON schema fixtures under parity/fixtures/schema', () => {
    const files = readdirSync(schemaRoot).filter((name) => !name.startsWith('.'));
    for (const file of files) {
      if (file === 'README.md') continue;
      expect(file.endsWith('.json')).toBe(true);
    }
  });
});
