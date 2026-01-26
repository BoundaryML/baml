# BEPs Infrastructure

Infrastructure as code for hosting BAML Enhancement Proposals (BEPs) with subdomain-based previews.

Uses **AWS CDK** (TypeScript) for declarative, type-safe infrastructure management.

## Architecture

- **Production**: `beps.boundaryml.com` → S3 root
- **Previews**: `{branch-name}.beps.boundaryml.com` → S3 `/{branch-name}/`
- **Auto-cleanup**: Previews expire after 14 days
- **Secure deployment**: GitHub Actions OIDC (no AWS keys)
- **Infrastructure**: Managed via CloudFormation

## Prerequisites

1. **Node.js 18+** and npm
2. **AWS CLI** with credentials configured
   - AWS SSO: `aws sso login --profile your-profile`
   - Static credentials: `aws configure`
   - Or use any valid credential method
3. **AWS CDK** installed globally (optional)
   ```bash
   npm install -g aws-cdk
   ```

## Quick Start

### 1. Configure AWS credentials

If using AWS SSO:
```bash
aws sso login --profile boundaryml-dev
export AWS_PROFILE=boundaryml-dev
```

If using static credentials, ensure they're configured:
```bash
aws sts get-caller-identity  # Should show your AWS account
```

### 2. Install dependencies

```bash
cd beps/infrastructure
npm install
```

### 3. Bootstrap CDK (first time only)

```bash
npm run bootstrap
# Or: cdk bootstrap aws://ACCOUNT-ID/us-east-1
```

### 4. Configure environment

Create a `.env` file or export environment variables:

```bash
export BEPS_DOMAIN="beps.boundaryml.com"
export GITHUB_ORG="boundaryml"
export GITHUB_REPO="baml"
# Optional: Use existing certificate
# export CERTIFICATE_ARN="arn:aws:acm:us-east-1:..."
```

### 4. Review the changes

```bash
npm run diff
```

### 5. Deploy

```bash
npm run deploy
```

The deployment will output all the GitHub secrets you need to configure.

### 6. Configure DNS

Add these CNAME records to your domain registrar:

```
Type: CNAME
Name: beps.boundaryml.com
Value: <from-output>.cloudfront.net

Type: CNAME
Name: *.beps.boundaryml.com
Value: <from-output>.cloudfront.net
```

### 7. Configure GitHub Secrets

Add these secrets to your GitHub repository (Settings → Secrets → Actions):

The deploy command outputs these values:
- `AWS_ROLE_ARN`
- `S3_BUCKET_BEPS`
- `CLOUDFRONT_DISTRIBUTION_ID_BEPS`
- `BEPS_DOMAIN`

## What Gets Created

### S3 Bucket
- Name: `baml-beps-{account-id}`
- Purpose: Hosts all static files
- Lifecycle: Auto-deletes files older than 14 days
- Encryption: S3-managed
- Access: Private (via CloudFront OAC only)

### CloudFront Distribution
- Domains: `beps.boundaryml.com` and `*.beps.boundaryml.com`
- SSL: ACM certificate (auto-created or use existing)
- Function: Routes subdomains to S3 paths
- Cache: Optimized for static content
- Protocol: HTTPS only (HTTP → HTTPS redirect)

### IAM Role (GitHub Actions)
- Name: `GitHubActions-BEPs-Deploy`
- Auth: OIDC (no long-lived credentials)
- Permissions:
  - S3: Read/Write/Delete to BEPs bucket
  - CloudFront: Create invalidations
- Scope: Only your GitHub org/repo

### ACM Certificate
- Domain: `beps.boundaryml.com`
- Alt names: `*.beps.boundaryml.com`
- Validation: DNS (automatic if using Route53)
- Region: us-east-1 (required for CloudFront)

## CDK Commands

```bash
# Install dependencies
npm install

# Compile TypeScript
npm run build

# Watch for changes
npm run watch

# Show what will be deployed
npm run diff

# Deploy to AWS
npm run deploy

# Synthesize CloudFormation template
npm run synth

# Destroy the stack (careful!)
cdk destroy
```

