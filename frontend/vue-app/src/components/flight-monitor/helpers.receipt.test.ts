import { describe, it, expect } from 'vitest';
import { getCaseReceiptProjection } from './helpers';
import type { BusinessCaseSummary, BusinessCaseWorkflowReceiptProjection } from '@/types/backend';

// Task 12b (F3): workflow_receipt must use a single canonical shape.
// The first-class `workflow_receipt` field is the canonical source.
// The legacy `context.workflow_receipt` fallback must be removed
// to prevent dual-shape drift.

describe('getCaseReceiptProjection — single canonical shape (Task 12b / F3)', () => {
  it('returns workflow_receipt from the first-class field', () => {
    const receipt: BusinessCaseWorkflowReceiptProjection = {
      receipt_group_id: 'grp-1',
      origin_type: 'workflow',
      summary: {
        total_count: 1,
        pending_count: 0,
        acknowledged_count: 1,
        rejected_count: 0,
        is_overdue: false,
        overall_status: 'confirmed',
      },
    };
    const caseData: BusinessCaseSummary = {
      case_id: '1',
      case_type: 'gate_change',
      flight_id: 'CA123',
      flight_no: 'CA123',
      created_at: '2026-01-01T00:00:00Z',
      created_by: 'tester',
      updated_by: 'tester',
      description: '',
      status: 'open',
      context: {},
      append_count: 0,
      workflow_receipt: receipt,
    };

    const result = getCaseReceiptProjection(caseData);
    expect(result).toEqual(receipt);
  });

  it('returns null when workflow_receipt is not present on the case object', () => {
    const caseData: BusinessCaseSummary = {
      case_id: '1',
      case_type: 'gate_change',
      flight_id: 'CA123',
      flight_no: 'CA123',
      created_at: '2026-01-01T00:00:00Z',
      created_by: 'tester',
      updated_by: 'tester',
      description: '',
      status: 'open',
      context: {
        workflow_receipt: {
          receipt_group_id: 'legacy-grp',
          origin_type: 'workflow',
          summary: {
            total_count: 1,
            pending_count: 1,
            acknowledged_count: 0,
            rejected_count: 0,
            is_overdue: false,
            overall_status: 'pending',
          },
        },
      },
      append_count: 0,
      // workflow_receipt NOT set as first-class field
    };

    // Must NOT fall back to context.workflow_receipt
    const result = getCaseReceiptProjection(caseData);
    expect(result).toBeNull();
  });

  it('returns null for null/undefined caseData', () => {
    expect(getCaseReceiptProjection(null)).toBeNull();
    expect(getCaseReceiptProjection(undefined)).toBeNull();
  });

  it('returns null when workflow_receipt is explicitly null', () => {
    const caseData: BusinessCaseSummary = {
      case_id: '1',
      case_type: 'gate_change',
      flight_id: 'CA123',
      flight_no: 'CA123',
      created_at: '2026-01-01T00:00:00Z',
      created_by: 'tester',
      updated_by: 'tester',
      description: '',
      status: 'open',
      context: {},
      append_count: 0,
      workflow_receipt: null,
    };

    expect(getCaseReceiptProjection(caseData)).toBeNull();
  });
});
