import { describe, expect, it } from 'vitest';
import fs from 'node:fs';

const source = fs.readFileSync(new URL('./InspectorPage.jsx', import.meta.url), 'utf8');

describe('Inspector integration diagnostics', () => {
  it('loads workspace delivery diagnostics even before an analysis is selected', () => {
    expect(source).toContain("['integration diagnostics',()=>client.integrationDiagnostics(),true]");
    expect(source).toContain('enabled:!!client&&(always||!!analysis)');
  });

  it('keeps attributed upstream-processing declarations visible without treating them as analysis output', () => {
    expect(source).toContain("['upstream processing declarations',()=>client.integrationProcessingDeclarations(0,100),true]");
  });
});
