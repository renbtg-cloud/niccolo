import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
import { useUiStore } from '../stores/ui.js';

export function useGroundedScores(){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis);
 return useQuery({
   queryKey:client&&analysis?qk.groundedScores(client.config,analysis):['grounded-scores','none'],
   enabled:!!client&&!!analysis,
   queryFn:async()=> (await client.groundedScoreRecords(analysis,100)).value,
 });
}
