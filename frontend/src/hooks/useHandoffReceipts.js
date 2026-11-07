import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
import { useUiStore } from '../stores/ui.js';

export function useHandoffReceipts(){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis);
 return useQuery({
   queryKey:client&&analysis?qk.analysis(client.config,analysis,'handoff-receipts'):['handoff-receipts','none'],
   enabled:!!client&&!!analysis,
   queryFn:async()=> (await client.handoffReceipts(analysis)).value,
 });
}
