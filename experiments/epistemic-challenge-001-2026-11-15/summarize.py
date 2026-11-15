#!/usr/bin/env python3
"""Recalculate experiment-001 summaries from raw model outputs, without inference."""
import argparse
import collections
import json
import math
from pathlib import Path

VERDICTS=('SUPPORTED','REFUTED','UNRESOLVED')
TYPES=('KNOWLEDGE','BELIEF','COMMUNICATION')

def simple_lexical_classifier(sentence):
    """Deliberately trivial baseline for overt verbs in synthetic statements.

    Not a general NLP or multi-lingual pipeline. Perfect results on this
    hand-designed dataset must not be extrapolated to unseen human material.
    """
    s=' '+sentence.lower()+' '
    if any(w in s for w in (' knows ', ' learned ', ' understands ')):
        return 'KNOWLEDGE'
    if any(w in s for w in (' believes ', ' suspects ', ' is convinced ')):
        return 'BELIEF'
    if any(w in s for w in (' says ', ' reports ', ' told ', ' claims ')):
        return 'COMMUNICATION'
    return 'UNKNOWN'

def exact_mcnemar_pvalue(b,c):
    """Two-sided conditional binomial sign test, p=0.5, for discordant pairs."""
    n=b+c
    if not n: return 1.0
    return min(1.0,2*sum(math.comb(n,k) for k in range(min(b,c)+1))/(2**n))

def compute(r):
    cases=r['cases'];parser=r['parser_cases']; by_t={t:{x['case_id']:x for x in cases if x['treatment']==t} for t in ('direct','challenge')}
    assert set(by_t['direct'])==set(by_t['challenge']) and len(by_t['direct'])==15
    keys=sorted(by_t['direct'])
    b=sum((not by_t['direct'][k]['match']) and by_t['challenge'][k]['match'] for k in keys)
    c=sum(by_t['direct'][k]['match'] and (not by_t['challenge'][k]['match']) for k in keys)
    model_correct=sum(x['operator_correct'] for x in parser)
    hybrid_correct=sum(x['hybrid_correct'] for x in parser)
    lexcorrect=sum(simple_lexical_classifier(x['sentence'])==x['gold_outer_operator'] for x in parser)
    per_t={}
    for t in by_t:
        rows=list(by_t[t].values())
        per_t[t]={
            'correct':sum(x['match'] for x in rows),
            'total':len(rows),
            'prediction_distribution':dict(collections.Counter(x['observed'] for x in rows)),
            'gold_distribution':dict(collections.Counter(x['expected'] for x in rows)),
            'mean_prompt_tokens':round(sum(x['prompt_tokens'] for x in rows)/len(rows),2),
            'total_inference_seconds':round(sum(x['seconds'] for x in rows),2),
            'by_gold':{v:{'correct':sum(x['match'] for x in rows if x['expected']==v),
                        'total':sum(x['expected']==v for x in rows)} for v in VERDICTS},
        }
    return {
        'experiment':r['experiment'],
        'matched_cases':len(keys),
        'direct':per_t['direct'],
        'challenge':per_t['challenge'],
        'paired_difference':{'challenge_only_correct':b,'direct_only_correct':c,
                            'two_sided_exact_mcnemar_pvalue':exact_mcnemar_pvalue(b,c)},
        'operator_parser':{'correct':model_correct,'total':len(parser),
                           'by_category':{v:{'correct':sum(x['operator_correct'] for x in parser if x['gold_outer_operator']==v),
                                             'total':sum(x['gold_outer_operator']==v for x in parser)} for v in TYPES}},
        'hybrid_narrow_query':{'correct':hybrid_correct,'total':len(parser),
                               'gold_structure_used':['agent','proposition','time','nested content']},
        'trivial_lexical_baseline':{'correct':lexcorrect,'total':len(parser)},
        'runtime':'Experimental non-reference Qwen GGUF CPU forward; token-logit choices only',
        'caveats':['Small synthetic non-blind dataset; no inference that DeepSeek improves',
                   'Direct vs challenge prompts have different token counts and few-shot examples',
                   'Constrained first-token classification does NOT test free-form test-log generation',
                   'Hybrid given gold agent/proposition/time: NOT end-to-end extraction',
                   'Trivial lexical baseline is tailored to overt synthetic keywords',
                   'No independent validation of GGUF runner numerical equivalence']}

def main():
    p=argparse.ArgumentParser();p.add_argument('input');p.add_argument('--out');a=p.parse_args()
    r=json.loads(Path(a.input).read_text())
    summary=compute(r)
    text=json.dumps(summary,indent=2,ensure_ascii=False)+'\n'
    if a.out:Path(a.out).write_text(text)
    print(text)
if __name__=='__main__': main()
