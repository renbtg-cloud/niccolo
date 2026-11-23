import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const identities=readFileSync(new URL('./IdentitiesPage.jsx',import.meta.url),'utf8');
const operations=readFileSync(new URL('./OperationsPage.jsx',import.meta.url),'utf8');
const situation=readFileSync(new URL('./SituationPage.jsx',import.meta.url),'utf8');
const artifacts=readFileSync(new URL('./ArtifactReferencePanel.jsx',import.meta.url),'utf8');

describe('human steering surfaces',()=>{
  it('keeps human attribution explicit but non-gating',()=>{
    expect(situation).toContain('<ArtifactReferencePanel client={client} onSummary={setArtifactSummary}/>');
    expect(artifacts).toContain('Who produced it');
    expect(artifacts).toContain('Production method');
    expect(artifacts).toContain('Scope within artifact');
    expect(artifacts).toContain('Qualification');
    expect(artifacts).toContain('Limitations');
    expect(artifacts).toContain('Representations remain attributable and challengeable.');
    expect(identities).toContain('Coverage does not prove that the mappings are correct.');
  });
  it('presents controller pause as explicit human input rather than inferred consent',()=>{
    expect(operations).toContain('Niccolò needs your input');
    expect(operations).toContain('it is not inferred consent for any external action');
    expect(operations).toContain('Continue with this understanding');
    expect(operations).toContain('Reconsider the reasoning plan');
    expect(operations).toContain('Stop this analysis');
  });
});
