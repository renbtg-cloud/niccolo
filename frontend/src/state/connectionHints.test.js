import { describe, expect, it } from 'vitest';
import { connectionHints, hasPreparedConnectionHints } from './connectionHints.js';

describe('connection hints', () => {
  it('reads only non-secret workspace and Situation identifiers from the URL', () => {
    expect(connectionHints('?workspace=w&situation=s&token=leak')).toEqual({ workspace: 'w', situation: 's', token: '' });
  });
  it('uses empty values when no hints are present', () => {
    expect(connectionHints('')).toEqual({ workspace: '', situation: '', token: '' });
  });
  it('treats a link as prepared only when both non-secret identifiers are present', () => {
    expect(hasPreparedConnectionHints(connectionHints('?workspace=w&situation=s'))).toBe(true);
    expect(hasPreparedConnectionHints(connectionHints('?workspace=w'))).toBe(false);
    expect(hasPreparedConnectionHints(connectionHints('?situation=s'))).toBe(false);
  });
});
