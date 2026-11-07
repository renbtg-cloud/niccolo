import { describe, expect, it } from 'vitest';
import { preparedDemoUrl } from './preparedDemoLink.js';

describe('prepared demo link', () => {
  it('contains workspace and Situation but never credentials', () => {
    const url=preparedDemoUrl({origin:'https://demo.example',pathname:'/niccolo'},{workspace:'w',situation:'s',token:'secret'});
    expect(url).toBe('https://demo.example/niccolo?workspace=w&situation=s');
    expect(url).not.toContain('secret');
    expect(url).not.toContain('token');
  });
});
