import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./useClient.js', import.meta.url), 'utf8');

describe('client session hook', () => {
  it('subscribes only to stable scalar values from Zustand 5', () => {
    for (const key of ['workspace', 'situation', 'token', 'connected']) {
      expect(source).toContain(`useSessionStore((s) => s.${key})`);
    }
    expect(source).not.toMatch(/useSessionStore\(\(s\)\s*=>\s*\(\{/);
  });

  it('recreates the HTTP client whenever the session identity changes', () => {
    expect(source).toContain('[workspace, situation, token, connected]');
    expect(source).toContain('connected ? new NiccoloClient(');
  });
});
