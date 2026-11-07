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
 for(const row of rows)if(Object.hasOwn(counts,row.kind))counts[row.kind]+=1;
 return {
   total:rows.length,
   received:counts.RECEIVED,
   reviewed:counts.REVIEWED,
   declined:counts.DECLINED,
   receiptMeansAgreement:payload?.receipt_means_agreement===true,
   receiptGrantsAuthority:payload?.receipt_grants_authority===true,
 };
}
