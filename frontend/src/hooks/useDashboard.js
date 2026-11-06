import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
import { useUiStore } from '../stores/ui.js';

export function useDashboard(){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis); const operation=useUiStore(s=>s.activeOperation);
 return useQuery({
   queryKey:client&&analysis?qk.dashboard(client.config,analysis,operation):['dashboard','none'],
   enabled:!!client&&!!analysis,
   queryFn:async()=> (await client.dashboard(analysis,operation)).value,
 });
}
