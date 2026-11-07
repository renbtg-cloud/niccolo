import { describe, expect, it } from 'vitest';
import { agenticPlanPollInterval } from './useAgenticPlan.js';

const q=(data)=>({state:{data}});
describe('agentic plan polling',()=>{
  it('polls while the controller may still make progress',()=>expect(agenticPlanPollInterval(q({complete:false,blocked:false,waiting_for_human:false}))).toBe(1000));
  it('stops at terminal or explicit no-progress boundaries',()=>{
    expect(agenticPlanPollInterval(q({complete:true}))).toBe(false);
    expect(agenticPlanPollInterval(q({blocked:true}))).toBe(false);
    expect(agenticPlanPollInterval(q({waiting_for_human:true}))).toBe(false);
  });
});
