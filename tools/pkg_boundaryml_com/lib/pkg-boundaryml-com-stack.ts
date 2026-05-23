import * as cdk from 'aws-cdk-lib';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as s3 from 'aws-cdk-lib/aws-s3';
import { Construct } from 'constructs';

const GITHUB_REPO = 'BoundaryML/baml';
const GITHUB_OIDC_PROVIDER_ARN =
  'arn:aws:iam::277707123528:oidc-provider/token.actions.githubusercontent.com';

export class PkgBoundarymlComStack extends cdk.Stack {
  public readonly bucket: s3.Bucket;
  public readonly githubReleaseRole: iam.Role;

  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    this.bucket = new s3.Bucket(this, 'PkgBoundarymlComSiteBucket', {
      websiteIndexDocument: 'index.html',
      websiteErrorDocument: 'error.html',
      publicReadAccess: true,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ACLS,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
      autoDeleteObjects: false,
    });

    const githubProvider = iam.OpenIdConnectProvider.fromOpenIdConnectProviderArn(
      this,
      'GitHubOidcProvider',
      GITHUB_OIDC_PROVIDER_ARN,
    );

    this.githubReleaseRole = new iam.Role(this, 'GitHubReleaseRole', {
      roleName: 'pkg-boundaryml-com-github-release',
      description: `Assumed by GitHub Actions in ${GITHUB_REPO} (tag pushes only) to publish to the pkg.boundaryml.com bucket`,
      assumedBy: new iam.OpenIdConnectPrincipal(githubProvider, {
        StringEquals: {
          'token.actions.githubusercontent.com:aud': 'sts.amazonaws.com',
        },
        StringLike: {
          'token.actions.githubusercontent.com:sub': `repo:${GITHUB_REPO}:ref:refs/tags/*`,
        },
      }),
      maxSessionDuration: cdk.Duration.hours(1),
    });

    this.bucket.grantPut(this.githubReleaseRole);
    this.bucket.grantPutAcl(this.githubReleaseRole);
    this.githubReleaseRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['s3:ListBucket'],
        resources: [this.bucket.bucketArn],
      }),
    );

    new cdk.CfnOutput(this, 'BucketName', {
      value: this.bucket.bucketName,
      description: 'Name of the S3 bucket hosting the static site',
    });

    new cdk.CfnOutput(this, 'WebsiteUrl', {
      value: this.bucket.bucketWebsiteUrl,
      description: 'URL of the S3 static website endpoint',
    });

    new cdk.CfnOutput(this, 'GitHubReleaseRoleArn', {
      value: this.githubReleaseRole.roleArn,
      description: 'IAM role ARN for GitHub Actions OIDC',
    });
  }
}
