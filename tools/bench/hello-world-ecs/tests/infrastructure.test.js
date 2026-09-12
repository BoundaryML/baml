const { test } = require('node:test');
const assert = require('node:assert/strict');
const cdk = require('aws-cdk-lib');
const { Template } = require('aws-cdk-lib/assertions');
const { FoundationStack, BenchmarkStack, cells, dashboard, validateRun } = require('../infra/benchmark');
const images = Object.fromEntries([...cells().map(cell => cell.name), 'load'].map(name => [name, `123456789012.dkr.ecr.us-east-1.amazonaws.com/hello@sha256:${'a'.repeat(64)}`]));
function fixtures() {
  const app = new cdk.App();
  const env = { account: '123456789012', region: 'us-east-1' };
  const foundation = new FoundationStack(app, 'hello-world-foundation', { env });
  return { app, foundation, env };
}
function resources(stack) { return Template.fromStack(stack).toJSON().Resources; }
test('ten dedicated hosts and tasks have fixed architecture and resource limits without autoscaling', () => {
  const f = fixtures();
  const r = resources(new BenchmarkStack(f.app, 'steady-one', { ...f, images, rate: 100 }));
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
});
test('concurrent runs share only foundation and immutable images', () => {
  const f = fixtures();
  const stackA = new BenchmarkStack(f.app, 'steady-ten', { ...f, images, rate: 10 });
  const stackB = new BenchmarkStack(f.app, 'steady-hundred', { ...f, images, rate: 100 });
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
    assert.equal(task.ContainerDefinitions[0].Environment.find(e => e.Name === 'RATE_PER_TARGET').Value, expected);
  }
  assert.equal(Object.values(resources(f.foundation)).filter(v => v.Type === 'AWS::EC2::VPC').length, 1);
});
test('invalid images, run names, and rates fail before deployment', () => {
  for (const rate of [0, -1, 1.5, true, 10001]) assert.throws(() => validateRun('run-one', images, rate));
  assert.throws(() => validateRun('run-one', {}, 100));
  assert.throws(() => validateRun('run-one', { ...images, load: 'image:latest' }, 100));
  assert.throws(() => validateRun('../bad', images, 100));
});
test('comparison dashboard keeps twenty series and valid unique metric ids', () => {
  for (const widget of dashboard(['steady-ten', 'steady-hundred'], 'us-east-1').widgets) {
    const metrics = widget.properties.metrics;
    assert([20, 40].includes(metrics.length));
    assert.equal(new Set(metrics.filter(m => typeof m[0] === 'string').map(m => m.at(-1).label)).size, 20);
    assert.equal(new Set(metrics.map(m => m.at(-1).id)).size, metrics.length);
  }
});
