import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./ReviewPage.jsx', import.meta.url), 'utf8');

describe('ReviewPage hook order', () => {
  it('reuses the same clarification ID after an interrupted response', () => {
    expect(source).toContain('pendingClarification.current?.signature!==signature');
    expect(source).toContain('client.answerClarification(analysis,questionId,pendingClarification.current.id,text)');
    expect(source).toContain('pendingClarification.current=null;');
  });

  it('reuses the same handoff receipt ID without confusing review kinds', () => {
    expect(source).toContain('hash:handoffView.sha256,kind,note');
    expect(source).toContain('pendingReceipt.current?.signature!==signature');
    expect(source).toContain('client.recordHandoffReceipt(analysis,pendingReceipt.current.id,handoffView.sha256,kind,note)');
    expect(source).toContain('pendingReceipt.current=null;');
  });

  it('surfaces explicitly selected prior-export context without calling it evidence', () => {
    expect(source).toContain('useForensicContext()');
    expect(source).toContain('Imported historical context');
    expect(source).toContain('current non-evidence context');
    expect(source).toContain('not part of the frozen analysis input');
    expect(source).toContain('documentary evidence: no');
  });

  it('declares React and TanStack hooks before conditional returns', () => {
    const firstConditionalReturn = source.indexOf("if(!client)return");
    expect(firstConditionalReturn).toBeGreaterThan(0);
    for (const call of ['useState(', 'useQuery(', 'useMutation(', 'useQueryClient(']) {
      expect(source.lastIndexOf(call), call).toBeLessThan(firstConditionalReturn);
    }
  });
});
