import { useEpistemicView } from '../hooks/useEpistemicView.js';
import { useClient } from '../hooks/useClient.js';
import { useUiStore } from '../stores/ui.js';

function kindLabel(kind){return kind.toLowerCase().replaceAll('_',' ');}
function Assumption({item}){return <li><strong>{item.statement}</strong><div><small>Basis: {item.basis_note} · Status: {kindLabel(item.latest_status)}</small></div></li>}
function Gap({item}){return <li><strong>{item.description}</strong><div><small>Status: {kindLabel(item.state)}{item.requested_source_kind?` · Useful source: ${kindLabel(item.requested_source_kind)}`:''}</small></div></li>}
function Alternative({item}){return <li><strong>{item.text}</strong><div><small>{item.rationale}</small></div></li>}
function CandidateInterpretation({item}){return <li><strong>{item.text}</strong><div><small>{item.rationale}</small></div></li>}
function SourceLink({item}){return <li className="provenance-link"><div><strong>{kindLabel(item.relation)}</strong> · {item.speaker_label||'Unknown speaker'}</div><blockquote>{item.content}</blockquote><small>{item.format} · {item.locator} · fragment #{item.ordinal} · {kindLabel(item.source_class)}</small></li>}
function ContextLink({item}){return <li className="context-link"><div><strong>{kindLabel(item.relation)}</strong> · {kindLabel(item.kind)}</div><blockquote>{item.text}</blockquote><small>Status: {kindLabel(item.status)} · human context, not evidence</small></li>}
function ClaimCard({claim}){return <article className="claim-card" id={`claim-${claim.claim_id}`}>
 <div className="claim-heading"><span className={`claim-kind kind-${claim.kind.toLowerCase()}`}>{kindLabel(claim.kind)}</span><a href={`#claim-${claim.claim_id}`}>#{claim.ordinal}</a></div>
 <p className="claim-text">{claim.text}</p>
 {claim.qualification&&<p className="qualification"><strong>Qualification:</strong> {claim.qualification}</p>}
 {claim.assumptions.length>0&&<div className="claim-detail"><h4>Assumptions</h4><ul>{claim.assumptions.map(x=><Assumption key={x.assumption_id} item={x}/>)}</ul></div>}
 {claim.alternatives.length>0&&<div className="claim-detail"><h4>Competing interpretations</h4><ul>{claim.alternatives.map(x=><Alternative key={x.alternative_id} item={x}/>)}</ul><small>Alternatives are recorded here without an automatic ranking.</small></div>}
 {claim.evidence_gaps.length>0&&<div className="claim-detail"><h4>Evidence gaps</h4><ul>{claim.evidence_gaps.map(x=><Gap key={x.gap_id} item={x}/>)}</ul></div>}
 {claim.source_provenance?.length>0&&<div className="claim-detail"><h4>Source provenance</h4><ul>{claim.source_provenance.map(x=><SourceLink key={`${x.evidence_id}-${x.relation}`} item={x}/>)}</ul><small>A support/contradiction relation records how the fragment was used; it does not establish truth or independent corroboration.</small></div>}
 {claim.human_context?.length>0&&<div className="claim-detail"><h4>Human context used</h4><ul>{claim.human_context.map(x=><ContextLink key={`${x.contribution_id}-${x.relation}`} item={x}/>)}</ul></div>}
 </article>}

export function ReasoningPage(){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis); const view=useEpistemicView();
 if(!client)return <p>Connect first.</p>;
 if(!analysis)return <section><h2>Reasoning</h2><p>Select or run an analysis first.</p></section>;
 if(view.error)return <section><h2>Reasoning</h2><p className="error">Reasoning view unavailable: {view.error.message}</p></section>;
 const data=view.data;
 return <section><h2>Reasoning</h2>
  {!data&&<p>Loading recorded reasoning…</p>}
  {data&&<>
   <p>This view separates what the analysis said from assumptions, competing interpretations and missing evidence. Recording a claim here does not make it true.</p>
   {data.candidate_interpretations?.length>0&&<article className="standalone-reasoning"><h3>Competing interpretations considered</h3><ul>{data.candidate_interpretations.map(x=><CandidateInterpretation key={x.interpretation_id} item={x}/>)}</ul><small>These are model-generated candidates for consideration, not facts, claims, decisions, or an automatic ranking.</small></article>}
   {data.claims.length>0&&<nav className="claim-index" aria-label="Claims">{data.claims.map(c=><a key={c.claim_id} href={`#claim-${c.claim_id}`}>#{c.ordinal} {kindLabel(c.kind)}</a>)}</nav>}
   <div className="claim-list">{data.claims.map(c=><ClaimCard key={c.claim_id} claim={c}/>)}</div>
   {data.claims.length===0&&<p>No structured claims have been recorded for this analysis.</p>}
   {(data.unlinked_assumptions.length>0||data.unlinked_evidence_gaps.length>0)&&<article className="standalone-reasoning"><h3>Analysis-wide uncertainties</h3>
     {data.unlinked_assumptions.length>0&&<><h4>Assumptions not tied to one claim</h4><ul>{data.unlinked_assumptions.map(x=><Assumption key={x.assumption_id} item={x}/>)}</ul></>}
     {data.unlinked_evidence_gaps.length>0&&<><h4>Evidence gaps not tied to one claim</h4><ul>{data.unlinked_evidence_gaps.map(x=><Gap key={x.gap_id} item={x}/>)}</ul></>}
   </article>}
   <p className="epistemic-boundary"><small>Claims are not verified facts. Candidate interpretations are not facts and are not automatically ranked. Assumptions are not facts. Source-link counts are not counts of independent evidence. Human context is not evidence. Closing an evidence gap does not by itself prove a claim. The browser renders this backend-owned read model; it does not infer or rank the reasoning.</small></p>
  </>}
 </section>;
}
