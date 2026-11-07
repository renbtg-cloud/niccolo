import { describe, expect, it } from 'vitest';
import { handoffSummary, receiptSummary } from './handoffPresentation.js';

describe('handoffSummary',()=>{
 it('summarizes durable handoff without inventing authority',()=>{
   const v=handoffSummary({
     handoff_sha256:'a'.repeat(64),
     read_model_is_authority:false,
     read_model_executes_actions:false,
     handoff:{
       version:2,
       claims:[{},{}],
       open_questions:[{}],
       clarifications:[{answered:false},{answered:true}],
       candidate_interpretations:[{}],
       handoff_grants_authority:false,
       handoff_starts_analysis:false,
       handoff_executes_actions:false,
     },
   });
   expect(v).toEqual({
     version:2,
     sha256:'a'.repeat(64),
     claimCount:2,
     openClaimQuestionCount:1,
     clarificationCount:2,
     unansweredClarificationCount:1,
     candidateInterpretationCount:1,
     nonAuthority:true,
     readModelIsAuthority:false,
     readModelExecutesActions:false,
   });
 });
 it('returns null when no handoff exists',()=>expect(handoffSummary(null)).toBeNull());
 it('summarizes receipts without equating them with agreement or authority',()=>{
   expect(receiptSummary({receipts:[{kind:'RECEIVED'},{kind:'REVIEWED'},{kind:'REVIEWED'}],receipt_means_agreement:false,receipt_grants_authority:false})).toEqual({total:3,received:1,reviewed:2,declined:0,receiptMeansAgreement:false,receiptGrantsAuthority:false});
 });
});
