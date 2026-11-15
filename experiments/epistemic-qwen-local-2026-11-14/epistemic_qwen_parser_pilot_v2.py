#!/usr/bin/env python3
"""Corrected local-Qwen semantic extraction pilot, no symbol/label collisions.

Research-only, 15 synthetic examples; randomized-looking deterministic label
rotation to expose output position bias. No model training and no API calls.
"""
import argparse,json,sys,time
from pathlib import Path
ROOT=Path(__file__).parent
sys.path.insert(0,str(ROOT))
from epistemic_qwen_parser_pilot import CASES
sys.path.insert(0,str(ROOT/'runtime'))
from gguf_qwen3_experimental import GGUF,Store,Tokenizer,Runner
from eval_choice_model import enc_chat

TYPES=('KNOWLEDGE','BELIEF','COMMUNICATION')
TYPE_CODE={'K':'KNOWLEDGE','B':'BELIEF','R':'COMMUNICATION'}
RULES=(
  'Identify the MAIN subject\'s outermost mental or communicative act. '
  'There are exactly three categories: '
  'KNOWLEDGE = the subject knows or learned something; '
  'BELIEF = the subject believes, suspects, or is convinced of something; '
  'COMMUNICATION = the subject says, claims, testifies, or reports something. '
  'Nested clauses do not change the main subject\'s category. '
  'For example, "Maria knows Cheng believes a claim" has category KNOWLEDGE. '
  '"Maria believes Cheng knows a claim" has category BELIEF. '
  '"Maria reports Cheng knows a claim" has category COMMUNICATION. '
  'Classify the outermost action, without deciding whether the content is true. '
  'Choose one of X, Y or Z according to the mapping given below. '
  'Reply ONLY with the corresponding letter.'
)

def main():
 ap=argparse.ArgumentParser();ap.add_argument('--model',default='/mnt/data/Qwen3-1.7B-Q4_K_M.gguf');ap.add_argument('--cache',default='/mnt/data/qwen_model_runtime/converted');ap.add_argument('--out',default='/mnt/data/qwen_model_runtime/epistemic-parser-v2-results.json');a=ap.parse_args()
 g=GGUF(a.model);t=Tokenizer(g.meta);r=Runner(g,Store(g,a.cache));labels='XYZ';choice_ids=[t.encode(x)[0] for x in labels];rows=[]
 print('MODEL',g.meta.get('general.name'),'LABELS',dict(zip(labels,choice_ids)),flush=True)
 for i,(id,code,sentence) in enumerate(CASES):
  mapping={ch:TYPES[(i+j)%3] for j,ch in enumerate(labels)}
  gold=next(ch for ch,meaning in mapping.items() if meaning==TYPE_CODE[code])
  descriptions='\n'.join(f'{ch} = {mapping[ch]}' for ch in labels)
  chat=(f'<|im_start|>system\n{RULES}\n\n{descriptions}<|im_end|>\n'
        f'<|im_start|>user\n{sentence}\nWhich letter is correct?<|im_end|>\n'
        f'<|im_start|>assistant\n<think>\n\n</think>\n')
  ids=enc_chat(t,chat);r.cache=[];start=time.monotonic()
  _,logits=r.forward(ids,0,choices=choice_ids)
  pred=labels[max(range(3),key=lambda n:logits[n])]
  row={'id':id,'sentence':sentence,'expected':TYPE_CODE[code],'mapping':mapping,'predicted':mapping[pred],
       'correct':pred==gold,'tokens':len(ids),'seconds':round(time.monotonic()-start,2),
       'logits':dict(zip(labels,[round(x,3) for x in logits]))}
  rows.append(row)
  out={'experiment':'epistemic-semantic-outermost-v2-no-label-collision',
       'description':'15 synthetic non-blind statements, five per category. Balanced labels rotated by item. Research inference only.',
       'model':'Qwen3-1.7B Q4_K_M','model_sha256':'d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5',
       'method':'no K/B/R label collisions; system role prompt; first token X/Y/Z logit; custom non-reference GGUF CPU runtime',
       'correct':sum(x['correct'] for x in rows),'total':len(rows),'results':rows}
  Path(a.out).write_text(json.dumps(out,indent=2))
  print('RESULT',json.dumps(row),flush=True)
 print('FINAL',json.dumps({'correct':out['correct'],'total':out['total']}),flush=True)
if __name__=='__main__':main()