## Stack Outputs

After deployment, the stack outputs:

| Output | Description |
|--------|-------------|
| `BucketName` | S3 bucket name |
| `DistributionId` | CloudFront distribution ID |
| `DistributionDomain` | CloudFront domain for DNS |
| `DeployRoleArn` | IAM role ARN for GitHub |
| `Domain` | Your configured domain |
| `GitHubSecrets` | JSON with all secrets |

## Customization

### Use Existing Certificate

If you already have an ACM certificate:

```bash
export CERTIFICATE_ARN="arn:aws:acm:us-east-1:123456789012:certificate/..."
npm run deploy
```

### Change Domain

```bash
export BEPS_DOMAIN="docs.example.com"
npm run deploy
```

### Change Expiration Period

Edit `lib/beps-stack.ts`:

```typescript
lifecycleRules: [
  {
    id: 'DeleteOldPreviews',
    enabled: true,
    expiration: cdk.Duration.days(30), // Change to 30 days
    prefix: '',
  },
],
```

### Multi-Environment Setup

Create separate stacks for dev/staging/prod:

```typescript
// In bin/app.ts
new BepsStack(app, 'BepsStack-Dev', {
  domain: 'beps-dev.boundaryml.com',
  // ...
});

new BepsStack(app, 'BepsStack-Prod', {
  domain: 'beps.boundaryml.com',
  // ...
});
```

## Troubleshooting

### Certificate Validation Pending

If using a new certificate, you need to add DNS validation records:

```bash
aws acm describe-certificate \
  --certificate-arn <arn> \
  --region us-east-1
```

Add the CNAME records shown in the output.

### CloudFront Access Denied

Make sure the Origin Access Control (OAC) is properly configured. The stack automatically sets this up, but if you're having issues:

1. Check the bucket policy allows CloudFront
2. Verify the OAC ID in the distribution settings
3. Check CloudFront logs for specific errors

### GitHub Actions Can't Assume Role

1. Verify OIDC provider is configured
2. Check the trust policy includes your repo
3. Ensure `id-token: write` permission in workflow
4. Confirm AWS account ID matches

### Previews Not Working

1. Check CloudFront Function is attached
2. Verify files are uploaded to correct S3 path
3. Test with curl: `curl -H "Host: branch.beps.boundaryml.com" https://cloudfront-domain/`
4. Check CloudFront distribution behavior

## Cost Estimate

Based on moderate usage:

| Service | Cost |
|---------|------|
| S3 Storage (10 GB) | ~$0.23/month |
| S3 Requests (100K) | ~$0.04/month |
| CloudFront (100 GB) | ~$8.50/month |
| ACM Certificate | Free |
| Route 53 (optional) | $0.50/month |
| **Total** | **~$9-10/month** |

## Cleanup

To completely remove all resources:

```bash
# This will delete everything!
cdk destroy

# Confirm when prompted
```

Note: The S3 bucket is set to `RETAIN` by default. If you want to delete it:

1. Empty the bucket first: `aws s3 rm s3://bucket-name --recursive`
2. Then run `cdk destroy`

## CI/CD Integration

This CDK stack is designed to work with `.github/workflows/deploy-beps.yml`. The workflow:

1. Builds MkDocs site
2. Deploys to S3 using the IAM role
3. Invalidates CloudFront cache
4. Comments preview URLs on PRs

## Security

- ✅ No long-lived AWS credentials
- ✅ OIDC-based authentication
- ✅ Scoped IAM permissions
- ✅ Private S3 bucket (CloudFront OAC only)
- ✅ HTTPS enforced
- ✅ S3 encryption at rest

## Support

For issues or questions:
1. Check the troubleshooting section above
2. Review CloudFormation events in AWS Console
3. Check CDK diff output for unexpected changes
4. File an issue in the repository

## Further Reading

- [AWS CDK Documentation](https://docs.aws.amazon.com/cdk/)
- [CloudFront Functions](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/cloudfront-functions.html)
- [GitHub OIDC with AWS](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-amazon-web-services)
- [S3 Lifecycle Policies](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lifecycle-mgmt.html)

