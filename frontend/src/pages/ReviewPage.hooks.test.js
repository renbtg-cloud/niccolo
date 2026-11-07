import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./ReviewPage.jsx', import.meta.url), 'utf8');

describe('ReviewPage hook order', () => {
  it('declares React and TanStack hooks before conditional returns', () => {
    const firstConditionalReturn = source.indexOf("if(!client)return");
    expect(firstConditionalReturn).toBeGreaterThan(0);
    for (const call of ['useState(', 'useQuery(', 'useMutation(', 'useQueryClient(']) {
      expect(source.lastIndexOf(call), call).toBeLessThan(firstConditionalReturn);
    }
  });
});
