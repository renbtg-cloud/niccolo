import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { analysisSchema } from '../config.js';
import { useUiStore } from '../stores/ui.js';
import { useAnalysisOutput } from '../hooks/useAnalysisOutput.js';

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
 const analysis=useUiStore(s=>s.analysis); const setAnalysis=useUiStore(s=>s.setAnalysis);
 const {register,handleSubmit}=useForm({resolver:zodResolver(analysisSchema),defaultValues:{analysis}});
 const output=useAnalysisOutput(analysis);
 return <section><h2>Analysis</h2>
   <form onSubmit={handleSubmit(v=>setAnalysis(v.analysis))}><input aria-label="Analysis UUID" placeholder="Analysis UUID" {...register('analysis')}/><button>Select analysis</button></form>
   <p>Selected: <code>{analysis||'none'}</code></p>
   {analysis ? <AnalysisAnswer output={output.data} pending={output.isPending} error={output.error}/> : <p>Select an analysis to inspect its published answer.</p>}
   <p><small>Claims, assumptions, alternatives, evidence gaps, decisions and human gates remain inspectable separately; selecting an analysis here does not change it.</small></p>
 </section>;
}
