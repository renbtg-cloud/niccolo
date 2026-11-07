export function handoffSummary(payload){
 const handoff=payload?.handoff;
 if(!handoff)return null;
 const clarifications=handoff.clarifications??[];
 const candidates=handoff.candidate_interpretations??[];
 return {
   version:handoff.version,
   sha256:payload.handoff_sha256??'',
   claimCount:(handoff.claims??[]).length,
   openClaimQuestionCount:(handoff.open_questions??[]).length,
   clarificationCount:clarifications.length,
   unansweredClarificationCount:clarifications.filter(x=>!x.answered).length,
   candidateInterpretationCount:candidates.length,
   nonAuthority:handoff.handoff_grants_authority===false&&handoff.handoff_starts_analysis===false&&handoff.handoff_executes_actions===false,
   readModelIsAuthority:payload.read_model_is_authority===true,
   readModelExecutesActions:payload.read_model_executes_actions===true,
 };
}

export function receiptSummary(payload){
 const rows=payload?.receipts??[];
 const counts={RECEIVED:0,REVIEWED:0,DECLINED:0};
 const currentCounts={RECEIVED:0,REVIEWED:0,DECLINED:0};
 for(const row of rows){
   if(Object.hasOwn(counts,row.kind))counts[row.kind]+=1;
   if(row.matches_current_handoff===true&&Object.hasOwn(currentCounts,row.kind))currentCounts[row.kind]+=1;
 }
 const currentTotal=currentCounts.RECEIVED+currentCounts.REVIEWED+currentCounts.DECLINED;
 return {
   events:rows.map(row=>({receiptId:row.receipt_id,kind:row.kind,note:row.note??null,recipient:row.recipient_principal_id,createdAt:row.created_at,handoffSha256:row.handoff_sha256,current:row.matches_current_handoff===true})),
   currentHandoffSha256:payload?.current_handoff_sha256??'',
   total:rows.length,
   currentTotal,
   historicalTotal:rows.length-currentTotal,
   received:counts.RECEIVED,
   reviewed:counts.REVIEWED,
   declined:counts.DECLINED,
   currentReceived:currentCounts.RECEIVED,
   currentReviewed:currentCounts.REVIEWED,
   currentDeclined:currentCounts.DECLINED,
   receiptMeansAgreement:payload?.receipt_means_agreement===true,
   receiptGrantsAuthority:payload?.receipt_grants_authority===true,
 };
}
