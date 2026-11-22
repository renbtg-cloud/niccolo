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
     const root=(await client.analysisClarifications(analysis, null, 25)).value;
     const questions=root.clarifications??[];
     const results=await Promise.allSettled(questions.map(question =>
       client.clarificationAnswers(analysis,question.question_id)
     ));
     const clarifications=questions.map((question,index)=>{
       const result=results[index];
       return {
         ...question,
         answers: result.status==='fulfilled' ? result.value.value.answers??[] : [],
         answersUnavailable: result.status==='rejected',
         moreAnswers: result.status==='fulfilled' && Boolean(result.value.value.next_after),
       };
     });
     return {...root,clarifications};
   },
 });
}
