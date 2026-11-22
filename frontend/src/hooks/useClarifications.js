import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
import { useUiStore } from '../stores/ui.js';

export function useClarifications(after = null){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis);
 return useQuery({
   queryKey:client&&analysis?[...qk.analysis(client.config,analysis,'clarifications'),after]:['clarifications','none'],
   enabled:!!client&&!!analysis,
   retry:false,
   queryFn:async ({ signal })=> (await client.analysisClarifications(analysis, after, 25, signal)).value,
 });
}

export function useClarificationAnswers(questionId, after = null) {
 const client=useClient();
 const analysis=useUiStore(s=>s.analysis);
 return useQuery({
   queryKey:client&&analysis?[...qk.analysis(client.config,analysis,'clarification-answers'),questionId,after]:['clarification-answers','none'],
   enabled:Boolean(client&&analysis&&questionId),
   retry:false,
   queryFn:async ({ signal })=> (await client.clarificationAnswers(analysis, questionId, after, 25, signal)).value,
 });
}
