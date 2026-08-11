-- =====================================================
-- Migration 059: Add JSONB expression indexes for
-- policy_interception error_code lookups
-- =====================================================

-- This project executes migrations inside a transaction, so avoid CONCURRENTLY here.
CREATE INDEX IF NOT EXISTS idx_fbc_policy_interception_error_code
    ON flight_business_cases ((context->>'error_code'))
    WHERE case_type = 'policy_interception';

CREATE INDEX IF NOT EXISTS idx_afbc_policy_interception_error_code
    ON archived_flight_business_cases ((context->>'error_code'))
    WHERE case_type = 'policy_interception';
