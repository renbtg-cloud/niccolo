"""Fresh bounded semantic benchmark; synthetic author-specified gold."""
from __future__ import annotations

import importlib.util
import itertools
import json
import random
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "challenge002_protocol", HERE.parent / "epistemic-challenge-002-2026-11-15" / "protocol.py")
previous = importlib.util.module_from_spec(spec)
spec.loader.exec_module(previous)
resolve, mental, neg, parse_visible = (previous.resolve, previous.mental,
                                      previous.neg, previous.parse_visible)
RELATIONS = ("K", "B", "R", "N", "A")  # A: ambiguous / abstain, no premise
LABELS = previous.LABELS
LETTERS = "VWXYZ"
SEED = 2026111503

# Fixed high-precision full-phrase recognition, rather than the predecessor's
# substring matching. Do not tune after observing model performance.
LEXICON = {
    "K": r"(?:knows|verified|confirmed|established|has direct evidence|has firsthand confirmation)",
    "B": r"(?:believes|suspects|thinks|assumes|is convinced|has a hunch)",
    "R": r"(?:said|told the team|reported|claimed|relayed)",
    "N": r"(?:does not know|is unaware|has no knowledge|has not learned)",
}


def lexical(phrase):
    return next((r for r, pattern in LEXICON.items()
                 if re.fullmatch(pattern, phrase, re.I)), "A")


def interpret(case, relations):
    facts = list(case["givens"])
    assert len(relations) == len(case["records"])
    for item, relation in zip(case["records"], relations):
        t, person, _, payload = parse_visible(item["text"])
        if relation != "A":
            facts.append(mental(relation, person, t, payload))
    return resolve(facts, case["rules"], case["query"])


def record(person, wording, payload, gold, time="t1"):
    return {"text": f"{time} | {person} | {wording} | {payload}", "gold": gold}


def rule(*premises, conclusion):
    return {"if": list(premises), "then": conclusion}


