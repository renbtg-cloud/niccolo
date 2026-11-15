#!/usr/bin/env python3
"""Niccolò experiment 001: five-actor temporal epistemic micro-world.

Synthetic fixtures. Factive-K closure, explicit negation, non-factive B and R,
no closed-world inference, no time persistence. Not a real-world psychological model.

Treatments: direct-response Qwen, test-challenge Qwen, and a deliberately
narrow Qwen outer-operator extraction + deterministic oracle composition.
The Qwen runtime is experimental and is not numerically verified against ggml.
"""
import argparse
import hashlib
import json
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
RUNTIME_CANDIDATES = [
    HERE / 'runtime',
    HERE.parent / 'epistemic-qwen-local-2026-11-14' / 'runtime',
    HERE.parent / 'niccolo-qwen-epistemic-lab' / 'runtime',
]
RUNTIME = next((x for x in RUNTIME_CANDIDATES if (x / 'gguf_qwen3_experimental.py').exists()), None)
if RUNTIME is None:
    raise RuntimeError('Missing experimental Qwen runtime; see prior epistemic Qwen pilot')
sys.path.insert(0, str(RUNTIME))
from gguf_qwen3_experimental import GGUF, Store, Tokenizer, Runner
from eval_choice_model import enc_chat

ACTORS = ('Cheng', 'Mara', 'Nia', 'Omar', 'Priya')
P = ('ATOM', 'audit_was_launched')
Q = ('ATOM', 'evidence_folder_was_destroyed')
def NOT(p): return ('NOT', p)
def K(a,p,t): return ('K', a, p, t)
def B(a,p,t): return ('B', a, p, t)
def R(a,b,p,t): return ('R', a, b, p, t)

def explain(expr):
    kind=expr[0]
    if kind=='ATOM': return {'audit_was_launched':'the audit was launched', 'evidence_folder_was_destroyed':'the evidence folder was destroyed'}[expr[1]]
    if kind=='NOT': return 'it is not the case that '+explain(expr[1])
    if kind=='K': return f'{expr[1]} knows at {expr[3]} that {explain(expr[2])}'
    if kind=='B': return f'{expr[1]} believes at {expr[3]} that {explain(expr[2])}'
    if kind=='R': return f'{expr[1]} told {expr[2]} at {expr[4]} that {explain(expr[3])}'
    raise ValueError(expr)

def deduct(facts, query):
    known=set(facts)
    rounds=[]
    for _ in range(16):
        newly=[(f[2],f) for f in sorted(known, key=repr) if f[0]=='K' and f[2] not in known]
        if not newly: break
        for implied,parent in newly:
            rounds.append({'factive_premise':explain(parent),'entailed':explain(implied)})
            known.add(implied)
    yes=query in known
    # A query NOT(p) is refuted by p, not by NOT(NOT(p)).
    counter=query[1] if query[0]=='NOT' else NOT(query)
    no=counter in known
    if yes and no: return 'INCONSISTENT',rounds
    verdict='SUPPORTED' if yes else 'REFUTED' if no else 'UNRESOLVED'
    return verdict,rounds

# 15 new case IDs. Five named actors collectively; cases are kept independent.
CASES = [
 ('C01',[K('Mara',P,'t1')],P),
 ('C02',[K('Mara',P,'t1'),B('Cheng',NOT(P),'t2')],NOT(P)),
 ('C03',[R('Mara','Nia',P,'t1')],K('Nia',P,'t2')),
 ('C04',[B('Nia',K('Cheng',P,'t2'),'t2')],K('Cheng',P,'t2')),
 ('C05',[K('Omar',B('Nia',K('Cheng',P,'t2'),'t2'),'t3')],B('Nia',K('Cheng',P,'t2'),'t2')),
 ('C06',[K('Priya',NOT(Q),'t4')],Q),
 ('C07',[K('Priya',NOT(Q),'t4')],NOT(Q)),
 ('C08',[B('Cheng',NOT(P),'t5')],P),
 ('C09',[NOT(K('Cheng',P,'t9')),K('Cheng',P,'t10')],K('Cheng',P,'t9')),
 ('C10',[NOT(K('Cheng',P,'t9')),K('Cheng',P,'t10')],K('Cheng',P,'t10')),
 ('C11',[K('Priya',R('Nia','Omar',NOT(P),'t7'),'t8')],NOT(P)),
 ('C12',[K('Priya',R('Nia','Omar',NOT(P),'t7'),'t8')],R('Nia','Omar',NOT(P),'t7')),
 ('C13',[K('Mara',B('Cheng',NOT(P),'t5'),'t6')],K('Cheng',NOT(P),'t5')),
 ('C14',[B('Omar',NOT(Q),'t3'),K('Priya',NOT(Q),'t4')],Q),
 ('C15',[NOT(K('Cheng',P,'t9'))],K('Cheng',P,'t9')),
]

