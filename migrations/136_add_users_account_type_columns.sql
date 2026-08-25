-- PR #本体两层改造 - Alter users table for Position/Personal account separation
-- As specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md

SET TRANSACTION READ WRITE;

-- ============================================================================
-- Add new columns to users table
-- ============================================================================

-- Add account_type column (personal | position)
ALTER TABLE users ADD COLUMN IF NOT EXISTS account_type VARCHAR(20) DEFAULT 'personal';

-- Add login_enabled column (position accounts have login_enabled=false)
ALTER TABLE users ADD COLUMN IF NOT EXISTS login_enabled BOOLEAN DEFAULT TRUE;

-- Add current_occupant_user_id column for seat occupation tracking
-- This tracks which personal user currently occupies a position seat
ALTER TABLE users ADD COLUMN IF NOT EXISTS current_occupant_user_id VARCHAR(64);

-- ============================================================================
-- Apply business constraints
-- ============================================================================

-- Position accounts cannot be admins (business rule enforcement)
ALTER TABLE users 
    ADD CONSTRAINT check_position_not_admin 
    CHECK (account_type != 'position' OR is_admin IS NULL);

-- Login enabled default for positions should be FALSE
UPDATE users SET login_enabled = FALSE WHERE account_type = 'position';

-- ============================================================================
-- Migrate existing users
-- ============================================================================

-- All existing users default to 'personal' account type
-- Migration strategy:
-- 1. Personal accounts: account_type='personal', login_enabled=TRUE, no current_occupant_user_id
-- 2. Position accounts: Will be created NEW via User Management UI (not migrated)
-- Existing users are automatically 'personal'

-- ============================================================================
-- Create index for current_occupant_user_id
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_users_occupant ON users(current_occupant_user_id) 
WHERE current_occupant_user_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_users_account_type ON users(account_type);

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON COLUMN users.account_type IS 'Account type: personal (human user) | position (seat/stable recipient, not loginable)';
COMMENT ON COLUMN users.login_enabled IS 'Whether this account can login (positions have FALSE)';
COMMENT ON COLUMN users.current_occupant_user_id IS 'For position accounts: tracks which personal user currently occupies this seat after OccupySeat';

COMMENT ON CONSTRAINT check_position_not_admin ON users IS 
    'Business rule: Position accounts (seats) cannot have admin privileges';

-- ============================================================================
-- Rollback notes
-- ============================================================================

/*
To rollback these changes:
ALTER TABLE users DROP COLUMN IF EXISTS current_occupant_user_id;
ALTER TABLE users DROP COLUMN IF EXISTS login_enabled;
ALTER TABLE users DROP COLUMN IF EXISTS account_type;
*/
