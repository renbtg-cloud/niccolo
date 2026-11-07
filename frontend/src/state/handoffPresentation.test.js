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
   expect(receiptSummary({current_handoff_sha256:'b'.repeat(64),receipts:[{receipt_id:'r1',kind:'RECEIVED',note:'got it',recipient_principal_id:'p1',created_at:'2026-11-07 12:00:00+00',handoff_sha256:'a'.repeat(64),matches_current_handoff:false},{receipt_id:'r2',kind:'REVIEWED',note:null,recipient_principal_id:'p1',created_at:'2026-11-07 12:01:00+00',handoff_sha256:'b'.repeat(64),matches_current_handoff:true},{receipt_id:'r3',kind:'REVIEWED',note:'second review',recipient_principal_id:'p2',created_at:'2026-11-07 12:02:00+00',handoff_sha256:'a'.repeat(64),matches_current_handoff:false}],receipt_means_agreement:false,receipt_grants_authority:false})).toEqual({events:[{receiptId:'r1',kind:'RECEIVED',note:'got it',recipient:'p1',createdAt:'2026-11-07 12:00:00+00',handoffSha256:'a'.repeat(64),current:false},{receiptId:'r2',kind:'REVIEWED',note:null,recipient:'p1',createdAt:'2026-11-07 12:01:00+00',handoffSha256:'b'.repeat(64),current:true},{receiptId:'r3',kind:'REVIEWED',note:'second review',recipient:'p2',createdAt:'2026-11-07 12:02:00+00',handoffSha256:'a'.repeat(64),current:false}],currentHandoffSha256:'b'.repeat(64),total:3,currentTotal:1,historicalTotal:2,received:1,reviewed:2,declined:0,currentReceived:0,currentReviewed:1,currentDeclined:0,receiptMeansAgreement:false,receiptGrantsAuthority:false});
 });
});
