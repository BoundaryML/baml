# BEPs Deployment Guide

Complete guide for deploying BAML Enhancement Proposals (BEPs) documentation to AWS with subdomain-based previews using AWS CDK.

## Overview

- **Production**: `https://beps.boundaryml.com` (from `canary` branch)
- **Previews**: `https://{branch-name}.beps.boundaryml.com` (from PRs)
- **Auto-cleanup**: Previews expire after 14 days
- **Security**: GitHub Actions OIDC (no AWS keys needed)
- **Infrastructure**: AWS CDK with TypeScript

## Quick Start

```bash
# If using AWS SSO, login first
aws sso login --profile boundaryml-dev
export AWS_PROFILE=boundaryml-dev

# Deploy infrastructure
cd beps/infrastructure
npm install
./deploy.sh
```

See [infrastructure/README.md](./infrastructure/README.md) for detailed documentation.

## What Gets Deployed

### AWS Resources

1. **S3 Bucket** - Hosts all static files
   - Production files at root
   - Preview files at `/{branch-name}/`
   - Auto-expires files after 14 days

2. **CloudFront Distribution** - CDN with subdomain routing
   - Main domain: `beps.boundaryml.com`
   - Wildcard: `*.beps.boundaryml.com`
   - Custom function routes subdomains to correct S3 paths

3. **ACM Certificate** - SSL for HTTPS
   - Covers `beps.boundaryml.com` and `*.beps.boundaryml.com`
   - Auto-renewed by AWS

4. **IAM Role** - GitHub Actions deployment
   - OIDC-based (no long-lived credentials)
   - Scoped to your repository only
   - Can write to S3 and invalidate CloudFront

### GitHub Workflows

Located in `.github/workflows/`:

1. **`deploy-beps.yml`** - Main deployment
   - Builds MkDocs site
   - Deploys to S3 on push to `canary` or PR
   - Comments preview URLs on PRs

2. **`cleanup-beps-preview.yml`** - Immediate cleanup
   - Runs when branches are deleted
   - Removes S3 files for that branch

3. **`cleanup-stale-previews.yml`** - Scheduled cleanup
   - Runs daily at 2 AM UTC
   - Deletes previews for deleted branches
   - Deletes previews older than 14 days

## Post-Deployment Steps

### 1. Configure GitHub Secrets

Add these to your repository (Settings → Secrets → Actions):

```
AWS_ROLE_ARN=arn:aws:iam::123456789012:role/GitHubActions-BEPs-Deploy
S3_BUCKET_BEPS=baml-beps-123456789012
CLOUDFRONT_DISTRIBUTION_ID_BEPS=E1234567890ABC
BEPS_DOMAIN=beps.boundaryml.com
```

The deployment outputs these exact values.

### 2. Configure DNS

Add CNAME records to your domain registrar:

| Type | Name | Value |
|------|------|-------|
| CNAME | `beps.boundaryml.com` | `d1234567890abc.cloudfront.net` |
| CNAME | `*.beps.boundaryml.com` | `d1234567890abc.cloudfront.net` |

Get the CloudFront domain from the deployment output.

### 3. Validate Certificate (if new)

If a new ACM certificate was created, validate it via DNS:

```bash
aws acm describe-certificate \
  --certificate-arn <arn-from-output> \
  --region us-east-1
```

Add the CNAME records shown in the output.

### 4. Test Deployment

Push a change to the `canary` branch or create a PR to test the deployment:

```bash
git checkout -b test-deployment
# Make a change to beps/docs/
git commit -am "Test deployment"
git push origin test-deployment
# Open PR on GitHub
```

You should see:
- ✅ Workflow runs successfully
- ✅ Comment with preview URL
- ✅ Preview site accessible at `https://test-deployment.beps.boundaryml.com`

## Local Development

### Serve locally

```bash
cd beps
uv run --with mkdocs-material --with mkdocs-awesome-pages-plugin mkdocs serve
```

Or use mise:

```bash
mise run bep:serve
```

### Create new BEP

```bash
mise run bep:new
```

### Update BEP metadata

```bash
mise run bep:update BEP-001
```

### Update README table

