import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const connect = readFileSync(new URL('./ConnectPanel.jsx', import.meta.url), 'utf8');
const situation = readFileSync(new URL('./SituationPage.jsx', import.meta.url), 'utf8');
const review = readFileSync(new URL('./ReviewPage.jsx', import.meta.url), 'utf8');

describe('buyer-facing primary surfaces', () => {
  it('collapses prepared demo identifiers while keeping the bearer token explicit', () => {
    expect(connect).toContain('Prepared Situation');
    expect(connect).toContain('<summary>Connection identifiers</summary>');
    expect(connect).toContain('Bearer tokens are never read from the URL or persisted.');
    expect(connect).toContain('Share prepared demo link');
    expect(connect).toContain('Send the bearer token separately.');
  });

  it('describes Situation preparation without blocking a first pass', () => {
    expect(situation).toContain('What Niccolò has to work with');
    expect(situation).toContain('not a completeness score and not a gate');
    expect(situation).toContain('Analyze this Situation →');
    expect(situation).toContain('Connect this session first.');
    expect(situation).not.toContain('Connect to the BFF first.');
  });

  it('retains upload document identity when a partial import must be retried', () => {
    expect(situation).toContain('pendingDocumentId.current ?? crypto.randomUUID()');
    expect(situation).toContain('pendingDocumentId.current = documentId');
    expect(situation).toContain('pendingDocumentId.current = null');
    expect(situation).toContain('onError: refresh');
    expect(situation).toContain('pendingContribution.current?.signature !== signature');
    expect(situation).toContain('pendingCorrection.current?.signature !== signature');
    expect(situation).toContain('pendingContribution.current.id');
    expect(situation).toContain('pendingCorrection.current.id');
  });

  it('keeps Review self-contained with answer and concrete competing interpretations', () => {
    expect(review).toContain("<h3>Niccolò's answer</h3>");
    expect(review).toContain('Competing interpretations considered');
    expect(review).toContain('These remain candidates, not facts, and are not automatically ranked.');
  });
});
