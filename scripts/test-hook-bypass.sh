#!/bin/bash

# Script to demonstrate the three ways to bypass the pre-commit hook

echo "=== Testing Git Hook Bypass Methods ==="
echo ""
echo "This script demonstrates the three ways to bypass the pre-commit hook."
echo "Note: These are dry-runs and won't actually create commits."
echo ""

# Color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}Method 1: Using --no-verify flag${NC}"
echo "Command: git commit --no-verify -m \"WIP: experimental changes\""
echo "This completely bypasses the pre-commit hook"
echo ""

echo -e "${BLUE}Method 2: Adding [skip-checks] to commit message${NC}"
echo "Command: git commit -m \"WIP: testing something [skip-checks]\""
echo "The hook will detect [skip-checks] in the message and skip"
echo ""

echo -e "${BLUE}Method 3: Setting SKIP_CHECKS environment variable${NC}"
echo "Command: SKIP_CHECKS=1 git commit -m \"WIP: quick save\""
echo "The hook checks for this environment variable"
echo ""

echo -e "${GREEN}Summary of escape hatches:${NC}"
echo "  1. ${YELLOW}--no-verify${NC}: Completely bypasses all hooks (fastest)"
echo "  2. ${YELLOW}[skip-checks]${NC}: Add to commit message (most visible in git log)"
echo "  3. ${YELLOW}SKIP_CHECKS=1${NC}: Set environment variable (cleanest commit message)"
echo ""
echo "All three methods will allow you to commit without running fmt or clippy."