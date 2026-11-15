#!/usr/bin/env python3
"""Independent, reproducible local-Qwen epistemic micro-pilot.

Research only. This is a small, developer-visible feasibility experiment, NOT
an accuracy benchmark or evidence of improvement over a frontier model.

The truth oracle is a minimal factive knowledge closure, intentionally without
closed-world negation or temporal persistence. All cases are synthetic.
"""
import argparse
import hashlib
import json
import os
import platform
import re
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'runtime'))
from gguf_qwen3_experimental import GGUF, Store, Tokenizer, Runner
from eval_choice_model import enc_chat

P = ('P', 'audit_active', 't0')
P1 = ('P', 'audit_active', 't1')
def K(a, p, t='t0'): return ('K', a, p, t)
def B(a, p, t='t0'): return ('B', a, p, t)
def N(p): return ('NOT', p)

def solve(facts, query):
    known = set(facts)
    # Knowledge is factive. Beliefs are not. No closure for unprovided facts.
    while True:
        found = {f[2] for f in known if f[0] == 'K'}
        if found <= known: break
        known |= found
    if query in known and N(query) in known: return 'INCONSISTENT'
    if query in known: return 'A'
    if N(query) in known: return 'B'
    return 'C'

CASES = [
    dict(id='E01', facts=[K('Cheng',P)], query=K('Cheng',P),
         prose='At t0, Cheng knows that the audit is active. At t0, does Cheng know that the audit is active?',
         formal='Facts: K(Cheng, audit_active@t0). Query: K(Cheng, audit_active@t0)?'),
    dict(id='E02', facts=[B('Cheng',P)], query=P,
         prose='At t0, Cheng believes the audit is active, but no information about its actual status is given. Is the audit active at t0?',
         formal='Facts: B(Cheng, audit_active@t0). Query: audit_active@t0?'),
    dict(id='E03', facts=[K('Maria',K('Cheng',P))], query=K('Cheng',P),
         prose='At t0, Maria knows that Cheng knows the audit is active. Does Cheng know the audit is active at t0?',
         formal='Facts: K(Maria, K(Cheng, audit_active@t0)). Query: K(Cheng, audit_active@t0)?'),
    dict(id='E04', facts=[B('Maria',K('Cheng',P))], query=K('Cheng',P),
         prose='At t0, Maria believes that Cheng knows the audit is active, but nothing else about Cheng is stated. Does Cheng know the audit is active at t0?',
         formal='Facts: B(Maria, K(Cheng, audit_active@t0)). Query: K(Cheng, audit_active@t0)?'),
    dict(id='E05', facts=[N(P),B('Cheng',P)], query=P,
         prose='At t0, the audit is NOT active. Cheng nevertheless believes it is active. Is the audit active at t0?',
         formal='Facts: NOT(audit_active@t0); B(Cheng, audit_active@t0). Query: audit_active@t0?'),
    dict(id='E06', facts=[K('Maria',B('Cheng',P))], query=B('Cheng',P),
         prose='At t0, Maria knows that Cheng believes the audit is active. Is it true that Cheng believes the audit is active at t0?',
         formal='Facts: K(Maria, B(Cheng, audit_active@t0)). Query: B(Cheng, audit_active@t0)?'),
    dict(id='E07', facts=[K('Maria',B('Cheng',P))], query=K('Cheng',P),
         prose='At t0, Maria knows that Cheng BELIEVES the audit is active. No claim is made that Cheng knows it. Does Cheng KNOW the audit is active at t0?',
         formal='Facts: K(Maria, B(Cheng, audit_active@t0)). Query: K(Cheng, audit_active@t0)?'),
    dict(id='E08', facts=[N(K('Cheng',P)),B('Maria',K('Cheng',P))], query=K('Cheng',P),
         prose='At t0, Cheng does NOT know that the audit is active. Maria mistakenly believes that Cheng knows. Does Cheng know at t0 that the audit is active?',
         formal='Facts: NOT(K(Cheng, audit_active@t0)); B(Maria, K(Cheng, audit_active@t0)). Query: K(Cheng, audit_active@t0)?'),
    dict(id='E09', facts=[K('Cheng',P)], query=K('Cheng',P1,'t1'),
         prose='At t0, Cheng knows the audit is active. No facts about t1 are provided. Does Cheng know the audit is active at t1?',
         formal='Facts: K(Cheng, audit_active@t0). No time-persistence rule. Query: K(Cheng, audit_active@t1)?'),
    dict(id='E10', facts=[K('Maria',K('Cheng',P1,'t1'),'t1')], query=K('Maria',K('Cheng',P),'t0'),
         prose='At t1, Maria knows that Cheng knows the audit is active at t1. No earlier facts are supplied. At t0, did Maria know that Cheng knew it at t0?',
         formal='Facts: K(Maria, K(Cheng, audit_active@t1), t1). No retroactive-knowledge rule. Query: K(Maria, K(Cheng, audit_active@t0), t0)?'),
]

