import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { connectionSchema } from '../config.js';
import { useSessionStore } from '../stores/session.js';
import { useUiStore } from '../stores/ui.js';
import { connectionHints, hasPreparedConnectionHints } from '../state/connectionHints.js';
import { preparedDemoUrl } from '../state/preparedDemoLink.js';
import { connectionFailureMessage, validateConnection } from '../state/connectionProbe.js';
export function ConnectPanel(){
 const queryClient=useQueryClient();
 const [checking,setChecking]=useState(false); const [connectionError,setConnectionError]=useState('');
 const hints=connectionHints(window.location.search); const prepared=hasPreparedConnectionHints(hints);
 const {register,handleSubmit,formState:{errors}}=useForm({resolver:zodResolver(connectionSchema),defaultValues:hints});
 const connect=useSessionStore(s=>s.connect); const disconnect=useSessionStore(s=>s.disconnect); const connected=useSessionStore(s=>s.connected); const workspace=useSessionStore(s=>s.workspace); const situation=useSessionStore(s=>s.situation); const clearUi=()=>{useUiStore.getState().setAnalysis('');useUiStore.getState().setActiveOperation('');};
 const clearSessionView=()=>{queryClient.clear();clearUi();};
 const connectClean=async(credentials)=>{
   setChecking(true);setConnectionError('');
   try{
     await validateConnection(credentials);
     clearSessionView();connect(credentials);
   }catch(error){
     setConnectionError(connectionFailureMessage(error));
   }finally{
     setChecking(false);
   }
 };
 const disconnectClean=()=>{disconnect();clearSessionView();}; const demoUrl=connected?preparedDemoUrl(window.location,{workspace,situation}):'';
 if(connected)return <div className="connection"><strong>Session connected</strong><button onClick={disconnectClean}>Disconnect</button><details className="demo-share"><summary>Share prepared demo link</summary><code>{demoUrl}</code><div><a href={demoUrl}>Open prepared link</a></div><small>This link contains workspace and Situation identifiers only. Send the bearer token separately.</small></details></div>;
 return <form className="connection" onSubmit={handleSubmit(connectClean)}>
   {prepared
    ? <div className="prepared-session"><strong>Prepared Situation</strong><small>Workspace and Situation are already selected by this demo link.</small><details><summary>Connection identifiers</summary><div className="row"><input aria-label="Workspace" placeholder="Workspace UUID" {...register('workspace')}/><input aria-label="Situation" placeholder="Situation UUID" {...register('situation')}/></div></details></div>
    : <><input aria-label="Workspace" placeholder="Workspace UUID" {...register('workspace')}/><input aria-label="Situation" placeholder="Situation UUID" {...register('situation')}/></>}
   <input aria-label="Bearer token" type="password" placeholder="Bearer token" {...register('token')}/>
   <button type="submit" disabled={checking}>{checking?'Checking…':'Connect'}</button>
   {connectionError&&<span className="error" role="alert">{connectionError}</span>}
   <small>{prepared?'Workspace and Situation are ready. Paste the bearer token from the separately supplied secret (for a local install: .niccolo-local/token.txt), then Connect.':'Workspace and Situation may be prefilled by a prepared demo link.'} Bearer tokens are never read from the URL or persisted.</small>
   {Object.keys(errors).length>0&&<span className="error">Check connection fields.</span>}
 </form>;
}
