import { describe, expect, it } from 'vitest';
import { projectLabel, projectLabels } from './project-label';

describe('projectLabel', () => {
  it('prefers the name the package declares', () => {
    expect(projectLabel('/Users/kai/baml-demos/aie', 'my_app')).toBe('my_app');
  });

  it('falls back to the project directory', () => {
    expect(projectLabel('/Users/kai/baml-demos/python-discord-bot')).toBe(
      'python-discord-bot',
    );
  });

  it('never falls back to the unnamed default', () => {
    // Two unnamed projects must not both read "user".
    expect(projectLabel('/a/one')).not.toBe(projectLabel('/a/two'));
  });

  it('looks past a bare baml_src directory', () => {
    expect(projectLabel('/Users/kai/repos/myapp/baml_src')).toBe('myapp');
  });

  it('handles trailing separators and windows paths', () => {
    expect(projectLabel('/Users/kai/repos/myapp/')).toBe('myapp');
    expect(projectLabel('C:\\Users\\kai\\repos\\myapp')).toBe('myapp');
  });
});

describe('projectLabels', () => {
  it('leaves distinct labels alone', () => {
    const labels = projectLabels([
      { path: '/demos/aie' },
      { path: '/demos/bamlcode' },
    ]);
    expect([...labels.values()]).toEqual(['aie', 'bamlcode']);
  });

  it('disambiguates two projects that would read the same', () => {
    const labels = projectLabels([
      { path: '/work/alpha/service' },
      { path: '/work/beta/service' },
    ]);
    expect(labels.get('/work/alpha/service')).toBe('alpha/service');
    expect(labels.get('/work/beta/service')).toBe('beta/service');
  });

  it('disambiguates by directory even when a declared name repeats', () => {
    const labels = projectLabels([
      { name: 'svc', path: '/work/alpha/service' },
      { name: 'svc', path: '/work/beta/service' },
    ]);
    expect(labels.get('/work/alpha/service')).toBe('alpha/svc');
    expect(labels.get('/work/beta/service')).toBe('beta/svc');
  });
});
