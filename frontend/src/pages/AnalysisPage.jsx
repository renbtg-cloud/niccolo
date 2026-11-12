import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { analysisSchema } from '../config.js';
import { useUiStore } from '../stores/ui.js';
import { useAnalysisOutput } from '../hooks/useAnalysisOutput.js';
import { useOperationObserver } from '../hooks/useOperationObserver.js';
import { useAnalysisHistory } from '../hooks/useAnalysisHistory.js';
import { analysisHistoryLabel, historyGroups } from '../state/analysisHistoryPresentation.js';

function AnalysisAnswer({ output, pending, error }) {
  if (error) return <p className="error">Analysis output unavailable: {error.message}</p>;
  if (pending) return <p>Loading analysis output…</p>;
  if (!output) return <p>No synthesis has been published for this analysis yet.</p>;
  return <article className="analysis-answer">
    <div className="answer-heading">
      <h3>Niccolò's answer</h3>
      <span className={output.input_is_current ? 'status-current' : 'status-historical'}>{output.input_is_current ? 'Current input' : 'Historical input'}</span>
    </div>
    <div className="answer-text">{output.answer_text}</div>
    <h4>Coverage</h4>
    <div className="coverage-note">{output.coverage_note}</div>
    {!output.input_is_current && <p className="historical-note">The Situation changed after this analysis input was frozen. This output is preserved rather than silently rewritten.</p>}
  </article>;
}

export function AnalysisPage(){
 const analysis=useUiStore(s=>s.analysis); const activeOperation=useUiStore(s=>s.activeOperation); const setAnalysis=useUiStore(s=>s.setAnalysis);
 const {register,handleSubmit,reset}=useForm({resolver:zodResolver(analysisSchema),defaultValues:{analysis}});
 useEffect(()=>{reset({analysis});},[analysis,reset]);
 const observed=useOperationObserver(activeOperation);
 const output=useAnalysisOutput(analysis, observed.operation?.target_analysis_id===analysis ? observed.operation : null); const history=useAnalysisHistory(); const rows=historyGroups(history.data?.analyses??[]);
 return <section><h2>Analysis</h2>
   <form onSubmit={handleSubmit(v=>setAnalysis(v.analysis))}><input aria-label="Analysis UUID" placeholder="Analysis UUID" {...register('analysis')}/><button>Select analysis</button></form>
   <p>Selected: <code>{analysis||'none'}</code></p>
   <article className="analysis-history"><h3>Analysis history</h3>
     {history.isLoading&&<p>Loading analysis history…</p>}
     {history.error&&<p className="error">{history.error.message}</p>}
     {!history.isLoading&&rows.length===0&&<p>No analyses have been recorded yet.</p>}
     <ul>{rows.map(entry=><li key={entry.analysis_id} style={{marginLeft:`${entry.depth*18}px`}}>
       <button className={entry.analysis_id===analysis?'history-selected':''} onClick={()=>setAnalysis(entry.analysis_id)}>{entry.goal||'Analysis without explicit goal'}</button>
       <div><small>Revision {entry.input_revision} · {analysisHistoryLabel(entry)}{entry.imported_context_count?` · ${entry.imported_context_count} selected prior-context item(s)`:''}{entry.reanalysis_of?` · reanalysis of ${entry.reanalysis_of}`:''}</small></div>
     </li>)}</ul>
     <small>Lineage shown here is recorded lineage only; missing links are not inferred.</small>
   </article>
   {analysis ? <AnalysisAnswer output={output.data} pending={output.isPending} error={output.error}/> : <p>Select an analysis to inspect its published answer.</p>}
   <p><small>Claims, assumptions, alternatives, evidence gaps, decisions and human gates remain inspectable separately; selecting an analysis here does not change it.</small></p>
 </section>;
}
