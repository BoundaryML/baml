const cdk = require('aws-cdk-lib');
const { ec2, ecs, ecr, iam, logs, servicediscovery, events, cloudwatch, ssm } = {
  ec2: cdk.aws_ec2, ecs: cdk.aws_ecs, ecr: cdk.aws_ecr, iam: cdk.aws_iam,
  logs: cdk.aws_logs, servicediscovery: cdk.aws_servicediscovery,
  events: cdk.aws_events, cloudwatch: cdk.aws_cloudwatch, ssm: cdk.aws_ssm,
};
const matrix = require('../matrix.json');
const cells = () => matrix.variants.flatMap(variant => Object.keys(matrix.architectures).map(arch => ({ variant, arch, name: `${variant}-${arch}` })));
const logical = name => name.split('-').map(part => part[0].toUpperCase() + part.slice(1)).join('');

function validateRun(name, images, rate) {
  if (!/^[a-z][a-z0-9-]{1,30}[a-z0-9]$/.test(name)) throw new Error('Run name must be 3..32 lowercase letters, digits, or hyphens, starting with a letter');
  if (!Number.isInteger(rate) || rate < 1 || rate > 10000) throw new Error('Rate must be 1..10000');
  const expected = [...cells().map(cell => cell.name), 'load'].sort();
  if (JSON.stringify(Object.keys(images).sort()) !== JSON.stringify(expected)) throw new Error('Image manifest must contain all ten workload cells and load');
  for (const [key, image] of Object.entries(images)) {
    if (!/^\d{12}\.dkr\.ecr\.[a-z0-9-]+\.amazonaws\.com(?:\.cn)?\/[a-z0-9/_-]+@sha256:[0-9a-f]{64}$/.test(image)) throw new Error(`${key}: expected immutable ECR image digest`);
  }
}

function dashboard(runs, region) {
  const charts = [
    ['HTTP 200 / second', 'BAML/HelloWorld', 'Http200', 'Sum', 'Count/Second'],
    ['Response latency p90 (all responses)', 'BAML/HelloWorld', 'LatencyMs', 'p90', 'Milliseconds'],
    ['CPU (% of task allocation)', 'AWS/ECS', 'CPUUtilization', 'Average', 'Percent'],
    ['Memory (% of container limit)', 'AWS/ECS', 'MemoryUtilization', 'Average', 'Percent'],
    ['Transport errors / minute', 'BAML/HelloWorld', 'TransportErrors', 'Sum', 'Count'],
    ['HTTP 200 body mismatches / minute', 'BAML/HelloWorld', 'BodyMismatches', 'Sum', 'Count'],
  ];
  return { start: '-PT1H', periodOverride: 'inherit', widgets: charts.map(([title, namespace, metric, stat, unit], index) => {
    const metrics = [];
    for (const run of runs) for (const { variant, arch, name } of cells()) {
      const label = `${run} / ${variant} / ${arch}`;
      const id = `m${metrics.length}`;
      const dims = namespace === 'AWS/ECS' ? ['ClusterName', run, 'ServiceName', `${run}-${name}`] : ['RunName', run, 'Variant', variant, 'Architecture', arch];
      metrics.push([namespace, metric, ...dims, { label, stat, id, ...(metric === 'Http200' ? { visible: false } : {}) }]);
      if (metric === 'Http200') metrics.push([{ expression: `${id}/PERIOD(${id})`, label, id: `e${id.slice(1)}` }]);
    }
    return { type: 'metric', x: index % 2 * 12, y: Math.floor(index / 2) * 7, width: 12, height: 7,
      properties: { title, region, period: 60, view: 'timeSeries', stacked: false, metrics, yAxis: { left: { min: 0, label: unit, showUnits: false } } } };
  }) };
}

class FoundationStack extends cdk.Stack {
  constructor(scope, id, props = {}) {
    super(scope, id, props);
    this.vpc = new ec2.Vpc(this, 'Network', { ipAddresses: ec2.IpAddresses.cidr('10.80.0.0/16'),
      availabilityZones: [`${this.region}a`], natGateways: 0,
      subnetConfiguration: [{ name: 'Public', subnetType: ec2.SubnetType.PUBLIC, cidrMask: 24 }] });
    this.repository = new ecr.Repository(this, 'Images', { imageTagMutability: ecr.TagMutability.IMMUTABLE,
      imageScanOnPush: true, removalPolicy: cdk.RemovalPolicy.RETAIN });
    new cdk.CfnOutput(this, 'RepositoryUri', { value: this.repository.repositoryUri });
    new cdk.CfnOutput(this, 'VpcId', { value: this.vpc.vpcId });
    new cdk.CfnOutput(this, 'SubnetId', { value: this.vpc.publicSubnets[0].subnetId });
  }
}

