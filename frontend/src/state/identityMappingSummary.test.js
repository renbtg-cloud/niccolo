import { describe, expect, it } from 'vitest';
import { identityMappingSummary } from './identityMappingSummary.js';

describe('identity mapping summary', () => {
  it('counts explicit mappings without treating coverage as truth', () => {
    expect(identityMappingSummary(
      [{speaker_label:'Ana'},{speaker_label:'Bob'},{speaker_label:'Unknown'}],
      [{speaker_label:'Ana',person_id:'p1'},{speaker_label:'Bob',person_id:'p2'},{speaker_label:'Unknown',person_id:null}],
    )).toEqual({speakerCount:3,mappedCount:2,unmappedCount:1,allMapped:false,mappingEstablishesIdentityTruth:false});
  });
  it('marks allMapped only for a non-empty fully explicit set', () => {
    expect(identityMappingSummary([{speaker_label:'Ana'}],[{speaker_label:'Ana',person_id:'p1'}]).allMapped).toBe(true);
    expect(identityMappingSummary([],[]).allMapped).toBe(false);
  });
});
