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

  it('requires a separate explicit action before exporting examined source text', () => {
    expect(source).toContain('presentationSourceExportPreview(analysis)');
    expect(source).toContain('presentationPackageWithExaminedSources(');
    expect(source).toContain('pendingSensitiveExport.current?.analysis!==analysis');
    expect(source).toContain('pendingSensitiveExport.current.eventId');
    expect(source).toContain('presentationExportEvents(analysis,25)');
    expect(source).toContain('Include examined source text');
    expect(source).toContain('Scope preview:');
    expect(source).toContain('examined_source_text_bytes');
    expect(source).toContain('within_export_bounds');
    expect(source).toContain('Sensitive export.');
    expect(source).toContain('only source fragments recorded as examined by this analysis');
    expect(source).toContain('does not prove those excerpts are true');
    expect(source).toContain('No remote publication occurred; the authorization event was recorded locally.');
    expect(source).toContain('Recent sensitive export audit');
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