# Epistemic interpretation must respect the exact operators, not English patterns.
# The 12 one-utterance parser cases are a DIFFERENT, narrower task, not directly
# comparable to all the questions above. The verifier is given gold inner content.
PARSER_CASES = [
 ('P01','Mara knows that the audit was launched.','KNOWLEDGE'),
 ('P02','Cheng believes that the audit was launched.','BELIEF'),
 ('P03','Nia says that the audit was launched.','COMMUNICATION'),
 ('P04','Omar learned that the evidence folder was destroyed.','KNOWLEDGE'),
 ('P05','Priya suspects that the evidence folder was destroyed.','BELIEF'),
 ('P06','Mara reports that the evidence folder was destroyed.','COMMUNICATION'),
 ('P07','Cheng is convinced that the audit was launched.','BELIEF'),
 ('P08','Nia knows the audit was launched.','KNOWLEDGE'),
 ('P09','Omar told Priya that the audit was launched.','COMMUNICATION'),
 ('P10','Priya understands that the audit was launched.','KNOWLEDGE'),
 ('P11','Mara claims that the audit was launched.','COMMUNICATION'),
 ('P12','Cheng suspects the audit was launched.','BELIEF'),
]

OUTCOMES=('SUPPORTED','REFUTED','UNRESOLVED')
TYPES=('KNOWLEDGE','BELIEF','COMMUNICATION')

DIRECT_SYSTEM=(
 'You are given independent, stipulated model facts. Decide whether the query '
 'is entailed by those facts, refuted by explicit negation, or unresolved. '
 'Knowledge is factive: if an agent knows a proposition, the proposition is true. '
 'Beliefs can be false. Reporting a proposition does not establish that it is true '
 'or that the recipient learned it. Do not infer unstated facts, including '
 'knowledge at another time. No closed-world negation. '
 'Judge the question using only the stated facts. '
)

CHALLENGE_SYSTEM=(
 'Solve a small, formal reasoning challenge with independently specified test cases. '
 'Do not substitute psychology, common sense, or world knowledge for the stipulated facts. '
 'Semantics: K(a,p,t) entails p (factivity), including when p is another mental-state '
 'assertion. B(a,p,t) is non-factive; R(a,b,p,t) records an utterance, not the truth '
 'of p or the recipient\'s knowledge. Explicit NOT(q) refutes q. '
 'No closed-world negation; no unstated temporal persistence. '
 'For this experiment, output only the result-code letter for the current test; '
 'the independent harness records input, expected and observed results, and explanatory derivations. '
 'Illustrative verified unit tests (not the target case): '
 'TEST T_A: Ada believes the gate is open; query "the gate is open" => UNRESOLVED. '
 'TEST T_B: Leo knows the gate is closed; query "the gate is closed" => SUPPORTED. '
 'TEST T_C: explicit NOT(package_sent); query "package_sent" => REFUTED. '
)

PARSER_SYSTEM=(
 'Identify the MAIN OUTERMOST operator in the sentence. '
 'KNOWLEDGE means knows, learned, or understands; '
 'BELIEF means believes, suspects, or is convinced; '
 'COMMUNICATION means says, reports, claims, or told. '
 'Do not treat the asserted content as true unless operator is KNOWLEDGE. '
 'Return ONLY the answer letter from the mapping.'
)