SYSTEM = ("Decide only what logically follows from the stated facts. "
          "Knowledge is factive: if an agent KNOWS proposition p, then p is true. "
          "A BELIEF may be true or false. Do not assume unstated facts, and do not "
          "assume facts persist between times unless told so. "
          "The notation K(agent, p) means 'agent knows p'; B(agent, p) means "
          "'agent believes p'; NOT(p) is explicit negation. "
          "For each query, select exactly one letter: "
          "A = YES, entailed; B = NO, explicitly contradicted; "
          "C = UNKNOWN, neither entailed nor contradicted. "
          "Answer ONLY A, B, or C, with no explanation.")

def prompt(form, content):
    text = (f"{SYSTEM}\nCase description:\n{content}\nReturn A, B, or C.")
    chat = f'<|im_start|>user\n{text}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n'
    return chat

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--model', default='/mnt/data/Qwen3-1.7B-Q4_K_M.gguf')
    ap.add_argument('--cache', default='/mnt/data/qwen_model_runtime/converted')
    ap.add_argument('--out', default='/mnt/data/qwen_model_runtime/epistemic-pilot-results.json')
    ap.add_argument('--limit', type=int, default=10)
    ap.add_argument('--forms', default='prose,formal')
    args = ap.parse_args()
    gguf = GGUF(args.model)
    tok = Tokenizer(gguf.meta)
    runner = Runner(gguf, Store(gguf,args.cache))
    choice_ids = [tok.encode(x)[0] for x in ('A','B','C')]
    labels=['A','B','C']
    assert len(set(choice_ids))==3
    cases=CASES[:args.limit]
    forms=args.forms.split(',')
    print('MODEL',gguf.meta['general.name'],'CASES',len(cases),'FORMS',forms,'CHOICE_TOKENS',choice_ids,flush=True)
    records=[]
    for c in cases:
        truth=solve(c['facts'], c['query'])
        if truth=='INCONSISTENT': raise ValueError(c['id']+' inconsistent')
        for form in forms:
            runner.cache=[]
            text=prompt(form,c[form])
            ids=enc_chat(tok,text)
            started=time.monotonic()
            _,logits=runner.forward(ids,0,choices=choice_ids)
            predicted=labels[max(range(3),key=lambda j: logits[j])]
            row={'id':c['id'],'form':form,'gold':truth,'prediction':predicted,
                 'correct':predicted==truth,'token_count':len(ids),
                 'seconds':round(time.monotonic()-started,3),
                 'logits':dict(zip(labels,[round(x,4) for x in logits])),
                 'scenario':c[form]}
            records.append(row)
            print('RESULT',json.dumps(row,ensure_ascii=False),flush=True)
            Path(args.out).write_text(json.dumps({'description':'Experimental local Qwen3 1.7B epistemic formalization micro-pilot; no hidden test set, no empirical quality claim',
                      'model':gguf.meta['general.name'],
                      'model_sha256':'d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5',
                      'oracle':'Factive knowledge closure; explicit negation; no closed-world or temporal persistence',
                      'methods':'Uncalibrated first-token letter logits; greedy choice; non-thinking assistant prefix; research-only non-reference GGUF runtime',
                      'cases':records,
                      'scores':{f:{'pass':sum(r['correct'] for r in records if r['form']==f), 'n':sum(r['form']==f for r in records)} for f in forms}}
                  ,indent=2,ensure_ascii=False))
    print('FINAL',json.dumps({'score':{f:[sum(r['correct'] for r in records if r['form']==f),sum(r['form']==f for r in records)] for f in forms}},ensure_ascii=False),flush=True)

if __name__=='__main__':main()
