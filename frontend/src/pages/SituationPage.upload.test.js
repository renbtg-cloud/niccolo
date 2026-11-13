import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./SituationPage.jsx', import.meta.url), 'utf8');

describe('Situation multi-source first-user import', () => {
  it('states that txt means a WhatsApp chat export, not generic text', () => {
    expect(source).toContain('a WhatsApp chat export (.txt)');
    expect(source).toContain('Generic text files are not interpreted as WhatsApp conversations.');
    expect(source).not.toContain('WhatsApp/text export');
  });

  it('accepts several supported files in one explicit user selection', () => {
    expect(source).toContain('multiple');
    expect(source).toContain('setFiles(Array.from(event.target.files ?? []))');
    expect(source).toContain('for (const [index, selected] of selectedFiles.entries())');
    expect(source).toContain('Import and attach');
  });

  it('keeps stable document IDs across a partially failed retry', () => {
    expect(source).toContain('pendingDocumentIds = useRef(new Map())');
    expect(source).toContain('pendingDocumentIds.current.get(selected)');
    expect(source).toContain('pendingDocumentIds.current.set(selected, documentId)');
    expect(source).toContain('retrying the same selection replays rather than duplicates');
  });

  it('renders duplicate-metadata selected files with distinct React keys', () => {
    expect(source).toContain('map((selected, index) => <li key={');
    expect(source).toContain(':${index}');
  });

  it('does not alias distinct files that happen to share metadata', () => {
    expect(source).toContain('same name/size/mtime yet different bytes');
    expect(source).not.toContain('JSON.stringify([\n          selected.name');
  });

  it('shows bounded per-file progress and preserves completed work after a partial failure', () => {
    expect(source).toContain('setUploadProgress({ done: 0, total: selectedFiles.length');
    expect(source).toContain('processing <strong>{uploadProgress.current}</strong>');
    expect(source).toContain('Successfully completed files remain attached');
    expect(source).toContain('will replay safely if you retry the same selection');
  });

  it('clears the native file input after a successful batch so the same files can be selected again', () => {
    expect(source).toContain('sourceInputRef = useRef(null)');
    expect(source).toContain('ref={sourceInputRef}');
    expect(source).toContain("sourceInputRef.current.value = ''");
  });

  it('clears retry identities only after the whole selected batch succeeds or the user changes selection', () => {
    expect(source).toContain('pendingDocumentIds.current.clear()');
    expect(source).toContain('setFiles([])');
    expect(source).toContain('onError: refresh');
  });

  it('keeps receipt publication attached to the visible source record rather than the upload action', () => {
    expect(source).toContain("import { SourceIntakePublicationPanel }");
    expect(source).toContain('<SourceIntakePublicationPanel document={document}/>');
    expect(source.indexOf('<SourceIntakePublicationPanel document={document}/>')).toBeGreaterThan(source.indexOf('<h3>Source catalog</h3>'));
  });
});
