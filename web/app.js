import { initialState, reduce } from "/state.js";
import {
  BackendError, addPersistentContribution, backendConfig, createSituation,
  listPersistentRevisions, openSituation, retractPersistentContribution, setPersistentFocus,
} from "/api.js";

let state = initialState();
let backend = null;
let persisted = null;
let persistedHistory = [];
const $ = (id) => document.getElementById(id);

function currentPremises() {
  if (!persisted) return state.premises;
  return persisted.contributions.map((c) => ({ id:c.contribution_id, text:c.text, kind:c.kind === "HYPOTHETICAL" ? "hypothetical" : "user-reported", active:true }));
}
function render() {
  const labels = {act:"I am about to say or do something",understand:"I am trying to understand something",discover:"Help me figure out what matters"};
  $("selected-intent").textContent = state.intention ? `Current intention: ${labels[state.intention]}` : "No intention selected yet.";
  $("revision-count").textContent = `revision ${persisted?.revision ?? state.revision}`;
  if (persisted) $("focus").value = persisted.focus;
  $("source-list").replaceChildren(...state.sources.map((name)=>{const li=document.createElement("li");li.textContent=`${name} — selected locally, not uploaded or analyzed`;return li;}));
  $("premise-list").replaceChildren(...currentPremises().map((premise)=>{const li=document.createElement("li");const text=document.createElement("span");text.textContent=`${premise.kind === "hypothetical" ? "WHAT IF" : "USER-REPORTED"}: ${premise.text}`;li.append(text);if(premise.active){const button=document.createElement("button");button.type="button";button.className="text-button";button.textContent="Retract";button.addEventListener("click",async()=>{if(backend){try{persisted=await retractPersistentContribution(fetch,backend,premise.id);await refreshHistory();status(`Persistent revision ${persisted.revision}.`);}catch(e){status(errorText(e),true);}}else{state=reduce(state,{type:"retract-premise",id:premise.id});}render();});li.append(button);}return li;}));
  const history=persisted ? persistedHistory.map((x)=>({revision:x.revision,reason:x.cause})) : state.history;
  $("revision-list").replaceChildren(...history.map((entry)=>{const li=document.createElement("li");li.textContent=`r${entry.revision}: ${entry.reason}`;return li;}));
}
function status(text,error=false){$("backend-status").textContent=text;$("backend-status").classList.toggle("error",error);}
function errorText(e){return e instanceof BackendError ? `Backend HTTP ${e.status}: ${e.message}` : e.message;}
async function refreshHistory(){if(!backend)return;const value=await listPersistentRevisions(fetch,backend);persistedHistory=value.revisions ?? [];}

$("connect-backend").addEventListener("click",async()=>{try{backend=backendConfig($("workspace-id").value,$("situation-id").value,$("bearer-token").value);try{persisted=await openSituation(fetch,backend);}catch(e){if(!(e instanceof BackendError)||e.status!==404)throw e;const focus=$("focus").value.trim();if(!focus)throw new Error("Set a focus before creating a new persistent Situation.");persisted=await createSituation(fetch,backend,focus);}await refreshHistory();status(`Connected. Persistent revision ${persisted.revision}.`);render();}catch(e){backend=null;persisted=null;persistedHistory=[];status(errorText(e),true);render();}});
$("intentions").addEventListener("click",(event)=>{const button=event.target.closest("button[data-intent]");if(!button)return;state=reduce(state,{type:"select-intention",value:button.dataset.intent});render();});
$("sources").addEventListener("change",(event)=>{for(const file of event.target.files??[])state=reduce(state,{type:"add-source",name:file.name});render();});
$("set-focus").addEventListener("click",async()=>{const value=$("focus").value;if(backend){try{persisted=await setPersistentFocus(fetch,backend,value);await refreshHistory();status(`Persistent revision ${persisted.revision}.`);}catch(e){status(errorText(e),true);}}else state=reduce(state,{type:"set-focus",value});render();});
$("add-premise").addEventListener("click",async()=>{const field=$("premise");const text=field.value;const hypothetical=$("what-if").checked;if(backend){try{persisted=await addPersistentContribution(fetch,backend,crypto.randomUUID(),text,hypothetical);await refreshHistory();status(`Persistent revision ${persisted.revision}.`);field.value="";$("what-if").checked=false;}catch(e){status(errorText(e),true);}}else{state=reduce(state,{type:"add-premise",text,hypothetical});field.value="";$("what-if").checked=false;}render();});
render();
