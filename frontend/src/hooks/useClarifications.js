import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
import { useUiStore } from '../stores/ui.js';

export function useClarifications(){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis);
 return useQuery({
   queryKey:client&&analysis?qk.analysis(client.config,analysis,'clarifications'):['clarifications','none'],
   enabled:!!client&&!!analysis,
   queryFn:async()=>{
     const root=(await client.analysisClarifications(analysis)).value;
     const clarifications=await Promise.all((root.clarifications??[]).map(async question=>{
       const answers=(await client.clarificationAnswers(analysis,question.question_id)).value;
       return {...question,answers:answers.answers??[]};
     }));
     return {...root,clarifications};
   },
 });
}