```bash
mise run bep:readme
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         GitHub Actions                        │
│  (Triggered by push to canary or PR)                         │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            │ OIDC Auth (no keys!)
                            │
                            ▼
                ┌───────────────────────┐
                │   IAM Role (Deploy)   │
                └───────────┬───────────┘
                            │
                ┌───────────┴───────────┐
                │                       │
                ▼                       ▼
        ┌───────────────┐      ┌──────────────┐
        │   S3 Bucket   │      │  CloudFront  │
        │               │◄─────┤              │
        │  /index.html  │      │  Distribution│
        │  /branch-1/   │      └──────┬───────┘
        │  /branch-2/   │             │
        └───────────────┘             │
                                      │
                                      ▼
                          ┌────────────────────────┐
                          │    CloudFront Function │
                          │   (Subdomain Router)   │
                          └────────────────────────┘
                                      │
                ┌─────────────────────┴──────────────────────┐
                │                                            │
                ▼                                            ▼
    beps.boundaryml.com                    branch.beps.boundaryml.com
    → /index.html                          → /branch/index.html
```

## How Subdomain Routing Works

The CloudFront Function examines the `Host` header:

1. **Main domain** (`beps.boundaryml.com`)
   - Request: `/docs/setup/`
   - Serves: `s3://bucket/docs/setup/index.html`

2. **Subdomain** (`my-branch.beps.boundaryml.com`)
   - Request: `/docs/setup/`
   - Extracts subdomain: `my-branch`
   - Serves: `s3://bucket/my-branch/docs/setup/index.html`

This allows infinite preview environments without changing infrastructure!

## Preview Cleanup

### Automatic Cleanup

1. **On branch deletion**: Immediate cleanup via workflow
2. **Daily at 2 AM UTC**: Cleanup stale previews (14+ days)
3. **S3 Lifecycle**: Final safety net (14 days)

### Manual Cleanup

```bash
# Clean up a specific branch
BRANCH="my-feature"
aws s3 rm s3://baml-beps-123456789012/$BRANCH/ --recursive
aws cloudfront create-invalidation \
  --distribution-id E1234567890ABC \
  --paths "/$BRANCH/*"
```

## Cost Estimate

Based on moderate usage (100 GB/month):

| Service | Monthly Cost |
|---------|--------------|
| S3 Storage (10 GB) | $0.23 |
| S3 Requests (100K) | $0.04 |
| CloudFront (100 GB) | $8.50 |
| ACM Certificate | Free |
| **Total** | **~$9-10** |

## Troubleshooting

### Preview URL returns 404

1. Check files were uploaded: `aws s3 ls s3://bucket/branch-name/`
2. Check CloudFront Function is attached
3. Test with direct S3 path: `curl https://cloudfront-domain/branch-name/index.html`

### Certificate validation stuck

1. Check DNS records were added correctly
2. Wait 5-10 minutes for DNS propagation
3. Verify records: `dig validation-domain.example.com CNAME`

### GitHub Actions can't assume role

1. Verify `id-token: write` permission in workflow
2. Check IAM role trust policy includes your repo
3. Confirm AWS account ID matches
4. Check OIDC provider exists

### Changes not showing up

1. CloudFront cache may be stale (wait 5 minutes or invalidate)
2. Check deployment actually ran successfully
3. Verify files in S3 have correct timestamp

## Security

- ✅ No AWS credentials in GitHub (uses OIDC)
- ✅ IAM role scoped to specific repository
- ✅ S3 bucket is private (CloudFront OAC only)
- ✅ HTTPS enforced everywhere
- ✅ S3 encryption at rest

## Updates and Maintenance

### Update infrastructure (CDK)

```bash
cd beps/infrastructure
# Edit lib/beps-stack.ts
npm run diff     # Preview changes
npm run deploy   # Apply changes
```

### Update dependencies

```bash
cd beps
# Update mkdocs.yml
# Or update Python dependencies
```

### Scale for more traffic

CloudFront automatically scales. For very high traffic:
1. Enable CloudFront access logs
2. Monitor CloudFront metrics in CloudWatch
3. Consider upgrading PriceClass for better global performance

## Support

For issues:
1. Check the troubleshooting section above
2. Review GitHub Actions logs
3. Check CloudFormation events (if using CDK)
4. Review CloudFront/S3 logs

## Further Reading

- [MkDocs Material](https://squidfunk.github.io/mkdocs-material/)
- [CloudFront Functions](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/cloudfront-functions.html)
- [GitHub OIDC with AWS](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-amazon-web-services)
- [AWS CDK](https://docs.aws.amazon.com/cdk/)

