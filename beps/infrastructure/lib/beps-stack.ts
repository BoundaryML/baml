import * as cdk from 'aws-cdk-lib';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as cloudfront from 'aws-cdk-lib/aws-cloudfront';
import * as cloudfront_origins from 'aws-cdk-lib/aws-cloudfront-origins';
import * as acm from 'aws-cdk-lib/aws-certificatemanager';
import * as iam from 'aws-cdk-lib/aws-iam';
import { Construct } from 'constructs';

export interface BepsStackProps extends cdk.StackProps {
  domain?: string; // Optional: Custom domain (skip for testing)
  githubOrg: string;
  githubRepo: string;
  certificateArn?: string; // Optional: Use existing certificate
  githubOidcProviderArn?: string; // Optional: Use existing OIDC provider
}

export class BepsStack extends cdk.Stack {
  public readonly bucket: s3.Bucket;
  public readonly distribution: cloudfront.Distribution;
  public readonly deployRole: iam.Role;

  constructor(scope: Construct, id: string, props: BepsStackProps) {
    super(scope, id, props);

    const { domain, githubOrg, githubRepo, certificateArn, githubOidcProviderArn } = props;

    // S3 Bucket for hosting
    this.bucket = new s3.Bucket(this, 'BepsBucket', {
      bucketName: `baml-beps-${this.account}`,
      publicReadAccess: false,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
      autoDeleteObjects: false,
      versioned: false,
      encryption: s3.BucketEncryption.S3_MANAGED,
      lifecycleRules: [
        {
          id: 'DeleteOldPreviews',
          enabled: true,
          expiration: cdk.Duration.days(14),
          // Don't expire files in the root (production)
          prefix: '',
          // This will only affect files with last modified > 14 days
        },
      ],
    });

    // SSL Certificate (only if custom domain is provided)
    let certificate: acm.ICertificate | undefined;
    if (domain) {
      certificate = certificateArn
        ? acm.Certificate.fromCertificateArn(this, 'Certificate', certificateArn)
        : new acm.Certificate(this, 'Certificate', {
          domainName: domain,
          subjectAlternativeNames: [`*.${domain}`],
          validation: acm.CertificateValidation.fromDns(),
        });
    }

    // CloudFront Function for subdomain routing
    const routingFunction = new cloudfront.Function(this, 'SubdomainRouter', {
      code: cloudfront.FunctionCode.fromInline(`
function handler(event) {
    var request = event.request;
    var host = request.headers.host.value;
    
    // Extract subdomain from host
    var parts = host.split('.');
    
    // Check if this is a custom domain with subdomain (e.g., branch.beps.boundaryml.com)
    // Skip CloudFront domains (*.cloudfront.net) and main custom domain
    var isCloudFrontDomain = host.endsWith('.cloudfront.net');
    var isSubdomain = !isCloudFrontDomain && parts.length > 2 && parts[0] !== 'beps';
    
    if (isSubdomain) {
        var subdomain = parts[0];
        
        // Handle index.html
        if (request.uri === '/' || request.uri === '') {
            request.uri = '/' + subdomain + '/index.html';
        } else if (request.uri.endsWith('/')) {
            request.uri = '/' + subdomain + request.uri + 'index.html';
        } else if (!request.uri.includes('.')) {
            request.uri = '/' + subdomain + request.uri + '/index.html';
        } else {
            request.uri = '/' + subdomain + request.uri;
        }
    } else {
        // Main domain or CloudFront domain - add index.html if needed
        if (request.uri === '/' || request.uri === '') {
            request.uri = '/index.html';
        } else if (request.uri.endsWith('/')) {
            request.uri = request.uri + 'index.html';
        } else if (!request.uri.includes('.')) {
            request.uri = request.uri + '/index.html';
        }
    }
    
    return request;
}
      `),
      runtime: cloudfront.FunctionRuntime.JS_1_0,
      comment: 'Routes requests based on subdomain to branch-specific S3 paths',
    });

    // Origin Access Control for S3
    const oac = new cloudfront.CfnOriginAccessControl(this, 'OAC', {
      originAccessControlConfig: {
        name: 'beps-oac',
        originAccessControlOriginType: 's3',
        signingBehavior: 'always',
        signingProtocol: 'sigv4',
        description: 'OAC for BAML BEPs S3 bucket',
      },
    });

    // CloudFront Distribution
    this.distribution = new cloudfront.Distribution(this, 'Distribution', {
      defaultBehavior: {
        origin: new cloudfront_origins.S3Origin(this.bucket),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
        compress: true,
        functionAssociations: [
          {
            function: routingFunction,
            eventType: cloudfront.FunctionEventType.VIEWER_REQUEST,
          },
        ],
      },
      // Only add custom domain if provided
      ...(domain && certificate ? {
        domainNames: [domain, `*.${domain}`],
        certificate: certificate,
      } : {}),
      minimumProtocolVersion: cloudfront.SecurityPolicyProtocol.TLS_V1_2_2021,
      httpVersion: cloudfront.HttpVersion.HTTP2_AND_3,
      priceClass: cloudfront.PriceClass.PRICE_CLASS_100,
      comment: 'BAML BEPs documentation with subdomain-based previews',
      errorResponses: [
        {
          httpStatus: 404,
          responseHttpStatus: 404,
          responsePagePath: '/404.html',
          ttl: cdk.Duration.minutes(5),
        },
      ],
    });

    // Update the distribution to use OAC (workaround for L2 construct limitation)
    const cfnDistribution = this.distribution.node.defaultChild as cloudfront.CfnDistribution;
    cfnDistribution.addPropertyOverride(
      'DistributionConfig.Origins.0.OriginAccessControlId',
      oac.attrId
    );
    cfnDistribution.addPropertyOverride(
      'DistributionConfig.Origins.0.S3OriginConfig.OriginAccessIdentity',
      ''
    );

    // Bucket policy to allow CloudFront OAC
    this.bucket.addToResourcePolicy(
      new iam.PolicyStatement({
        sid: 'AllowCloudFrontServicePrincipal',
        effect: iam.Effect.ALLOW,
        principals: [new iam.ServicePrincipal('cloudfront.amazonaws.com')],
        actions: ['s3:GetObject'],
        resources: [this.bucket.arnForObjects('*')],
        conditions: {
          StringEquals: {
            'AWS:SourceArn': `arn:aws:cloudfront::${this.account}:distribution/${this.distribution.distributionId}`,
          },
        },
      })
    );

    // OIDC Provider ARN for GitHub Actions
    // If provided, use existing provider. Otherwise assume standard ARN format.
    const oidcProviderArn = githubOidcProviderArn ||
      `arn:aws:iam::${this.account}:oidc-provider/token.actions.githubusercontent.com`;

    // IAM Role for GitHub Actions
    // Use a custom assume role policy document to avoid importing the OIDC provider resource
    this.deployRole = new iam.Role(this, 'GitHubActionsRole', {
      roleName: 'GitHubActions-BEPs-Deploy',
      assumedBy: new iam.FederatedPrincipal(
        oidcProviderArn,
        {
          StringEquals: {
            'token.actions.githubusercontent.com:aud': 'sts.amazonaws.com',
          },
          StringLike: {
            'token.actions.githubusercontent.com:sub': `repo:${githubOrg}/${githubRepo}:*`,
          },
        },
        'sts:AssumeRoleWithWebIdentity'
      ),
      description: 'Role for GitHub Actions to deploy BEPs to S3',
      maxSessionDuration: cdk.Duration.hours(1),
    });

    // Grant permissions to the role
    this.bucket.grantReadWrite(this.deployRole);
    this.bucket.grantDelete(this.deployRole);

    this.deployRole.addToPolicy(
      new iam.PolicyStatement({
        effect: iam.Effect.ALLOW,
        actions: ['cloudfront:CreateInvalidation'],
        resources: [
          `arn:aws:cloudfront::${this.account}:distribution/${this.distribution.distributionId}`,
        ],
      })
    );

    // Outputs
    new cdk.CfnOutput(this, 'BucketName', {
      value: this.bucket.bucketName,
      description: 'S3 bucket name for BEPs',
      exportName: 'BepsBucketName',
    });

    new cdk.CfnOutput(this, 'DistributionId', {
      value: this.distribution.distributionId,
      description: 'CloudFront distribution ID',
      exportName: 'BepsDistributionId',
    });

    new cdk.CfnOutput(this, 'DistributionDomain', {
      value: this.distribution.distributionDomainName,
      description: 'CloudFront distribution domain name',
      exportName: 'BepsDistributionDomain',
    });

    new cdk.CfnOutput(this, 'DeployRoleArn', {
      value: this.deployRole.roleArn,
      description: 'IAM role ARN for GitHub Actions',
      exportName: 'BepsDeployRoleArn',
    });

    if (domain) {
      new cdk.CfnOutput(this, 'Domain', {
        value: domain,
        description: 'Primary domain for BEPs',
        exportName: 'BepsDomain',
      });
    }

    // Print GitHub Secrets
    new cdk.CfnOutput(this, 'GitHubSecrets', {
      value: JSON.stringify({
        AWS_ROLE_ARN: this.deployRole.roleArn,
        S3_BUCKET_BEPS: this.bucket.bucketName,
        CLOUDFRONT_DISTRIBUTION_ID_BEPS: this.distribution.distributionId,
        BEPS_DOMAIN: domain || this.distribution.distributionDomainName,
      }, null, 2),
      description: 'GitHub Secrets to configure (JSON format)',
    });
  }
}

