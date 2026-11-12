import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const app = readFileSync(new URL('../App.jsx', import.meta.url), 'utf8');
const page = readFileSync(new URL('./ForensicPage.jsx', import.meta.url), 'utf8');

describe('prior Niccolò export quarantine surface', () => {
  it('keeps prior exports under Technical rather than the primary Situation → Analyze → Review path', () => {
    expect(app).toContain("['/forensic','Prior exports']");
    expect(app).toContain('<Route path="/forensic" element={<ForensicPage/>}/>');
    expect(app).toContain('<details className="advanced-nav">');
  });

  it('makes quarantine semantics unavoidable before import or binding', () => {
    expect(page).toContain('untrusted historical material');
    expect(page).toContain('does <strong>not</strong> make its claims true');
    expect(page).toContain('non-evidence historical context');
    expect(page).toContain('does not rewrite the frozen analysis input');
    expect(page).toContain('published answer, if any, remains unchanged.');
  });

  it('reuses stable batch and binding IDs after an interrupted response', () => {
    expect(page).toContain('pendingImport.current?.signature !== signature');
    expect(page).toContain('pendingImport.current.batchId');
    expect(page).toContain('pendingBinding.current?.signature !== signature');
    expect(page).toContain('pendingBinding.current.bindingId');
  });

  it('allows retained bytes to be downloaded without presenting them as native evidence', () => {
    expect(page).toContain('Download quarantined original bytes');
    expect(page).toContain('not a native evidence record');
    expect(page).toContain("source_kind === 'ORIGINAL_SOURCE_BYTES'");
    expect(page).toContain('downloadForensicOriginal');
  });

  it('lets a human explicitly start a new analysis that carries selected quarantined context', () => {
    expect(page).toContain('launchApprovedReanalysis');
    expect(page).toContain('pendingReanalysis.current?.signature !== signature');
    expect(page).toContain('Start new analysis with selected history');
    expect(page).toContain('carried forward as non-evidence context before reasoning starts');
    expect(page).toContain("navigate('/operations')");
    expect(page).toContain('setActiveOperation(result.agenticOperation)');
  });

  it('supports bounded pagination over batches and items', () => {
    expect(page).toContain('Previous imports');
    expect(page).toContain('Next imports');
    expect(page).toContain('Previous items');
    expect(page).toContain('Next items');
    expect(page).toContain('next_before_batch_id');
    expect(page).toContain('next_after_source_path');
  });
});
