import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
import { useUiStore } from '../stores/ui.js';

export function useReasoningInput(){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis);
 return useQuery({
   queryKey:client&&analysis?qk.analysis(client.config,analysis,'reasoning-input'):['reasoning-input','none'],
   enabled:!!client&&!!analysis,
   queryFn:async()=> (await client.reasoningInput(analysis)).value,
 });
}
