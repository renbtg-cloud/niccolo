import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./SituationPage.jsx', import.meta.url), 'utf8');

describe('Situation multi-source first-user import', () => {
  it('accepts several supported files in one explicit user selection', () => {
    expect(source).toContain('multiple');
    expect(source).toContain('setFiles(Array.from(event.target.files ?? []))');
    expect(source).toContain('for (const selected of selectedFiles)');
    expect(source).toContain('Import and attach');
  });

  it('keeps stable document IDs across a partially failed retry', () => {
    expect(source).toContain('pendingDocumentIds = useRef(new Map())');
    expect(source).toContain('pendingDocumentIds.current.get(signature)');
    expect(source).toContain('pendingDocumentIds.current.set(signature, documentId)');
    expect(source).toContain('retrying the same selection replays rather than duplicates');
  });

  it('shows bounded per-file progress and preserves completed work after a partial failure', () => {
    expect(source).toContain('setUploadProgress({ done: 0, total: selectedFiles.length');
    expect(source).toContain('processing <strong>{uploadProgress.current}</strong>');
    expect(source).toContain('Successfully completed files remain attached');
    expect(source).toContain('will replay safely if you retry the same selection');
  });

  it('clears retry identities only after the whole selected batch succeeds or the user changes selection', () => {
    expect(source).toContain('pendingDocumentIds.current.clear()');
    expect(source).toContain('setFiles([])');
    expect(source).toContain('onError: refresh');
  });
});
