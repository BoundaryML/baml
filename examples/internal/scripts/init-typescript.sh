#!/usr/bin/env bash

# Script to initialize a new TypeScript workflow from template

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if workflow name is provided
if [ -z "$1" ]; then
    echo -e "${RED}Error: Please provide a workflow name${NC}"
    echo "Usage: pnpm typescript-init <workflow_name>"
    exit 1
fi

WORKFLOW_NAME="$1"
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
BASE_DIR="$( cd "${SCRIPT_DIR}/.." && pwd )"
TEMPLATE_DIR="${BASE_DIR}/_template/typescript-demo"

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

echo -e "${BLUE}Creating TypeScript workflow '${WORKFLOW_NAME}'...${NC}"

# Create target directory
mkdir -p "$TARGET_DIR"

# Copy template files (excluding .env and other generated files)
echo "Copying template files..."
rsync -av \
    --exclude='.env' \
    --exclude='.next' \
    --exclude='node_modules' \
    --exclude='baml_client' \
    --exclude='dist' \
    --exclude='.turbo' \
    "${TEMPLATE_DIR}/" "${TARGET_DIR}/"

# Create .env file with placeholder
echo "BOUNDARY_API_KEY=sk-key-REPLACE_ME" > "${TARGET_DIR}/.env"

# Update package.json with the workflow name
if [ -f "${TARGET_DIR}/package.json" ]; then
    # Use a temporary file for the modification
    sed "s/\"name\": \".*\"/\"name\": \"${WORKFLOW_NAME}\"/" "${TARGET_DIR}/package.json" > "${TARGET_DIR}/package.json.tmp"
    mv "${TARGET_DIR}/package.json.tmp" "${TARGET_DIR}/package.json"
fi

# Update README if it exists
if [ -f "${TARGET_DIR}/README.md" ]; then
    echo "# ${WORKFLOW_NAME}

TypeScript BAML workflow

## Setup

Dependencies and BAML are already set up! Just:

\`\`\`bash
# Set your Boundary API key in .env
echo BOUNDARY_API_KEY=sk-key-YOUR_KEY > .env

# Allow direnv (if using)
direnv allow

# Run the TypeScript script
pnpm dev

# Or run the Next.js web server
pnpm web
\`\`\`

## Available Scripts

- \`pnpm dev\` - Run the TypeScript script (app/script.ts)
- \`pnpm web\` - Start Next.js development server at [http://localhost:3000](http://localhost:3000)
- \`pnpm generate\` - Generate BAML client code
- \`pnpm build\` - Build the Next.js application
- \`pnpm build:baml\` - Build BAML TypeScript client" > "${TARGET_DIR}/README.md"
fi

# Run pnpm install to set up dependencies (will also run prepare script)
echo ""
echo -e "${BLUE}Installing dependencies and setting up BAML...${NC}"
cd "${TARGET_DIR}"
pnpm install

echo ""
echo -e "${GREEN}✓ TypeScript workflow '${WORKFLOW_NAME}' created successfully!${NC}"
echo ""
echo "Next steps:"
echo -e "${BLUE}1.${NC} cd ${TARGET_DIR}"
echo -e "${BLUE}2.${NC} Set up your Boundary API key:"
echo "   echo BOUNDARY_API_KEY=sk-key-YOUR_KEY > .env"
echo -e "${BLUE}3.${NC} direnv allow  # (if using direnv)"
echo -e "${BLUE}4.${NC} pnpm dev  # Run the TypeScript script"
echo "   OR"
echo "   pnpm web  # Start the Next.js web server"