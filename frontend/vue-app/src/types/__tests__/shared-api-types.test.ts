import { describe, it, expectTypeOf } from 'vitest';
import type { ApiErrorResponse, ApiSuccessResponse, ChatMessageItem } from '../shared-api-types';

describe('shared-api-types', () => {
  it('ApiSuccessResponse has ok=true and data', () => {
    expectTypeOf<ApiSuccessResponse<{ id: string }>>().toMatchTypeOf<{ ok: true; data: { id: string } }>();
  });

  it('ApiErrorResponse has ok=false and error message', () => {
    expectTypeOf<ApiErrorResponse>().toMatchTypeOf<{ ok: false; error: string; status: number }>();
  });

  it('ChatMessageItem has required fields', () => {
    expectTypeOf<ChatMessageItem>().toHaveProperty('id').toEqualTypeOf<string>();
  });
});