class BenchmarkStack extends cdk.Stack {
  constructor(scope, id, props) {
    const { foundation, images, rate, appCount = 1, loadCount = 1, ...stackProps } = props;
    super(scope, id, stackProps);
    validateRun(id, images, rate);
    if (![0, 1].includes(appCount) || ![0, 1].includes(loadCount)) throw new Error('Counts must be 0 or 1');
    const run = id;
    const vpc = foundation.vpc;
    const subnet = vpc.publicSubnets[0];
    const cluster = new ecs.CfnCluster(this, 'Cluster', { clusterName: run, clusterSettings: [{ name: 'containerInsights', value: 'enhanced' }] });
    const appLogs = new logs.LogGroup(this, 'AppLogs', { logGroupName: `/baml/hello-world/${run}/apps`, retention: logs.RetentionDays.ONE_WEEK, removalPolicy: cdk.RemovalPolicy.RETAIN });
    const loadLogs = new logs.LogGroup(this, 'LoadLogs', { logGroupName: `/baml/hello-world/${run}/load`, retention: logs.RetentionDays.ONE_WEEK, removalPolicy: cdk.RemovalPolicy.RETAIN });
    const eventLogs = new logs.LogGroup(this, 'EventLogs', { logGroupName: `/baml/hello-world/${run}/task-events`, retention: logs.RetentionDays.ONE_WEEK, removalPolicy: cdk.RemovalPolicy.RETAIN });
    const executionRole = new iam.Role(this, 'ExecutionRole', { assumedBy: new iam.ServicePrincipal('ecs-tasks.amazonaws.com'),
      managedPolicies: [iam.ManagedPolicy.fromAwsManagedPolicyName('service-role/AmazonECSTaskExecutionRolePolicy')] });
    const instanceRole = new iam.Role(this, 'InstanceRole', { assumedBy: new iam.ServicePrincipal('ec2.amazonaws.com'), managedPolicies: [
      iam.ManagedPolicy.fromAwsManagedPolicyName('service-role/AmazonEC2ContainerServiceforEC2Role'),
      iam.ManagedPolicy.fromAwsManagedPolicyName('AmazonSSMManagedInstanceCore')] });
    const instanceProfile = new iam.CfnInstanceProfile(this, 'InstanceProfile', { roles: [instanceRole.roleName] });
    const hostSg = new ec2.SecurityGroup(this, 'HostSecurityGroup', { vpc, description: `${run} workload hosts` });
    const appSg = new ec2.SecurityGroup(this, 'AppSecurityGroup', { vpc, description: `${run} private workload tasks` });
    const loadSg = new ec2.SecurityGroup(this, 'LoadSecurityGroup', { vpc, description: `${run} load host` });
    for (const port of [8080, 9091]) appSg.addIngressRule(loadSg, ec2.Port.tcp(port));
    const namespace = new servicediscovery.PrivateDnsNamespace(this, 'Namespace', { name: `${run}.hello.internal`, vpc });
    const amis = {
      arm64: ssm.StringParameter.valueForTypedStringParameterV2(this, '/aws/service/ecs/optimized-ami/amazon-linux-2023/arm64/recommended/image_id', ssm.ParameterValueType.AWS_EC2_IMAGE_ID),
      x64: ssm.StringParameter.valueForTypedStringParameterV2(this, '/aws/service/ecs/optimized-ami/amazon-linux-2023/recommended/image_id', ssm.ParameterValueType.AWS_EC2_IMAGE_ID),
    };
    const makeInstance = (name, arch, load = false) => {
      const userData = ec2.UserData.forLinux();
      userData.addCommands('swapoff --all', 'systemctl mask swap.target', "cat >> /etc/ecs/ecs.config <<'EOF'", `ECS_CLUSTER=${run}`,
        `ECS_INSTANCE_ATTRIBUTES=${JSON.stringify({ 'bench.target': name })}`,
        'ECS_ENABLE_TASK_CPU_MEM_LIMIT=true', 'ECS_AWSVPC_BLOCK_IMDS=true',
        'ECS_ENABLE_AWSLOGS_EXECUTIONROLE_OVERRIDE=true', 'EOF');
      const instance = new ec2.CfnInstance(this, `${logical(name)}Instance`, { imageId: amis[arch],
        instanceType: load ? matrix.load_instance_type : matrix.architectures[arch].instance_type,
        iamInstanceProfile: instanceProfile.ref,
        networkInterfaces: [{ deviceIndex: '0', subnetId: subnet.subnetId, associatePublicIpAddress: true, groupSet: [(load ? loadSg : hostSg).securityGroupId] }],
        metadataOptions: { httpTokens: 'required', httpPutResponseHopLimit: 1 },
        blockDeviceMappings: [{ deviceName: '/dev/xvda', ebs: { volumeSize: 30, volumeType: 'gp3', encrypted: true, deleteOnTermination: true } }],
        userData: cdk.Fn.base64(userData.render()), tags: [{ key: 'Name', value: `${run}-${name}` }, { key: 'RunName', value: run }] });
      instance.addResourceDependency(cluster);
      return instance;
    };
    const logConfig = (group, prefix) => ({ logDriver: 'awslogs', options: { 'awslogs-group': group.logGroupName,
      'awslogs-region': this.region, 'awslogs-stream-prefix': prefix, mode: 'blocking' } });
    const makeServiceProps = (task, name, load = false) => ({ cluster: cluster.ref, serviceName: `${run}-${load ? 'load-' : ''}${name}`,
      taskDefinition: task.ref, desiredCount: load ? loadCount : appCount, launchType: 'EC2', schedulingStrategy: 'REPLICA',
      deploymentConfiguration: { minimumHealthyPercent: 0, maximumPercent: 100 },
      placementConstraints: [{ type: 'memberOf', expression: `attribute:bench.target == ${load ? 'load' : name}` }],
      ...(!load ? { networkConfiguration: { awsvpcConfiguration: { subnets: [subnet.subnetId], securityGroups: [appSg.securityGroupId], assignPublicIp: 'DISABLED' } } } : {}) });
    const loadInstance = makeInstance('load', 'x64', true);
    for (const { variant, arch, name } of cells()) {
      const key = logical(name);
      const instance = makeInstance(name, arch);
      const discovery = new servicediscovery.CfnService(this, `${key}Discovery`, { name, namespaceId: namespace.namespaceId,
        dnsConfig: { dnsRecords: [{ type: 'A', ttl: 10 }], routingPolicy: 'MULTIVALUE' }, healthCheckCustomConfig: { failureThreshold: 1 } });
      const task = new ecs.CfnTaskDefinition(this, `${key}Task`, { family: `${run}-${name}`, requiresCompatibilities: ['EC2'], networkMode: 'awsvpc',
        cpu: '1024', memory: '1024', executionRoleArn: executionRole.roleArn,
        runtimePlatform: { operatingSystemFamily: 'LINUX', cpuArchitecture: matrix.architectures[arch].ecs },
        containerDefinitions: [{ name: 'app', image: images[name], essential: true, cpu: 1024, memory: 1024,
          portMappings: [{ containerPort: 8080 }, { containerPort: 9091 }],
          environment: Object.entries({ BAML_PROFILE: '0', BAML_TELEMETRY_DISABLED: '1', BAML_LOG: 'off' }).map(([name, value]) => ({ name, value })),
          logConfiguration: logConfig(appLogs, name) }] });
      const service = new ecs.CfnService(this, `${key}Service`, { ...makeServiceProps(task, name), serviceRegistries: [{ registryArn: discovery.attrArn }] });
      service.addResourceDependency(instance);
      const env = { RunName: run, Variant: variant, Architecture: arch, RATE_PER_TARGET: String(rate), TARGET_URL: `http://${name}.${run}.hello.internal:8080/` };
      if (variant !== 'baml-only') env.PROCESS_METRICS_URL = `http://${name}.${run}.hello.internal:9091/metrics`;
      const loadTask = new ecs.CfnTaskDefinition(this, `${key}LoadTask`, { family: `${run}-load-${name}`, requiresCompatibilities: ['EC2'], networkMode: 'bridge',
        cpu: '256', memory: '512', executionRoleArn: executionRole.roleArn,
        containerDefinitions: [{ name: 'load', image: images.load, essential: true, cpu: 256, memory: 512, stopTimeout: 30,
          environment: Object.entries(env).map(([name, value]) => ({ name, value })), logConfiguration: logConfig(loadLogs, name) }] });
      const loadService = new ecs.CfnService(this, `${key}LoadService`, makeServiceProps(loadTask, name, true));
      loadService.addResourceDependency(loadInstance);
      loadService.addResourceDependency(service);
    }
    const taskEvents = new events.CfnRule(this, 'TaskEvents', { eventPattern: { source: ['aws.ecs'], 'detail-type': ['ECS Task State Change'], detail: { clusterArn: [cluster.attrArn] } },
      targets: [{ arn: eventLogs.logGroupArn, id: 'TaskEventLogs' }] });
    new logs.CfnResourcePolicy(this, 'EventLogPolicy', { policyName: `${run}-task-events`, policyDocument: cdk.Stack.of(this).toJsonString({
      Version: '2012-10-17', Statement: [{ Effect: 'Allow', Principal: { Service: ['events.amazonaws.com', 'delivery.logs.amazonaws.com'] },
        Action: ['logs:CreateLogStream', 'logs:PutLogEvents'], Resource: eventLogs.logGroupArn,
        Condition: { ArnEquals: { 'aws:SourceArn': taskEvents.attrArn } } }] }) });
    new cloudwatch.CfnDashboard(this, 'Dashboard', { dashboardName: run, dashboardBody: this.toJsonString(dashboard([run], this.region)) });
    cdk.Tags.of(this).add('RunName', run);
    cdk.Tags.of(this).add('Purpose', 'baml-hello-world-benchmark');
    new cdk.CfnOutput(this, 'ClusterName', { value: run });
    new cdk.CfnOutput(this, 'DashboardUrl', { value: `https://${this.region}.console.aws.amazon.com/cloudwatch/home?region=${this.region}#dashboards:name=${run}` });
    new cdk.CfnOutput(this, 'TaskEventsLogGroup', { value: eventLogs.logGroupName });
    new cdk.CfnOutput(this, 'LoadLogGroup', { value: loadLogs.logGroupName });
    this.addMetadata('LoadProfile', { rate_per_target: rate });
  }
}

module.exports = { FoundationStack, BenchmarkStack, cells, dashboard, validateRun };
