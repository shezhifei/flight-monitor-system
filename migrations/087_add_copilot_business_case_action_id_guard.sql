-- Migration: 087_add_copilot_business_case_action_id_guard
-- Description: Prevent duplicate AI Copilot business cases for the same confirmed draft action.

CREATE UNIQUE INDEX IF NOT EXISTS idx_flight_business_cases_copilot_batch_action
    ON flight_business_cases (
        (context->>'copilot_batch_id'),
        (context->>'copilot_action_id')
    )
    WHERE context->>'source' = 'ai_copilot_voice'
      AND context ? 'copilot_batch_id'
      AND context ? 'copilot_action_id';
