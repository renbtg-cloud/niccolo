import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { connectionSchema } from '../config.js';
import { useSessionStore } from '../stores/session.js';
import { useUiStore } from '../stores/ui.js';
import { connectionHints, hasPreparedConnectionHints } from '../state/connectionHints.js';
import { preparedDemoUrl } from '../state/preparedDemoLink.js';
import { connectionFailureMessage, validateConnection } from '../state/connectionProbe.js';
import { hasLocalHandoffFragment, redeemLocalHandoff, takeLocalHandoffFromFragment } from '../state/localHandoff.js';
export function ConnectPanel(){
 const queryClient=useQueryClient();
 const [checking,setChecking]=useState(()=>hasLocalHandoffFragment(window.location)); const [connectionError,setConnectionError]=useState('');
 const hints=connectionHints(window.location.search); const prepared=hasPreparedConnectionHints(hints);
 const {register,handleSubmit,getValues,formState:{errors}}=useForm({resolver:zodResolver(connectionSchema),defaultValues:hints});
 const handoffAttempted=useRef(false);
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
 useEffect(()=>{
   if(handoffAttempted.current||connected)return;
   handoffAttempted.current=true;
   const code=takeLocalHandoffFromFragment(window.location,window.history);
   if(!code){
     setChecking(false);
     return;
   }
   setChecking(true);setConnectionError('');
   void (async()=>{
     try{
       const token=await redeemLocalHandoff(code);
       const parsed=connectionSchema.safeParse({...getValues(),token});
       if(!parsed.success){
         setConnectionError('Automatic local connection could not use these prepared identifiers. Check the prepared link and connect manually.');
         setChecking(false);
         return;
       }
       await connectClean(parsed.data);
     }catch{
       setConnectionError('Automatic local connection expired or is unavailable. Paste the bearer token from .niccolo-local/token.txt and Connect.');
       setChecking(false);
     }
   })();
 },[]);
 const disconnectClean=()=>{disconnect();clearSessionView();}; const demoUrl=connected?preparedDemoUrl(window.location,{workspace,situation}):'';
 if(connected)return <div className="connection"><strong>Session connected</strong><button onClick={disconnectClean}>Disconnect</button><details className="demo-share"><summary>Share prepared demo link</summary><code>{demoUrl}</code><div><a href={demoUrl}>Open prepared link</a></div><small>This link contains workspace and Situation identifiers only. Send the bearer token separately.</small></details></div>;
 return <form className="connection" onSubmit={handleSubmit(connectClean)}>
   {prepared
    ? <div className="prepared-session"><strong>Prepared Situation</strong><small>Workspace and Situation are already selected by this demo link.</small><details><summary>Connection identifiers</summary><div className="row"><input aria-label="Workspace" placeholder="Workspace UUID" {...register('workspace')}/><input aria-label="Situation" placeholder="Situation UUID" {...register('situation')}/></div></details></div>
    : <><input aria-label="Workspace" placeholder="Workspace UUID" {...register('workspace')}/><input aria-label="Situation" placeholder="Situation UUID" {...register('situation')}/></>}
   <input aria-label="Bearer token" type="password" placeholder="Bearer token" disabled={checking} {...register('token')}/>
   <button type="submit" disabled={checking}>{checking?'Connecting…':'Connect'}</button>
   {connectionError&&<span className="error" role="alert">{connectionError}</span>}
   <small>{prepared?'Workspace and Situation are ready. A local one-shot handoff connects automatically when present; otherwise paste the bearer token from .niccolo-local/token.txt and Connect.':'Workspace and Situation may be prefilled by a prepared demo link.'} Bearer tokens are never placed in the URL or persisted.</small>
   {Object.keys(errors).length>0&&<span className="error">Check connection fields.</span>}
 </form>;
}
