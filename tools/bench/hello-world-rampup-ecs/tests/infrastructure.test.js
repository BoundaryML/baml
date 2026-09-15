const { test } = require('node:test');
const assert = require('node:assert/strict');
const cdk = require('aws-cdk-lib');
const { Template } = require('aws-cdk-lib/assertions');
const { FoundationStack, BenchmarkStack, cells, dashboard, normalizeProfile, validateRun } = require('../infra/benchmark');
const images = Object.fromEntries([...cells().map(cell => cell.name), 'load'].map(name => [name, `123456789012.dkr.ecr.us-east-1.amazonaws.com/hello@sha256:${'a'.repeat(64)}`]));
const ramp = { mode: 'ramp', start_rate_per_target: 100, increase_rate_per_target: 100, increase_every_seconds: 30,
  maximum_rate_per_target: 25000, on_seconds: 4, off_seconds: 1 };
function fixtures() {
  const app = new cdk.App();
  const env = { account: '123456789012', region: 'us-east-1' };
  const foundation = new FoundationStack(app, 'hello-world-foundation', { env });
  return { app, foundation, env };
}
function resources(stack) { return Template.fromStack(stack).toJSON().Resources; }
test('ten dedicated hosts and tasks have fixed architecture and resource limits without autoscaling', () => {
  const f = fixtures();
  const r = resources(new BenchmarkStack(f.app, 'ramp-one', { ...f, images, profile: ramp }));
  const values = Object.values(r);
  assert.equal(values.filter(v => v.Type === 'AWS::EC2::Instance').length, 11);
  assert(!values.some(v => /Scaling|CapacityProvider/.test(v.Type)));
  for (const host of values.filter(v => v.Type === 'AWS::EC2::Instance')) assert(JSON.stringify(host.Properties.UserData).includes('swapoff --all'));
  const tasks = values.filter(v => v.Type === 'AWS::ECS::TaskDefinition' && v.Properties.NetworkMode === 'awsvpc').map(v => v.Properties);
  assert.equal(tasks.length, 10);
  assert.equal(tasks.filter(t => t.RuntimePlatform.CpuArchitecture === 'ARM64').length, 5);
  for (const task of tasks) {
    assert.equal(task.Cpu, '1024'); assert.equal(task.Memory, '1024');
    assert.deepEqual(task.RequiresCompatibilities, ['EC2']);
    assert.equal(task.ContainerDefinitions.length, 1);
    assert.equal(task.ContainerDefinitions[0].Memory, 1024);
    assert.equal(task.ContainerDefinitions[0].LinuxParameters, undefined);
  }
  const services = values.filter(v => v.Type === 'AWS::ECS::Service' && v.Properties.NetworkConfiguration).map(v => v.Properties);
  assert.equal(new Set(services.map(s => s.PlacementConstraints[0].Expression)).size, 10);
  for (const service of services) { assert.equal(service.DesiredCount, 1); assert.equal(service.DeploymentConfiguration.MaximumPercent, 100); }
  const loadTasks = values.filter(v => v.Type === 'AWS::ECS::TaskDefinition' && v.Properties.NetworkMode === 'bridge').map(v => v.Properties);
  assert.equal(loadTasks.length, 10);
  for (const task of loadTasks) { assert.equal(task.Cpu, '4096'); assert.equal(task.Memory, '1024'); }
});
test('concurrent runs share only foundation and immutable images', () => {
  const f = fixtures();
  const sustain = rate => ({ mode: 'sustain', rate_per_target: rate, on_seconds: 4, off_seconds: 1 });
  const stackA = new BenchmarkStack(f.app, 'steady-ten', { ...f, images, profile: sustain(10) });
  const stackB = new BenchmarkStack(f.app, 'steady-hundred', { ...f, images, profile: sustain(100) });
  const a = resources(stackA), b = resources(stackB);
  for (const field of ['ClusterName', 'ServiceName', 'Family', 'LogGroupName', 'DashboardName']) {
    const names = r => Object.values(r).map(v => v.Properties?.[field]).filter(v => typeof v === 'string');
    const aNames = names(a), bNames = new Set(names(b));
    assert(aNames.length > 0);
    assert(aNames.every(name => !bNames.has(name)));
  }
  const namespace = r => Object.values(r).find(v => v.Type === 'AWS::ServiceDiscovery::PrivateDnsNamespace').Properties.Name;
  assert.notEqual(namespace(a), namespace(b));
  for (const [r, expected] of [[a, '10'], [b, '100']]) {
    const task = r.PythonOnlyArm64LoadTask.Properties;
    assert.equal(task.ContainerDefinitions[0].Environment.find(e => e.Name === 'START_RATE_PER_TARGET').Value, expected);
  }
  assert.equal(Object.values(resources(f.foundation)).filter(v => v.Type === 'AWS::EC2::VPC').length, 1);
});
test('profiles support fixed per-implementation rates and reject invalid duty cycles and rates', () => {
  const fixed = { mode: 'sustain', rate_per_target: { 'python-only': 1200, 'python-baml': 900, 'node-only': 3100,
    'node-baml': 2400, 'baml-only': 100 }, on_seconds: 4, off_seconds: 1 };
  assert.equal(normalizeProfile(fixed, { name: 'node-only-arm64', variant: 'node-only' }).start, 3100);
  assert.equal(normalizeProfile({ ...fixed, rate_per_target: { ...fixed.rate_per_target, 'node-only-arm64': 3200 } },
                               { name: 'node-only-arm64', variant: 'node-only' }).start, 3200);
  const continuation = { ...ramp, start_rate_per_target: { 'node-only': 4000, 'python-only': 100, 'python-baml': 100, 'node-baml': 100, 'baml-only': 100 },
    increase_rate_per_target: { 'node-only': 100, 'python-only': 0, 'python-baml': 0, 'node-baml': 0, 'baml-only': 0 },
    maximum_rate_per_target: { 'node-only': 25000, 'python-only': 100, 'python-baml': 100, 'node-baml': 100, 'baml-only': 100 } };
  assert.deepEqual(normalizeProfile(continuation, { name: 'node-only-x64', variant: 'node-only' }),
                   { mode: 'ramp', start: 4000, increase: 100, every: 30, maximum: 25000, on: 4, off: 1 });
  assert.deepEqual(normalizeProfile(continuation, { name: 'python-only-x64', variant: 'python-only' }),
                   { mode: 'ramp', start: 100, increase: 0, every: 30, maximum: 100, on: 4, off: 1 });
  for (const rate of [0, -1, 1.5, true, 50001]) assert.throws(() => validateRun('run-one', images, { ...fixed, rate_per_target: rate }));
  assert.throws(() => validateRun('run-one', images, { ...ramp, on_seconds: 3 }));
  assert.throws(() => validateRun('run-one', images, { ...ramp, increase_every_seconds: 31 }));
  assert.throws(() => validateRun('run-one', {}, ramp));
  assert.throws(() => validateRun('run-one', { ...images, load: 'image:latest' }, ramp));
  assert.throws(() => validateRun('../bad', images, ramp));
});
test('binary profile validates and reaches the AWS load task', () => {
  const binary = { mode: 'binary', lower_rate_per_target: 5000, upper_rate_per_target: 10000, resolution_rps: 100,
    seconds_per_candidate: 30, connections: 1000, request_timeout_ms: 800, on_seconds: 4, off_seconds: 1 };
  const normalized = normalizeProfile(binary, { name: 'node-only-arm64', variant: 'node-only' });
  assert.deepEqual(normalized, { mode: 'binary', start: 5000, increase: 0, every: 30, maximum: 10000, lower: 5000,
    upper: 10000, resolution: 100, seconds: 30, connections: 1000, timeout: 800, on: 4, off: 1 });
  const f = fixtures();
  const r = resources(new BenchmarkStack(f.app, 'binary-node', { ...f, images, profile: binary, targetCell: 'node-only-arm64' }));
  const environment = r.NodeOnlyArm64LoadTask.Properties.ContainerDefinitions[0].Environment;
  const value = name => environment.find(item => item.Name === name).Value;
  assert.equal(value('LOAD_MODE'), 'binary');
  assert.equal(value('SEARCH_UPPER_RPS'), '10000');
  assert.equal(value('VEGETA_CONNECTIONS'), '1000');
  assert.throws(() => normalizeProfile({ ...binary, request_timeout_ms: 1000 }, { name: 'node-only-arm64', variant: 'node-only' }));
});
test('comparison dashboard keeps twenty series and valid unique metric ids', () => {
  for (const widget of dashboard(['steady-ten', 'steady-hundred'], 'us-east-1').widgets) {
    const metrics = widget.properties.metrics;
    assert([20, 40].includes(metrics.length));
    assert.equal(new Set(metrics.filter(m => typeof m[0] === 'string').map(m => m.at(-1).label)).size, 20);
    assert.equal(new Set(metrics.map(m => m.at(-1).id)).size, metrics.length);
  }
});
test('local load mode exposes only one target to one client and creates no AWS load generator', () => {
  const f = fixtures();
  const r = resources(new BenchmarkStack(f.app, 'local-node', { ...f, images, profile: ramp, targetCell: 'node-only-arm64', loadCount: 0, localLoadCidr: '203.0.113.8/32' }));
  const values = Object.values(r);
  assert.equal(values.filter(v => v.Type === 'AWS::EC2::Instance').length, 1);
  const tasks = values.filter(v => v.Type === 'AWS::ECS::TaskDefinition').map(v => v.Properties);
  assert.equal(tasks.length, 1);
  assert.equal(tasks[0].NetworkMode, 'bridge');
  assert.equal(tasks[0].RuntimePlatform.CpuArchitecture, 'ARM64');
  assert.deepEqual(tasks[0].ContainerDefinitions[0].PortMappings[0], { ContainerPort: 8080, HostPort: 8080 });
  const ingress = values.flatMap(v => v.Properties?.SecurityGroupIngress || []);
  assert.equal(ingress.filter(rule => rule.CidrIp === '203.0.113.8/32').length, 2);
  assert.throws(() => new BenchmarkStack(f.app, 'bad-local', { ...f, images, profile: ramp, targetCell: 'node-only-arm64', localLoadCidr: '0.0.0.0/0' }));
});