def chat(sys_text,user_text):
    return (f'<|im_start|>system\n{sys_text}<|im_end|>\n'
            f'<|im_start|>user\n{user_text}<|im_end|>\n'
            f'<|im_start|>assistant\n<think>\n\n</think>\n')

def item_prompt(item,treatment,index):
    case_id,facts,query=item
    mapping={c:OUTCOMES[(i+index)%3] for i,c in enumerate('XYZ')}
    choices='\n'.join(f'{c} = {v}' for c,v in mapping.items())
    givens='\n'.join(f'E{j+1}: {explain(f)}.' for j,f in enumerate(facts))
    question=f'QUERY: {explain(query)}.'
    user=f'Case {case_id}; actors: {", ".join(ACTORS)}.\nSTIPULATED FACTS:\n{givens}\n{question}\n{choices}\nChoose one answer letter: X, Y, or Z.'
    if treatment=='direct': return chat(DIRECT_SYSTEM,user),mapping
    if treatment=='challenge':
        user=f'CHALLENGE {case_id}; TEST CASE 1/{1}\n{user}\nOutput only the result-code letter for this test.'
        return chat(CHALLENGE_SYSTEM,user),mapping
    raise ValueError(treatment)

def choice(runner,tok,prompt):
    tokens=enc_chat(tok,prompt)
    opts='XYZ'; tokids=[tok.encode(x)[0] for x in opts]
    assert len(set(tokids))==3 and all(len(tok.encode(x))==1 for x in opts)
    runner.cache=[]
    st=time.monotonic()
    _,logits=runner.forward(tokens,0,choices=tokids)
    pred=opts[max(range(3),key=lambda i:logits[i])]
    return pred,dict(zip(opts,[round(float(x),5) for x in logits])),round(time.monotonic()-st,3),len(tokens)

def run_case(item,treatment,i,runner,tok):
    case_id,facts,query=item
    expected,trace=deduct(facts,query)
    if expected=='INCONSISTENT': raise AssertionError(case_id+' inconsistent truth oracle')
    prompt,mapping=item_prompt(item,treatment,i)
    selected,logits,seconds,ntok=choice(runner,tok,prompt)
    predicted=mapping[selected]
    return {'case_id':case_id,'treatment':treatment,'facts':[explain(f) for f in facts],
      'query':explain(query),'expected':expected,'mapping':mapping,'chosen_letter':selected,
      'observed':predicted,'match':predicted==expected,'verified_oracle_trace':trace,
      'oracle_explanation':'Factive knowledge closure; only explicit negation refutes; absence of entailment is unresolved.',
      'logits_uncalibrated':logits,'seconds':seconds,'prompt_tokens':ntok,
      'prompt':prompt}

def run_parser_case(case,i,runner,tok):
    case_id,sentence,gold=case
    mapping={c:TYPES[(i+j)%3] for j,c in enumerate('XYZ')}
    texts='\n'.join(f'{c} = {meaning}' for c,meaning in mapping.items())
    prompt=chat(PARSER_SYSTEM,f'Case {case_id}. {sentence}\n{texts}\nWhich letter matches the outermost operator?')
    selected,logits,seconds,ntok=choice(runner,tok,prompt)
    detected=mapping[selected]
    # This oracle is intentionally given *gold* inner proposition/agent/time scaffolding.
    # It only uses the model prediction for the OUTERMOST operator.
    content=P if 'audit' in sentence else Q
    fact={'KNOWLEDGE':K('actor',content,'t0'), 'BELIEF':B('actor',content,'t0'),
          'COMMUNICATION':R('actor','receiver',content,'t0')}[detected]
    actual,trace=deduct([fact],content)
    actual_gold,trace_gold=deduct([{'KNOWLEDGE':K('actor',content,'t0'),
         'BELIEF':B('actor',content,'t0'),'COMMUNICATION':R('actor','receiver',content,'t0')}[gold]],content)
    return {'case_id':case_id,'sentence':sentence,'gold_outer_operator':gold,
            'model_outer_operator':detected,'operator_correct':detected==gold,
            'gold_semantic_query_outcome':actual_gold,'hybrid_query_outcome':actual,
            'hybrid_correct':actual_gold==actual,
            'verified_hybrid_trace':trace,'given_gold_scaffolding':['actor','content','time','structure'],
            'mapping':mapping,'chosen_letter':selected,'logits_uncalibrated':logits,
            'seconds':seconds,'prompt_tokens':ntok,'prompt':prompt}

