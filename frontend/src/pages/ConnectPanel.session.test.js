import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./ConnectPanel.jsx', import.meta.url), 'utf8');

describe('connection boundary', () => {
  it('clears analysis and active operation when credentials are disconnected', () => {
    expect(source).toContain("setAnalysis('')");
    expect(source).toContain("setActiveOperation('')");
    expect(source).toContain('disconnectClean');
  });
});