def build_cases():
    rng = random.Random(SEED)
    cases = []

    def pair(name, family, records, query, before, after, added, rules=(), givens=()):
        for phase, expected in enumerate((before, after)):
            cases.append({"case_id": f"{name}-r{phase}", "pair_id": name,
                          "phase": phase, "family": family, "records": records,
                          "query": query, "rules": list(rules),
                          "givens": list(givens) + (list(added) if phase else []),
                          "expected": expected})

    # Outcomes below are authored before running any model. The verifier must
    # reproduce every one under the stipulated record meanings.
    R = record
    S, F, U, C = LABELS
    pair("and-1", "conjunction", [R("Ela", "confirmed", "seal_intact", "K"),
         R("Noah", "checked the actual receipt and can vouch for", "fee_paid", "K")],
         "release_ready", S, C, ["!release_ready"],
         [rule("seal_intact", "fee_paid", conclusion="release_ready")])
    pair("and-2", "conjunction", [R("Uma", "has a hunch", "approval_exists", "B"),
         R("Zed", "verified", "funds_reserved", "K")], "ready", U, S,
         ["approval_exists"], [rule("approval_exists", "funds_reserved", conclusion="ready")])
    pair("and-3", "conjunction", [R("Tao", "circulated somebody else's assertion about", "permit_received", "R"),
         R("Mira", "has no knowledge", "insurance_live", "N")], "dispatch_ready", U, F,
         ["!dispatch_ready"], [rule("permit_received", "insurance_live", conclusion="dispatch_ready")])
    pair("and-4", "conjunction", [R("Eli", "cannot tell whether she knows", "report_valid", "A"),
         R("Sia", "confirmed", "signature_valid", "K")], "archive_ready", U, U,
         ["unrelated_note"], [rule("report_valid", "signature_valid", conclusion="archive_ready")])

    # Source group membership is a public stipulated rule, not hidden gold.
    pair("prov-1", "provenance", [R("Ivo", "reported", "invoice_missing", "R"),
         R("Nora", "forwarded an identical allegation about", "invoice_missing", "R")],
         "independent_corroboration", U, S, ["independent_register_confirms"],
         [rule("independent_register_confirms", mental("R", "Ivo", "t1", "invoice_missing"),
               conclusion="independent_corroboration")])
    pair("prov-2", "provenance", [R("Dara", "authenticated the original log and established", "entry_present", "K"),
         R("Hugo", "reported", "entry_present", "R")], "entry_present", S, C,
         ["!entry_present"])
    pair("prov-3", "provenance", [R("Pia", "put forward an unverified version of", "late_delivery", "R"),
         R("Rui", "read the source itself and verified", "late_delivery", "K")],
         "corroborated", S, S, ["!irrelevant_receipt"],
         [rule("late_delivery", mental("R", "Pia", "t1", "late_delivery"), conclusion="corroborated")])
    pair("prov-4", "provenance", [R("Sana", "believes", "key_changed", "B"),
         R("Vik", "could be quoting or confirming; no distinction is stated about", "key_changed", "A")],
         "key_changed", U, U, [mental("R", "Sana", "t2", "key_changed")])

    # Competing explanations are explicit, human-stipulated discriminators.
    h_rules = [rule("delay_for_review", conclusion="support_review"),
               rule("delay_for_bargaining", conclusion="support_bargaining"),
               rule("support_review", conclusion="prefer_review"),
               rule("support_bargaining", conclusion="!prefer_review")]
    pair("hyp-1", "competition", [R("Lian", "knows", "delay_for_review", "K"),
         R("Ari", "would put money on", "delay_for_bargaining", "B")],
         "prefer_review", S, C, ["delay_for_bargaining"], h_rules)
    pair("hyp-2", "competition", [R("Mina", "claimed", "delay_for_review", "R"),
         R("Niko", "personally checked and ascertained", "delay_for_bargaining", "K")],
         "prefer_review", F, C, ["delay_for_review"], h_rules)
    pair("hyp-3", "competition", [R("Orla", "has a working theory that", "delay_for_review", "B"),
         R("Paco", "is unaware", "delay_for_bargaining", "N")],
         "prefer_review", U, U, ["!delay_for_bargaining"], h_rules)
    pair("hyp-4", "competition", [R("Quin", "cannot determine whether the source establishes", "delay_for_review", "A"),
         R("Rosa", "relayed", "delay_for_bargaining", "R")],
         "prefer_review", U, S, ["delay_for_review"], h_rules)

    nested = mental("K", "Bela", "t0", "audit_started")
    pair("nest-1", "nested", [R("Anu", "saw incontrovertible evidence of", nested, "K"),
         R("Cai", "suspects", "!audit_started", "B")], "audit_started", S, C, ["!audit_started"])
    nested_b = mental("B", "Dino", "t0", "deal_closed")
    pair("nest-2", "nested", [R("Ema", "verified", nested_b, "K"),
         R("Fay", "is repeating another person's statement about", "deal_closed", "R")],
         "deal_closed", U, S, ["deal_closed"])
    pair("nest-3", "nested", [R("Gio", "has not been made privy to", "plan_changed", "N"),
         R("Hana", "confirmed", "plan_changed", "K")], mental("K", "Gio", "t1", "plan_changed"),
         F, C, [mental("K", "Gio", "t1", "plan_changed")])
    pair("nest-4", "nested", [R("Ina", "is known to have expressed either knowledge or doubt about", "contract_signed", "A"),
         R("Jai", "thinks", "contract_signed", "B")], "contract_signed", U, F, ["!contract_signed"])

    pair("time-1", "temporal_context", [R("Kai", "verified", "delayed_offer", "K", "t0"),
         R("Kai", "has established by personal inspection", "quick_offer", "K", "t2")],
         "anomaly", S, S, ["routine_call"],
         [rule(mental("K", "Kai", "t2", "quick_offer"), "personal_high_stakes_expects_delay",
               conclusion="anomaly")], ["personal_high_stakes_expects_delay"])
    pair("time-2", "temporal_context", [R("Lea", "knows", "decision_open", "K", "t0"),
         R("Max", "reckons it likely that", "decision_open", "B", "t2")],
         mental("K", "Lea", "t2", "decision_open"), U, F,
         [neg(mental("K", "Lea", "t2", "decision_open"))])
    pair("time-3", "temporal_context", [R("Nia", "remains without access to the information about", "offer_changed", "N", "t1"),
         R("Nia", "confirmed", "offer_changed", "K", "t2")],
         mental("K", "Nia", "t1", "offer_changed"), F, F, ["unrelated_message"])
    pair("time-4", "temporal_context", [R("Oli", "reported", "hard_bargaining", "R"),
         R("Paz", "has direct evidence", "high_stakes", "K")], "anomaly", U, S,
         ["hard_bargaining"], [rule("hard_bargaining", "high_stakes", "personal_baseline_expects_trust",
                                    conclusion="anomaly")], ["personal_baseline_expects_trust"])

    # Mapping is frozen independently of model outputs and balanced by class.
    def assign(items, key, values, letters, setter):
        for gold in values:
            group = [item for item in items if item[key] == gold]
            pos = list(letters) * (len(group) // len(letters))
            pos += rng.sample(list(letters), len(group) % len(letters))
            rng.shuffle(pos)
            for item, correct in zip(group, pos):
                distractors = iter(rng.sample([x for x in values if x != gold], len(values) - 1))
                setter(item, {letter: gold if letter == correct else next(distractors) for letter in letters})
    unique_records = [r for c in cases if c["phase"] == 0 for r in c["records"]]
    assign(unique_records, "gold", RELATIONS, LETTERS, lambda r, m: r.update(mapping=m))
    assign(cases, "expected", LABELS, LETTERS[1:], lambda c, m: c.update(mapping=m))
    rng.shuffle(cases)
    for c in cases:
        assert interpret(c, [r["gold"] for r in c["records"]])[0] == c["expected"], c
    return cases


def sensitivity(case, deterministic, chosen):
    """All possible relations for model-routed records, not a calibrated interval."""
    options = [(r,) if r != "A" else RELATIONS for r in deterministic]
    verdicts = sorted({interpret(case, combo)[0] for combo in itertools.product(*options)})
    return {"possible_verdicts": verdicts, "stable": len(verdicts) == 1,
            "chosen_verdict": interpret(case, chosen)[0]}


if __name__ == "__main__":
    (HERE / "cases.json").write_text(json.dumps(build_cases(), indent=2) + "\n")
