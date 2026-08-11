import type { Page } from '@playwright/test';
import { createRequire } from 'node:module';

import type { ApiRouteFixture, JsonValue } from './api.fixture';

const require = createRequire(import.meta.url);
const adminUserJson = require('../../parity/fixtures/common/auth-admin.json') as unknown;
const operatorUserJson = require('../../parity/fixtures/common/auth-operator.json') as unknown;
const readonlyUserJson = require('../../parity/fixtures/common/auth-readonly.json') as unknown;

export type AuthRole = 'admin' | 'operator' | 'readonly';

export interface AuthUserFixture {
  id: string;
  username: string;
  email: string;
  is_active: boolean;
  is_verified: boolean;
  is_admin: boolean;
  created_at: string;
  last_login_at: string | null;
  roles: string[];
  permissions: string[];
  display_name: string | null;
  effective_operator_name: string | null;
  effective_operator_label: string | null;
  operator_context_type: string | null;
  operator_context_id: string | null;
  department: string | null;
  job_level: number | null;
  job_title: string | null;
  permission_version: number;
}

export const AUTH_USERS: Readonly<Record<AuthRole, AuthUserFixture>> = Object.freeze({
  admin: adminUserJson as AuthUserFixture,
  operator: operatorUserJson as AuthUserFixture,
  readonly: readonlyUserJson as AuthUserFixture,
});

export function createAuthMeRoute(user: AuthUserFixture): ApiRouteFixture {
  return {
    id: `auth-me-${user.username}`,
    method: 'GET',
    pathname: '/api/v2/auth/me',
    query: {},
    response: {
      status: 200,
      body: user as unknown as JsonValue,
    },
  };
}

function encodeJwtPart(value: unknown): string {
  return Buffer.from(JSON.stringify(value), 'utf8').toString('base64url');
}

export function createFixtureAccessToken(user: AuthUserFixture, instant: string): string {
  const issuedAt = Math.floor(Date.parse(instant) / 1000);
  const payload = {
    sub: user.id,
    username: user.username,
    email: user.email,
    is_admin: user.is_admin,
    permissions: user.permissions,
    department: user.department,
    department_id: null,
    pv: user.permission_version,
    iat: issuedAt,
    exp: issuedAt + 3600,
    iss: 'fms-parity-fixture',
    aud: 'fms-web',
    type: 'access',
  };
  return `${encodeJwtPart({ alg: 'none', typ: 'JWT' })}.${encodeJwtPart(payload)}.fixture`;
}

export async function installLegacyAuthStorage(
  page: Page,
  user: AuthUserFixture,
  instant: string,
): Promise<void> {
  const accessToken = createFixtureAccessToken(user, instant);
  const expiresAt = Date.parse(instant) + 60 * 60 * 1000;
  await page.addInitScript(
    ({ token, expiry }) => {
      sessionStorage.setItem('access_token', token);
      sessionStorage.setItem('token_type', 'bearer');
      sessionStorage.setItem('token_expires_at', String(expiry));
      sessionStorage.setItem('session_secret', 'parity-session-secret');
    },
    { token: accessToken, expiry: expiresAt },
  );
}
