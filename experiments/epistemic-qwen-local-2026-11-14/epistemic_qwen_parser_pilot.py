#!/usr/bin/env python3
"""Epistemic semantic parsing pilot with local Qwen3-1.7B: 15 synthetic statements.

The model performs only outermost-operator recognition; formal closure is left to
an independent deterministic solver. Candidate labels are counterbalanced.
"""
import argparse,json,sys,time
from pathlib import Path
ROOT=Path(__file__).parent
sys.path.insert(0,str(ROOT/'runtime'))
from gguf_qwen3_experimental import GGUF,Store,Tokenizer,Runner
from eval_choice_model import enc_chat

CASES=[
 ('S01','K','Cheng knows that the audit is active.'),
 ('S02','B','Cheng believes the audit is active.'),
 ('S03','R','Cheng says the audit is active.'),
 ('S04','K','Maria knows that Cheng believes the audit is active.'),
 ('S05','B','Maria believes that Cheng knows the audit is active.'),
 ('S06','R','Maria reports that Cheng knows the audit is active.'),
 ('S07','K','Olivia knows that Maria reports that Cheng believes the audit is active.'),
 ('S08','B','Olivia suspects that Maria knows that Cheng approved the audit.'),
 ('S09','R','Olivia says that Maria believes Cheng knows about the audit.'),
 ('S10','K','Cheng knows that the audit is NOT active.'),
 ('S11','B','Cheng is convinced the audit is active.'),
 ('S12','R','Cheng testified that the audit is active.'),
 ('S13','B','Cheng suspects that Maria knows about the audit.'),
 ('S14','K','Cheng learned that the audit is active from a verified report.'),
 ('S15','R','Cheng claimed that the audit ended.'),
]
BASE=("Classify only the OUTERMOST epistemic operator expressed by the main subject of the sentence. "
      "K=knowledge (knows, learned); B=belief (believes, suspects, convinced); "
      "R=communication (says, reports, claims, testified). "
      "Even if the sentence has nested clauses, classify the OUTERMOST operator, "
      "NOT the innermost clause. Do not judge the proposition's truth. "
      "Examples: 'Maria knows Cheng believes p' has type K. "
      "'Maria believes Cheng knows p' has type B. "
      "'Maria claims Cheng knows p' has type R. "
      "Answer with exactly one letter, no explanation.")

def main():
 ap=argparse.ArgumentParser();ap.add_argument('--model',default='/mnt/data/Qwen3-1.7B-Q4_K_M.gguf');ap.add_argument('--cache',default='/mnt/data/qwen_model_runtime/converted');ap.add_argument('--out',default='/mnt/data/qwen_model_runtime/epistemic-parser-results.json');a=ap.parse_args()
 g=GGUF(a.model);t=Tokenizer(g.meta);r=Runner(g,Store(g,a.cache));labels='ABC';indices=[t.encode(x)[0] for x in labels];rows=[]
 for i,(case_id,truth,sentence) in enumerate(CASES):
  mapping={x:'KBR'[(j+i)%3] for j,x in enumerate(labels)}
  gold=next(letter for letter,category in mapping.items() if category==truth)
  instruction=' '.join(f'{ch}={mapping[ch]}.' for ch in labels)
  chat=(f'<|im_start|>system\n{BASE}\n{instruction}<|im_end|>\n'
        f'<|im_start|>user\n{sentence}\nWhich letter represents the outermost operator?<|im_end|>\n'
        f'<|im_start|>assistant\n<think>\n\n</think>\n')
  tokens=enc_chat(t,chat);r.cache=[];start=time.monotonic()
  _,logits=r.forward(tokens,0,choices=indices)
  predicted=labels[max(range(3),key=lambda j:logits[j])]
  row={'case':case_id,'sentence':sentence,'expected_type':truth,'mapping':mapping,'predicted_type':mapping[predicted],
       'correct':predicted==gold,'prompt_tokens':len(tokens),'seconds':round(time.monotonic()-start,2),
       'choice_logits':dict(zip(labels,[round(x,3) for x in logits]))}
  rows.append(row)
  out={'experiment':'outermost-operator-semantic-parsing','model':'Qwen3-1.7B Q4_K_M',
       'model_sha256':'d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5',
       'oracle':'synthetic human-authored tags; 5 K / 5 B / 5 R; counterbalanced first-letter labels',
       'method':'first-token constrained logits, non-thinking Qwen prefix, custom non-reference CPU GGUF runtime',
       'correct':sum(x['correct'] for x in rows),'total':len(rows),'records':rows}
  Path(a.out).write_text(json.dumps(out,indent=2))
  print('RESULT',json.dumps(row),flush=True)
 print('FINAL',json.dumps({'correct':out['correct'],'total':out['total']}),flush=True)
if __name__=='__main__':main()
