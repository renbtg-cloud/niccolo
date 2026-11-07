import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const identities=readFileSync(new URL('./IdentitiesPage.jsx',import.meta.url),'utf8');
const operations=readFileSync(new URL('./OperationsPage.jsx',import.meta.url),'utf8');
const situation=readFileSync(new URL('./SituationPage.jsx',import.meta.url),'utf8');

describe('human steering surfaces',()=>{
  it('makes identity review discoverable but non-gating',()=>{
    expect(situation).toContain('Review who source labels refer to →');
    expect(identities).toContain('label(s) remain unmapped. That is allowed');
    expect(identities).toContain('Coverage does not prove that the mappings are correct.');
    expect(identities).toContain('Continue to Analyze →');
  });
  it('presents controller pause as explicit human input rather than inferred consent',()=>{
    expect(operations).toContain('Niccolò needs your input');
    expect(operations).toContain('it is not inferred consent for any external action');
    expect(operations).toContain('Continue with this understanding');
    expect(operations).toContain('Reconsider the reasoning plan');
    expect(operations).toContain('Stop this analysis');
  });
});
