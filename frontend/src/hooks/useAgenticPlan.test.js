import { describe, expect, it } from 'vitest';
import { agenticPlanPollInterval } from './useAgenticPlan.js';

const q=(data)=>({state:{data}});
describe('agentic plan polling',()=>{
  it('polls while the controller may still make progress',()=>expect(agenticPlanPollInterval(q({complete:false,blocked:false,waiting_for_human:false}),'RUNNING')).toBe(1000));
  it('keeps polling across a transient complete controller revision while the operation is still live',()=>{
    expect(agenticPlanPollInterval(q({complete:true,blocked:false,waiting_for_human:false}),'RUNNING')).toBe(1000);
    expect(agenticPlanPollInterval(q({complete:true,blocked:false,waiting_for_human:false}),'WAITING_FOR_HUMAN')).toBe(1000);
  });
  it('stops only at stable controller or durable-operation boundaries',()=>{
    expect(agenticPlanPollInterval(q({blocked:true}),'RUNNING')).toBe(false);
    expect(agenticPlanPollInterval(q({waiting_for_human:true}),'WAITING_FOR_HUMAN')).toBe(false);
    for(const state of ['SUCCEEDED','FAILED','CANCELLED']){
      expect(agenticPlanPollInterval(q({complete:true,blocked:false,waiting_for_human:false}),state)).toBe(false);
    }
  });
});
