#!/usr/bin/env bash

# Script to initialize a new Python workflow from template

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if workflow name is provided
if [ -z "$1" ]; then
    echo -e "${RED}Error: Please provide a workflow name${NC}"
    echo "Usage: pnpm python-init <workflow_name>"
    exit 1
fi

WORKFLOW_NAME="$1"
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
BASE_DIR="$( cd "${SCRIPT_DIR}/.." && pwd )"
TEMPLATE_DIR="${BASE_DIR}/_template/python-demo"

# Get the username from the current directory or ask for it
# When run via pnpm, INIT_CWD contains the directory where pnpm was invoked
if [ -n "$INIT_CWD" ]; then
    CURRENT_DIR="$INIT_CWD"
else
    CURRENT_DIR=$(pwd)
fi

RELATIVE_DIR=$(realpath --relative-to="${BASE_DIR}" "${CURRENT_DIR}" 2>/dev/null || echo "")

# Check if we're in a user directory
USER_DIRS=("aaron" "antonio" "greg" "sam" "vaibhav")
CURRENT_FOLDER=$(basename "$CURRENT_DIR")

if [[ " ${USER_DIRS[@]} " =~ " ${CURRENT_FOLDER} " ]]; then
    # We're in a user directory
    TARGET_DIR="${CURRENT_DIR}/${WORKFLOW_NAME}"
elif [[ "$CURRENT_DIR" == "$BASE_DIR" ]]; then
    # We're in the base internal directory, prompt for user folder
    echo -e "${BLUE}Which folder do you want to create the workflow in?${NC}"
    echo "Available folders: ${USER_DIRS[*]}"
    read -p "Enter folder name: " USER_FOLDER
    TARGET_DIR="${BASE_DIR}/${USER_FOLDER}/${WORKFLOW_NAME}"
else
    # Default to current directory
    TARGET_DIR="${CURRENT_DIR}/${WORKFLOW_NAME}"
fi

# Check if target directory already exists
if [ -d "$TARGET_DIR" ]; then
    echo -e "${RED}Error: Directory ${TARGET_DIR} already exists${NC}"
    exit 1
fi

# Check if template directory exists
if [ ! -d "$TEMPLATE_DIR" ]; then
    echo -e "${RED}Error: Template directory not found at ${TEMPLATE_DIR}${NC}"
    exit 1
fi

echo -e "${BLUE}Creating Python workflow '${WORKFLOW_NAME}'...${NC}"

# Create target directory
mkdir -p "$TARGET_DIR"

# Copy template files (excluding .env and other generated files)
echo "Copying template files..."
rsync -av \
    --exclude='.env' \
    --exclude='.venv' \
    --exclude='__pycache__' \
    --exclude='*.pyc' \
    --exclude='.pytest_cache' \
    --exclude='baml_client' \
    --exclude='.next' \
    --exclude='node_modules' \
    "${TEMPLATE_DIR}/" "${TARGET_DIR}/"

# Create .env file with placeholder
echo "BOUNDARY_API_KEY=sk-key-REPLACE_ME" > "${TARGET_DIR}/.env"

# Update package.json with the workflow name
if [ -f "${TARGET_DIR}/package.json" ]; then
    # Use a temporary file for the modification
    sed "s/\"name\": \".*\"/\"name\": \"${WORKFLOW_NAME}\"/" "${TARGET_DIR}/package.json" > "${TARGET_DIR}/package.json.tmp"
    mv "${TARGET_DIR}/package.json.tmp" "${TARGET_DIR}/package.json"
fi

# Update pyproject.toml with the workflow name
if [ -f "${TARGET_DIR}/pyproject.toml" ]; then
    # Use a temporary file for the modification
    sed "s/^name = \".*\"/name = \"${WORKFLOW_NAME}\"/" "${TARGET_DIR}/pyproject.toml" > "${TARGET_DIR}/pyproject.toml.tmp"
    mv "${TARGET_DIR}/pyproject.toml.tmp" "${TARGET_DIR}/pyproject.toml"
fi

# Update README if it exists
if [ -f "${TARGET_DIR}/README.md" ]; then
    echo "# ${WORKFLOW_NAME}

Python BAML workflow

## Setup

\`\`\`bash
# Set your Boundary API key in .env
echo BOUNDARY_API_KEY=sk-key-YOUR_KEY > .env

# Allow direnv (if using)
direnv allow

# Set up everything (install deps, build BAML, generate)
pnpm setup

# Run the workflow
pnpm dev
\`\`\`

## Available Scripts

- \`pnpm dev\` - Run the Python workflow
- \`pnpm setup\` - One-liner setup (sync + build:baml + generate)
- \`pnpm sync\` - Install/update Python dependencies
- \`pnpm build:baml\` - Build BAML Python client
- \`pnpm generate\` - Generate BAML client code
- \`pnpm test\` - Run tests
- \`pnpm typecheck\` - Run type checking with mypy" > "${TARGET_DIR}/README.md"
fi

# Run pnpm install to set up dependencies (skip prepare script on initial install)
echo ""
echo -e "${BLUE}Installing dependencies...${NC}"
cd "${TARGET_DIR}"
pnpm install --ignore-scripts

echo ""
echo -e "${GREEN}✓ Python workflow '${WORKFLOW_NAME}' created successfully!${NC}"
echo ""
echo "Next steps:"
echo -e "${BLUE}1.${NC} cd ${TARGET_DIR}"
echo -e "${BLUE}2.${NC} Set up your Boundary API key:"
echo "   echo BOUNDARY_API_KEY=sk-key-YOUR_KEY > .env"
echo -e "${BLUE}3.${NC} direnv allow  # (if using direnv)"
echo -e "${BLUE}4.${NC} pnpm setup  # Install dependencies and set up BAML"
echo -e "${BLUE}5.${NC} pnpm dev  # Run the workflow"