import test from "node:test";
import assert from "node:assert/strict";
import {
  BackendError, addPersistentContribution, backendConfig, createSituation,
  listPersistentRevisions, openSituation, retractPersistentContribution, setPersistentFocus,
} from "./api.js";
const W="11111111-1111-4111-8111-111111111111";
const S="22222222-2222-4222-8222-222222222222";
function fake(status=200,payload={ok:true}) {
  const calls=[];
  const fetch=async (path,options)=>{calls.push({path,options});return {ok:status>=200&&status<300,status,json:async()=>payload};};
  return {fetch,calls};
}
test("configuration validates UUIDs and never stores beyond returned object",()=>{
  assert.deepEqual(backendConfig(W,S," secret "),{workspace:W,situation:S,token:"secret"});
  assert.throws(()=>backendConfig("bad",S,"x"));
});
test("persistent Situation calls carry bearer and no-store",async()=>{
  const f=fake(200,{revision:2}); const c=backendConfig(W,S,"secret");
  await openSituation(f.fetch,c); await createSituation(f.fetch,c,"Focus"); await setPersistentFocus(f.fetch,c,"New focus");
  assert.equal(f.calls.length,3); assert.equal(f.calls[0].options.headers.Authorization,"Bearer secret"); assert.equal(f.calls[0].options.cache,"no-store");
  assert.equal(JSON.parse(f.calls[1].options.body).focus,"Focus");
});
test("human contribution keeps explicit hypothetical flag and stable id",async()=>{
  const f=fake(); const c=backendConfig(W,S,"secret"); const id="33333333-3333-4333-8333-333333333333";
  await addPersistentContribution(f.fetch,c,id,"What if?",true); await retractPersistentContribution(f.fetch,c,id);
  assert.equal(JSON.parse(f.calls[0].options.body).kind,"hypothetical"); assert.match(f.calls[1].path,/retract$/);
});
test("revision listing is bounded and backend errors retain HTTP status",async()=>{
  const c=backendConfig(W,S,"secret"); const ok=fake(); await listPersistentRevisions(ok.fetch,c); assert.match(ok.calls[0].path,/limit=100$/);
  const bad=fake(404,{error:"missing"}); await assert.rejects(()=>openSituation(bad.fetch,c),(e)=>e instanceof BackendError&&e.status===404);
});
