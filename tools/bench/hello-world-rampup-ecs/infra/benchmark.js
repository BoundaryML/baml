const cdk = require('aws-cdk-lib');
const { ec2, ecs, ecr, iam, logs, servicediscovery, events, cloudwatch, ssm } = {
  ec2: cdk.aws_ec2, ecs: cdk.aws_ecs, ecr: cdk.aws_ecr, iam: cdk.aws_iam,
  logs: cdk.aws_logs, servicediscovery: cdk.aws_servicediscovery,
  events: cdk.aws_events, cloudwatch: cdk.aws_cloudwatch, ssm: cdk.aws_ssm,
};
const matrix = require('../matrix.json');
const cells = () => matrix.variants.flatMap(variant => Object.keys(matrix.architectures).map(arch => ({ variant, arch, name: `${variant}-${arch}` })));
const logical = name => name.split('-').map(part => part[0].toUpperCase() + part.slice(1)).join('');

function checkedRate(value, label) {
  if (!Number.isInteger(value) || value < 1 || value > 50000) throw new Error(`${label} must be an integer from 1..50000`);
  return value;
}

function cellValue(configured, cell, label) {
  if (Number.isInteger(configured)) return configured;
  if (configured && typeof configured === 'object') {
    const value = configured[cell.name] ?? configured[cell.variant];
    if (value !== undefined) return value;
  }
  throw new Error(`${label} is missing ${cell.name}`);
}

function normalizeProfile(profile, cell) {
  if (!profile || !['ramp', 'sustain', 'binary'].includes(profile.mode)) throw new Error('Profile mode must be ramp, sustain, or binary');
  if (profile.on_seconds !== 4 || profile.off_seconds !== 1) throw new Error('The load duty cycle must be 4 seconds on and 1 second off');
  if (profile.explicit_gc !== undefined && typeof profile.explicit_gc !== 'boolean') throw new Error('explicit_gc must be a boolean');
  const explicitGc = profile.explicit_gc === true;
  if (explicitGc && profile.mode !== 'binary') throw new Error('explicit_gc is currently supported only in binary mode');
  if (profile.mode === 'ramp') {
    const start = checkedRate(cellValue(profile.start_rate_per_target, cell, 'start_rate_per_target'), `start_rate_per_target for ${cell.name}`);
    const increase = cellValue(profile.increase_rate_per_target, cell, 'increase_rate_per_target');
    if (!Number.isInteger(increase) || increase < 0 || increase > 50000) throw new Error(`increase_rate_per_target for ${cell.name} must be an integer from 0..50000`);
    const every = profile.increase_every_seconds;
    const maximum = checkedRate(cellValue(profile.maximum_rate_per_target, cell, 'maximum_rate_per_target'), `maximum_rate_per_target for ${cell.name}`);
    if (!Number.isInteger(every) || every < 5 || every % 5 !== 0) throw new Error('increase_every_seconds must be a positive multiple of the 5-second duty cycle');
    if (maximum < start) throw new Error('maximum_rate_per_target must be at least start_rate_per_target');
    return { mode: 'ramp', start, increase, every, maximum, on: 4, off: 1, explicitGc };
  }
  if (profile.mode === 'binary') {
    const lower = checkedRate(cellValue(profile.lower_rate_per_target, cell, 'lower_rate_per_target'), `lower_rate_per_target for ${cell.name}`);
    const upper = checkedRate(cellValue(profile.upper_rate_per_target, cell, 'upper_rate_per_target'), `upper_rate_per_target for ${cell.name}`);
    const resolution = profile.resolution_rps;
    const seconds = profile.seconds_per_candidate;
    const connections = profile.connections;
    const timeout = profile.request_timeout_ms;
    if (upper <= lower) throw new Error('upper_rate_per_target must exceed lower_rate_per_target');
    if (!Number.isInteger(resolution) || resolution < 1 || resolution > upper - lower) throw new Error('resolution_rps must fit inside the search interval');
    if (!Number.isInteger(seconds) || seconds < 5 || seconds % 5 !== 0) throw new Error('seconds_per_candidate must be a positive multiple of five');
    if (!Number.isInteger(connections) || connections < 1 || connections > 10000) throw new Error('connections must be 1..10000');
    if (!Number.isInteger(timeout) || timeout < 100 || timeout >= 1000) throw new Error('request_timeout_ms must fit inside the one-second off-window');
    return { mode: 'binary', start: lower, increase: 0, every: seconds, maximum: upper, lower, upper, resolution, seconds, connections, timeout, on: 4, off: 1, explicitGc };
  }
  const rate = cellValue(profile.rate_per_target, cell, 'rate_per_target');
  checkedRate(rate, `rate_per_target for ${cell.name}`);
  return { mode: 'sustain', start: rate, increase: 0, every: 30, maximum: rate, on: 4, off: 1, explicitGc };
}

