import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source=readFileSync(new URL('./RuntimeCapacityPanel.jsx',import.meta.url),'utf8');

describe('runtime capacity presentation',()=>{
  it('keeps resource pressure operational rather than epistemic',()=>{
    expect(source).toContain('Runtime capacity: {pressure}');
    expect(source).toContain('Critical pressure:');
    expect(source).toContain('defer explicitly nonessential reasoning passes');
    expect(source).toContain('preserving synthesis, adversarial challenge and human control');
    expect(source).toContain('not confidence, epistemic quality, completeness or truth');
    expect(source).toContain('does not mutate host resource limits');
  });
  it('surfaces PID capacity as operational headroom, not epistemic quality',()=>{
    expect(source).toContain("PID_CAPACITY: 'PID headroom'");
    expect(source).toContain("PID_LIMIT_EXHAUSTED: 'PID limit exhausted'");
    expect(source).toContain('Observed PID headroom:');
    expect(source).toContain('process/thread slot(s)');
  });
  it('does not interpret missing telemetry as permission to degrade',()=>{
    expect(source).toContain('Missing telemetry does not authorize skipping reasoning work.');
  });
});
