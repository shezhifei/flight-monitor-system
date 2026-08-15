#!/bin/bash
# .git/hooks/pre-commit
# 
# Pre-commit hook for golden test structural validation
# Runs quick check before allowing commit to ensure test files are valid
#
# Usage: Copy this file to .git/hooks/pre-commit and make it executable
#   chmod +x .git/hooks/pre-commit

set -e

echo "🔍 Running quick golden test structural validation..."

# Check if golden tests directory exists
if [ ! -d "../eval/datasets" ]; then
    echo "⚠️  Warning: Golden test datasets directory not found"
    echo "   Skipping pre-commit validation"
    exit 0
fi

# Count total tests
TEST_COUNT=$(cat ../eval/datasets/*.jsonl 2>/dev/null | wc -l)

if [ "$TEST_COUNT" -eq 0 ]; then
    echo "⚠️  No golden tests found yet"
    echo "   This is okay for initial setup, but plan to add tests soon"
    exit 0
fi

echo "📊 Found $TEST_COUNT golden test cases"

# Run quick validation (MOCK mode, very permissive)
cd scripts
if python agent_validator.py --mode MOCK 2>&1 | grep -q "PASS"; then
    echo "✅ Golden test structure validation passed"
    exit 0
else
    echo "❌ Golden test validation failed!"
    echo ""
    echo "Please fix any issues with your test definitions:"
    echo "  - Ensure user_query is present in inputs"
    echo "  - Define required tools in expected_behavior"
    echo "  - Add evidence requirements"
    echo ""
    echo "Run 'python agent_validator.py' directly for details"
    exit 1
fi
