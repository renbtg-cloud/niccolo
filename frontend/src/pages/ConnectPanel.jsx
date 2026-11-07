import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { connectionSchema } from '../config.js';
import { useSessionStore } from '../stores/session.js';
import { useUiStore } from '../stores/ui.js';
import { connectionHints } from '../state/connectionHints.js';
export function ConnectPanel(){
 const {register,handleSubmit,formState:{errors}}=useForm({resolver:zodResolver(connectionSchema),defaultValues:connectionHints(window.location.search)});
 const connect=useSessionStore(s=>s.connect); const disconnect=useSessionStore(s=>s.disconnect); const connected=useSessionStore(s=>s.connected); const clearUi=()=>{useUiStore.getState().setAnalysis('');useUiStore.getState().setActiveOperation('');}; const disconnectClean=()=>{disconnect();clearUi();};
 if(connected)return <div className="connection"><strong>Backend connected</strong><button onClick={disconnectClean}>Disconnect</button></div>;
 return <form className="connection" onSubmit={handleSubmit(connect)}><input aria-label="Workspace" placeholder="Workspace UUID" {...register('workspace')}/><input aria-label="Situation" placeholder="Situation UUID" {...register('situation')}/><input aria-label="Bearer token" type="password" placeholder="Bearer token" {...register('token')}/><button type="submit">Connect</button><small>Workspace and Situation may be prefilled by a prepared demo link. Bearer tokens are never read from the URL or persisted.</small>{Object.keys(errors).length>0&&<span className="error">Check connection fields.</span>}</form>;
}
