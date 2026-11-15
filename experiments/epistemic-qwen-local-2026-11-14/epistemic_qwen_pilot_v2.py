#!/usr/bin/env python3
"""Qwen epistemic micro-pilot, revised instructions and counterbalanced options.

Uses same synthetic cases and mathematical oracle as the V1 pilot.
Separates system/user roles and rotates answer-label meanings by case to expose
first-letter bias. This test is exploratory and not a blind benchmark.
"""
import argparse
import json
import sys
import time
from pathlib import Path
BASE = Path(__file__).parent
sys.path.insert(0,str(BASE))
from epistemic_qwen_pilot import CASES,solve
sys.path.insert(0,str(BASE/'spititout-viper-v0.6'/'runtime'))
from gguf_qwen3_experimental import GGUF,Store,Tokenizer,Runner
from eval_choice_model import enc_chat

SYMBOLS = ('YES','NO','UNKNOWN')
BASE_RULES = (
  'You are a strict epistemic logician. Judge only what is explicitly stated or entailed. '
  'K(agent,p) means the agent KNOWS p, so p is TRUE. B(agent,p) means the agent BELIEVES p, '
  'but the belief may be false. NOT(p) explicitly negates p. '
  'Not mentioned does NOT mean false. Nothing persists across time unless stated. '
  'A knowledge claim nested inside another knowledge claim is also true, because knowledge is factive. '
  'When neither p nor NOT(p) follows, the answer must be UNKNOWN. '
)

def format_prompt(case,form,index):
    # By experiment design, option semantics rotate; do not treat A as always yes.
    assignment={ch:SYMBOLS[(j+index)%3] for j,ch in enumerate('ABC')}
    answer_map={meaning:ch for ch,meaning in assignment.items()}
    gold=answer_map[{'A':'YES','B':'NO','C':'UNKNOWN'}[solve(case['facts'],case['query'])]]
    label_desc=' '.join(f'{k}={assignment[k]}.' for k in 'ABC')
    system=BASE_RULES + '\n' + label_desc + '\nOnly output the single correct LETTER.'
    user=case[form]+'\nChoose the logically correct answer letter.'
    prompt=f'<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n'
    return prompt, gold, assignment

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--limit',type=int,default=10)
    ap.add_argument('--model',default='/mnt/data/Qwen3-1.7B-Q4_K_M.gguf')
    ap.add_argument('--cache',default='/mnt/data/qwen_model_runtime/converted')
    ap.add_argument('--out',default='/mnt/data/qwen_model_runtime/epistemic-pilot-v2-results.json')
    args=ap.parse_args()
    g=GGUF(args.model);t=Tokenizer(g.meta);r=Runner(g,Store(g,args.cache))
    labels='ABC';choice_ids=[t.encode(x)[0] for x in labels];rows=[]
    print('MODEL',g.meta.get('general.name'),'CHOICE_TOKENS',choice_ids,flush=True)
    for ix,c in enumerate(CASES[:args.limit]):
      for form in ('prose','formal'):
        prompt,gold,assignment=format_prompt(c,form,ix)
        ids=enc_chat(t,prompt);r.cache=[];start=time.monotonic()
        _,logits=r.forward(ids,0,choices=choice_ids)
        predicted=labels[max(range(3),key=lambda i:logits[i])]
        row={'case':c['id'],'form':form,'assignment':assignment,'gold':gold,'predicted':predicted,
             'correct':gold==predicted,'token_count':len(ids),'duration_seconds':round(time.monotonic()-start,2),
             'logits':dict(zip(labels,[round(x,3) for x in logits]))}
        rows.append(row)
        result={'experiment':'epistemic-first-token-v2','model_sha256':'d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5',
                'method':'system-role instructions; per-case counterbalanced A/B/C label semantics; experimental local GGUF runner',
                'oracle':'knowledge closure factivity + explicit negation; no temporal persistence, no default closed world',
                'results':rows,
                'by_form':{f:{'passed':sum(x['correct'] for x in rows if x['form']==f),'total':sum(x['form']==f for x in rows)} for f in ('prose','formal')}}
        Path(args.out).write_text(json.dumps(result,indent=2))
        print('RESULT',json.dumps(row),flush=True)
    print('FINAL',json.dumps(result['by_form']),flush=True)

if __name__=='__main__':main()
