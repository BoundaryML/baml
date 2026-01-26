#!/bin/bash
set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo "🚀 Deploying BEPs Infrastructure with AWS CDK"
echo "=============================================="
echo ""

# Check if Node.js is installed
if ! command -v node &> /dev/null; then
    echo -e "${RED}❌ Node.js is not installed${NC}"
    echo "Please install Node.js 18 or later"
    exit 1
fi

echo -e "${GREEN}✓ Node.js $(node --version)${NC}"

# Check if AWS CLI is installed
if ! command -v aws &> /dev/null; then
    echo -e "${RED}❌ AWS CLI is not installed${NC}"
    echo "Please install AWS CLI"
    exit 1
fi

echo -e "${GREEN}✓ AWS CLI installed${NC}"

# Check AWS credentials
if ! aws sts get-caller-identity &> /dev/null; then
    echo -e "${RED}❌ AWS credentials not configured or expired${NC}"
    echo ""
    echo "Options:"
    echo "  1. AWS SSO: aws sso login --profile your-profile"
    echo "  2. Static credentials: aws configure"
    echo "  3. Set AWS_PROFILE: export AWS_PROFILE=your-profile"
    echo ""
    exit 1
fi

ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
CURRENT_USER=$(aws sts get-caller-identity --query Arn --output text 2>/dev/null || echo "")
echo -e "${GREEN}✓ AWS Account: $ACCOUNT_ID${NC}"
if [ ! -z "$AWS_PROFILE" ]; then
    echo -e "${GREEN}✓ Profile: $AWS_PROFILE${NC}"
fi
if [ ! -z "$CURRENT_USER" ]; then
    echo -e "${GREEN}✓ Identity: $CURRENT_USER${NC}"
fi
echo ""

# Install dependencies
if [ ! -d "node_modules" ]; then
    echo "📦 Installing dependencies..."
    npm install
    echo -e "${GREEN}✓ Dependencies installed${NC}"
    echo ""
else
    echo -e "${GREEN}✓ Dependencies already installed${NC}"
    echo ""
fi

# Check for environment variables
echo "⚙️  Configuration:"
BEPS_DOMAIN=${BEPS_DOMAIN:-"beps.boundaryml.com"}
GITHUB_ORG=${GITHUB_ORG:-"boundaryml"}
GITHUB_REPO=${GITHUB_REPO:-"baml"}

echo "  Domain: $BEPS_DOMAIN"
echo "  GitHub: $GITHUB_ORG/$GITHUB_REPO"
if [ ! -z "$CERTIFICATE_ARN" ]; then
    echo "  Certificate: $CERTIFICATE_ARN"
fi
echo ""

# Build TypeScript
echo "🔨 Building TypeScript..."
npm run build
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Check if CDK is bootstrapped
echo "🔍 Checking CDK bootstrap..."
BOOTSTRAP_STACK=$(aws cloudformation describe-stacks \
    --stack-name CDKToolkit \
    --region us-east-1 \
    --query 'Stacks[0].StackName' \
    --output text 2>/dev/null || echo "")

if [ -z "$BOOTSTRAP_STACK" ]; then
    echo -e "${YELLOW}⚠ CDK not bootstrapped in us-east-1${NC}"
    read -p "Bootstrap CDK now? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        npm run bootstrap
        echo -e "${GREEN}✓ CDK bootstrapped${NC}"
    else
        echo -e "${RED}❌ CDK bootstrap required${NC}"
        exit 1
    fi
else
    echo -e "${GREEN}✓ CDK already bootstrapped${NC}"
fi
echo ""

# Show diff
echo "📋 Changes to be deployed:"
echo "------------------------"
npm run diff || true
echo ""

# Confirm deployment
read -p "Deploy to AWS? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Deployment cancelled"
    exit 0
fi

# Deploy
echo ""
echo "🚀 Deploying stack..."
npm run deploy

echo ""
echo "=================================================="
echo -e "${GREEN}✅ Deployment Complete!${NC}"
echo "=================================================="
echo ""
echo "📋 Next Steps:"
echo ""
echo "1. Configure DNS (add CNAME records shown in outputs)"
echo "2. Configure GitHub Secrets (values shown in outputs)"
echo "3. Push to 'canary' branch to trigger first deployment"
echo ""
echo "🌐 Your BEPs site will be available at:"
echo "  Production: https://$BEPS_DOMAIN"
echo "  Previews: https://{branch-name}.$BEPS_DOMAIN"
echo ""