def write_json(path,data):
    p=Path(path);p.parent.mkdir(parents=True,exist_ok=True)
    p.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf8')

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--model',default='/mnt/data/Qwen3-1.7B-Q4_K_M.gguf')
    p.add_argument('--cache',default='/mnt/data/epistemic-challenge-v001/cache')
    p.add_argument('--out',default='/mnt/data/epistemic-challenge-v001/results/challenge-001.json')
    p.add_argument('--limit',type=int,default=15)
    p.add_argument('--parser-limit',type=int,default=12)
    p.add_argument('--treatments',default='direct,challenge,parser')
    a=p.parse_args()
    model=GGUF(a.model);tok=Tokenizer(model.meta);runner=Runner(model,Store(model,a.cache))
    treatments=[x.strip() for x in a.treatments.split(',')]
    result={'experiment':'NICCOLÒ-EPISTEMIC-CHALLENGE-001','protocol':'preselected 15 synthetic micro-world cases; 12 operator parser items',
      'method':'GGUF experimental CPU runner; constrained first-token X/Y/Z logit (not actual text explanations)',
      'model_name':model.meta.get('general.name'),'expected_model_sha256':'d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5',
      'hypotheses':['Does test-case framing improve Qwen compared to direct framing on identical questions?',
                    'Can a tiny model parse a statement operator accurately enough to drive a narrow mathematical verifier?'],
      'limitations':['Synthetic preselected cases; no natural legal or personal input',
                     'Not blinded with respect to the harness designer; prompts fixed before running',
                     'No full natural-language generation of tests or explanations by Qwen',
                     'Parser hybrid uses gold actor/content/time scaffolding; not end-to-end',
                     'Not a head-to-head against DeepSeek',
                     'Custom unverified non-reference GGUF numerical runtime'],
      'cases':[], 'parser_cases':[], 'timings':{}, 'summaries':{}}
    start=time.monotonic()
    for i,item in enumerate(CASES[:a.limit]):
      for treatment in ('direct','challenge'):
       if treatment not in treatments: continue
       row=run_case(item,treatment,i,runner,tok)
       result['cases'].append(row)
       write_json(a.out,result)
       print('RESULT',row['case_id'],treatment,row['observed'],row['expected'],'PASS' if row['match'] else 'FAIL',row['seconds'],flush=True)
    if 'parser' in treatments:
      for i,item in enumerate(PARSER_CASES[:a.parser_limit]):
       row=run_parser_case(item,i,runner,tok)
       result['parser_cases'].append(row)
       write_json(a.out,result)
       print('PARSER',row['case_id'],row['model_outer_operator'],row['gold_outer_operator'],row['hybrid_correct'],row['seconds'],flush=True)
    result['timings']['wall_seconds']=round(time.monotonic()-start,2)
    for treatment in ('direct','challenge'):
       arr=[x for x in result['cases'] if x['treatment']==treatment]
       if arr: result['summaries'][treatment]={'correct':sum(x['match'] for x in arr),'total':len(arr)}
    if result['parser_cases']:
       arr=result['parser_cases']
       result['summaries']['parser']={'operator_correct':sum(x['operator_correct'] for x in arr),'hybrid_correct':sum(x['hybrid_correct'] for x in arr),'total':len(arr)}
    write_json(a.out,result)
    print('SUMMARY',json.dumps(result['summaries']),flush=True)

if __name__=='__main__': main()