function validateRun(name, images, profile) {
  if (!/^[a-z][a-z0-9-]{1,30}[a-z0-9]$/.test(name)) throw new Error('Run name must be 3..32 lowercase letters, digits, or hyphens, starting with a letter');
  for (const cell of cells()) normalizeProfile(profile, cell);
  const expected = [...cells().map(cell => cell.name), 'load'].sort();
  if (JSON.stringify(Object.keys(images).sort()) !== JSON.stringify(expected)) throw new Error('Image manifest must contain all ten workload cells and load');
  for (const [key, image] of Object.entries(images)) {
    if (!/^\d{12}\.dkr\.ecr\.[a-z0-9-]+\.amazonaws\.com(?:\.cn)?\/[a-z0-9/_-]+@sha256:[0-9a-f]{64}$/.test(image)) throw new Error(`${key}: expected immutable ECR image digest`);
  }
}

function dashboard(runs, region) {
  const charts = [
    ['HTTP 200 / wall second', 'BAML/HelloWorldRampup', 'Http200', 'Sum', 'Count/Second'],
    ['Configured active RPS', 'BAML/HelloWorldRampup', 'TargetRps', 'Average', 'Count/Second'],
    ['Response latency p90', 'BAML/HelloWorldRampup', 'LatencyP90Ms', 'Average', 'Milliseconds'],
    ['CPU (% of task allocation)', 'AWS/ECS', 'CPUUtilization', 'Average', 'Percent'],
    ['Memory (% of container limit)', 'AWS/ECS', 'MemoryUtilization', 'Average', 'Percent'],
    ['Transport errors / minute', 'BAML/HelloWorldRampup', 'TransportErrors', 'Sum', 'Count'],
    ['HTTP 200 body mismatches / minute', 'BAML/HelloWorldRampup', 'BodyMismatches', 'Sum', 'Count'],
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
    const { foundation, images, profile, appCount = 1, loadCount = 1, targetCell, localLoadCidr, ...stackProps } = props;
    super(scope, id, stackProps);
    validateRun(id, images, profile);
    if (![0, 1].includes(appCount) || ![0, 1].includes(loadCount)) throw new Error('Counts must be 0 or 1');
    if (targetCell && !cells().some(cell => cell.name === targetCell)) throw new Error(`Unknown target cell: ${targetCell}`);
    if (profile.explicit_gc && !['baml-only-arm64', 'baml-only-x64'].includes(targetCell)) throw new Error('explicit_gc requires one baml-only targetCell');
    if (localLoadCidr && !/^([0-9]{1,3}\.){3}[0-9]{1,3}\/32$/.test(localLoadCidr)) throw new Error('localLoadCidr must be an IPv4 /32');
    if (localLoadCidr && (!targetCell || loadCount !== 0)) throw new Error('localLoadCidr requires one targetCell and loadCount=0');
    const run = id;
    const selectedCells = targetCell ? cells().filter(cell => cell.name === targetCell) : cells();
    const vpc = foundation.vpc;
    const subnet = vpc.publicSubnets[0];
    const cluster = new ecs.CfnCluster(this, 'Cluster', { clusterName: run, clusterSettings: [{ name: 'containerInsights', value: 'enhanced' }] });
    const appLogs = new logs.LogGroup(this, 'AppLogs', { logGroupName: `/baml/hello-world-rampup/${run}/apps`, retention: logs.RetentionDays.ONE_WEEK, removalPolicy: cdk.RemovalPolicy.RETAIN });
    const loadLogs = new logs.LogGroup(this, 'LoadLogs', { logGroupName: `/baml/hello-world-rampup/${run}/load`, retention: logs.RetentionDays.ONE_WEEK, removalPolicy: cdk.RemovalPolicy.RETAIN });
    const eventLogs = new logs.LogGroup(this, 'EventLogs', { logGroupName: `/baml/hello-world-rampup/${run}/task-events`, retention: logs.RetentionDays.ONE_WEEK, removalPolicy: cdk.RemovalPolicy.RETAIN });
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
    if (localLoadCidr) for (const port of [8080, 9091]) hostSg.addIngressRule(ec2.Peer.ipv4(localLoadCidr), ec2.Port.tcp(port), 'Local benchmark client only');
    const namespace = new servicediscovery.PrivateDnsNamespace(this, 'Namespace', { name: `${run}.hello.internal`, vpc });
    const requiredArchitectures = new Set([...selectedCells.map(cell => cell.arch), ...(loadCount ? ['x64'] : [])]);
    const amis = Object.fromEntries([...requiredArchitectures].map(arch => [arch,
      ssm.StringParameter.valueForTypedStringParameterV2(this, `/aws/service/ecs/optimized-ami/amazon-linux-2023/${arch === 'arm64' ? 'arm64/recommended' : 'recommended'}/image_id`, ssm.ParameterValueType.AWS_EC2_IMAGE_ID)]));
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
      ...(!load && !localLoadCidr ? { networkConfiguration: { awsvpcConfiguration: { subnets: [subnet.subnetId], securityGroups: [appSg.securityGroupId], assignPublicIp: 'DISABLED' } } } : {}) });
    const loadInstance = loadCount ? makeInstance('load', 'x64', true) : undefined;
    for (const { variant, arch, name } of selectedCells) {
      const key = logical(name);
      const instance = makeInstance(name, arch);
      const discovery = new servicediscovery.CfnService(this, `${key}Discovery`, { name, namespaceId: namespace.namespaceId,
        dnsConfig: { dnsRecords: [{ type: 'A', ttl: 10 }], routingPolicy: 'MULTIVALUE' }, healthCheckCustomConfig: { failureThreshold: 1 } });
      const task = new ecs.CfnTaskDefinition(this, `${key}Task`, { family: `${run}-${name}`, requiresCompatibilities: ['EC2'], networkMode: localLoadCidr ? 'bridge' : 'awsvpc',
        cpu: '1024', memory: '1024', executionRoleArn: executionRole.roleArn,
        runtimePlatform: { operatingSystemFamily: 'LINUX', cpuArchitecture: matrix.architectures[arch].ecs },
        containerDefinitions: [{ name: 'app', image: images[name], essential: true, cpu: 1024, memory: 1024,
          portMappings: [{ containerPort: 8080, ...(localLoadCidr ? { hostPort: 8080 } : {}) }, { containerPort: 9091, ...(localLoadCidr ? { hostPort: 9091 } : {}) }],
          environment: Object.entries({ BAML_PROFILE: '0', BAML_TELEMETRY_DISABLED: '1', BAML_LOG: 'off' }).map(([name, value]) => ({ name, value })),
          logConfiguration: logConfig(appLogs, name) }] });
      const service = new ecs.CfnService(this, `${key}Service`, { ...makeServiceProps(task, name), ...(!localLoadCidr ? { serviceRegistries: [{ registryArn: discovery.attrArn }] } : {}) });
      service.addResourceDependency(instance);
      if (loadCount) {
        const load = normalizeProfile(profile, { variant, arch, name });
        const env = { RunName: run, Variant: variant, Architecture: arch, START_RATE_PER_TARGET: String(load.start),
          RATE_STEP_PER_TARGET: String(load.increase), RATE_STEP_SECONDS: String(load.every), MAX_RATE_PER_TARGET: String(load.maximum),
          ON_SECONDS: String(load.on), OFF_SECONDS: String(load.off), LOAD_MODE: load.mode, TARGET_URL: `http://${name}.${run}.hello.internal:8080/` };
        if (load.explicitGc) env.EXPLICIT_GC_URL = `http://${name}.${run}.hello.internal:8080/gc`;
        if (load.mode === 'binary') Object.assign(env, { SEARCH_LOWER_RPS: String(load.lower), SEARCH_UPPER_RPS: String(load.upper),
          SEARCH_RESOLUTION_RPS: String(load.resolution), SEARCH_SECONDS_PER_CANDIDATE: String(load.seconds),
          VEGETA_CONNECTIONS: String(load.connections), REQUEST_TIMEOUT_MS: String(load.timeout) });
        if (variant !== 'baml-only') env.PROCESS_METRICS_URL = `http://${name}.${run}.hello.internal:9091/metrics`;
        const loadTask = new ecs.CfnTaskDefinition(this, `${key}LoadTask`, { family: `${run}-load-${name}`, requiresCompatibilities: ['EC2'], networkMode: 'bridge',
          cpu: String(matrix.load_task_cpu), memory: String(matrix.load_task_memory_mib), executionRoleArn: executionRole.roleArn,
          containerDefinitions: [{ name: 'load', image: images.load, essential: true, cpu: matrix.load_task_cpu, memory: matrix.load_task_memory_mib, stopTimeout: 30,
            environment: Object.entries(env).map(([name, value]) => ({ name, value })), logConfiguration: logConfig(loadLogs, name) }] });
        const loadService = new ecs.CfnService(this, `${key}LoadService`, makeServiceProps(loadTask, name, true));
        loadService.addResourceDependency(loadInstance);
        loadService.addResourceDependency(service);
      }
      if (localLoadCidr) {
        new cdk.CfnOutput(this, 'TargetPublicIp', { value: instance.attrPublicIp });
        new cdk.CfnOutput(this, 'TargetUrl', { value: `http://${instance.attrPublicIp}:8080/` });
      }
    }
    const taskEvents = new events.CfnRule(this, 'TaskEvents', { eventPattern: { source: ['aws.ecs'], 'detail-type': ['ECS Task State Change'], detail: { clusterArn: [cluster.attrArn] } },
      targets: [{ arn: eventLogs.logGroupArn, id: 'TaskEventLogs' }] });
    new logs.CfnResourcePolicy(this, 'EventLogPolicy', { policyName: `${run}-task-events`, policyDocument: cdk.Stack.of(this).toJsonString({
      Version: '2012-10-17', Statement: [{ Effect: 'Allow', Principal: { Service: ['events.amazonaws.com', 'delivery.logs.amazonaws.com'] },
        Action: ['logs:CreateLogStream', 'logs:PutLogEvents'], Resource: eventLogs.logGroupArn,
        Condition: { ArnEquals: { 'aws:SourceArn': taskEvents.attrArn } } }] }) });
    new cloudwatch.CfnDashboard(this, 'Dashboard', { dashboardName: run, dashboardBody: this.toJsonString(dashboard([run], this.region)) });
    cdk.Tags.of(this).add('RunName', run);
    cdk.Tags.of(this).add('Purpose', 'baml-hello-world-rampup-benchmark');
    new cdk.CfnOutput(this, 'ClusterName', { value: run });
    new cdk.CfnOutput(this, 'DashboardUrl', { value: `https://${this.region}.console.aws.amazon.com/cloudwatch/home?region=${this.region}#dashboards:name=${run}` });
    new cdk.CfnOutput(this, 'TaskEventsLogGroup', { value: eventLogs.logGroupName });
    new cdk.CfnOutput(this, 'LoadLogGroup', { value: loadLogs.logGroupName });
    this.addMetadata('LoadProfile', profile);
  }
}

module.exports = { FoundationStack, BenchmarkStack, cells, dashboard, normalizeProfile, validateRun };
