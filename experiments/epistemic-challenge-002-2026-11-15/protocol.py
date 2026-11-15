"""Frozen synthetic experiment protocol; no model or network needed."""
from __future__ import annotations

import json
import random
import re
from pathlib import Path

SEED = 2026111502
LABELS = ("SUPPORTED", "REFUTED", "UNRESOLVED", "CONFLICT")
RELATIONS = ("K", "B", "R", "N")
LETTERS = "WXYZ"
ACTORS = ("Cheng", "Mara", "Nia", "Omar", "Priya", "Ada", "Leo", "Iris", "Sam", "Tess")

# Natural-language phrases intentionally vary in difficulty. Their intended
# meaning is stipulated by the synthetic author, not inferred from real people.
PHRASES = {
    "K": ("knows", "verified", "has firsthand confirmation", "established beyond doubt",
          "saw proof", "has direct evidence and now knows", "is aware as a fact"),
    "B": ("believes", "suspects", "has a hunch", "is leaning toward",
          "takes it to be true", "would bet", "is under the impression"),
    "R": ("reported", "told the team", "passed along the assertion", "repeated the account",
          "relayed", "put the allegation into circulation", "said"),
    "N": ("does not know", "is unaware", "has no knowledge", "remains in the dark",
          "has yet to find out", "is explicitly ignorant of the fact", "has not learned"),
}

# Each family has ten independent synthetic narratives, two revisions each.
# No demographic category is evidence; expectations are personal, supplied facts.
TOPICS = {
    "nested": ("audit_opened", "draft_changed", "meeting_cancelled", "key_revoked",
               "invoice_paid", "notice_delivered", "folder_moved", "offer_expired",
               "contract_signed", "shipment_arrived"),
    "contradiction": ("access_revoked", "payment_posted", "record_deleted", "approval_given",
                      "meeting_held", "offer_rejected", "report_sent", "receipt_found",
                      "vote_counted", "file_received"),
    "hypothesis": ("deadline_binding", "trust_building_signal", "resource_shortage",
                   "delegated_decision", "policy_constraint", "legal_review_pending",
                   "competing_offer", "budget_freeze", "schedule_conflict", "approval_pending"),
    "context": ("aggressive_high_stakes", "delayed_high_stakes", "abrupt_low_stakes",
                "unusually_formal_call", "rushed_high_stakes", "unscheduled_escalation",
                "silence_after_offer", "direct_refusal", "aggressive_routine",
                "shortened_negotiation"),
    "temporal": ("investigation_opened", "access_enabled", "decision_made", "offer_received",
                 "evidence_seen", "team_informed", "contract_drafted", "meeting_confirmed",
                 "notice_sent", "request_seen"),
}

# The expected verdicts are authored independently of the executable verifier.
# These ten patterns recur across families: statistical unit is a pair/template,
# not 100 independent observations.
MODES = (
    ("K", "derived", "negate", "SUPPORTED", "CONFLICT"),
    ("B", "derived", "affirm", "UNRESOLVED", "SUPPORTED"),
    ("R", "derived", "negate", "UNRESOLVED", "REFUTED"),
    ("N", "own_knowledge", "affirm", "REFUTED", "CONFLICT"),
    ("K", "derived", "irrelevant", "SUPPORTED", "SUPPORTED"),
    ("B", "own_belief", "negate", "SUPPORTED", "CONFLICT"),
    ("R", "own_report", "irrelevant", "SUPPORTED", "SUPPORTED"),
    ("N", "own_knowledge", "irrelevant", "REFUTED", "REFUTED"),
    ("K", "negative_content", "affirm", "REFUTED", "CONFLICT"),
    ("B", "derived", "irrelevant", "UNRESOLVED", "UNRESOLVED"),
)


def neg(p):
    return p[1:] if p.startswith("!") else "!" + p


def mental(relation, actor, time, p):
    if relation == "N":
        return neg(mental("K", actor, time, p))
    if relation not in ("K", "B", "R"):
        raise ValueError(relation)
    return f"{relation}({actor},{time},{p})"


def factive_payload(statement):
    """Only outer K is factive. Inner payload can itself be a mental fact."""
    match = re.fullmatch(r"K\([^,()]+,[^,()]+,(.+)\)", statement)
    return match.group(1) if match else None


def resolve(facts, rules, query):
    """Paraconsistent forward closure. No explosion, contraposition or persistence."""
    known = set(facts)
    trace = [{"rule": "stipulated", "conclusion": x} for x in facts]
    while True:
        changes = []
        for x in sorted(known):
            p = factive_payload(x)
            if p is not None and p not in known:
                changes.append(("factivity", [x], p))
        for rule in rules:
            if set(rule["if"]) <= known and rule["then"] not in known:
                changes.append(("explicit_rule", rule["if"], rule["then"]))
        if not changes:
            break
        for rule, parents, p in changes:
            if p not in known:
                known.add(p)
                trace.append({"rule": rule, "premises": parents, "conclusion": p})
    yes, no = query in known, neg(query) in known
    return {(True, False): "SUPPORTED", (False, True): "REFUTED",
            (False, False): "UNRESOLVED", (True, True): "CONFLICT"}[yes, no], trace


def parse_visible(line):
    """The ONLY source for the pivotal actor, time, wording and payload.

    All four are explicitly present in the public semi-structured record.
    No hidden semantic fields are given to either hybrid.
    """
    match = re.fullmatch(r"(t\d+) \| ([A-Za-z]+) \| (.+?) \| (.+)", line)
    if not match:
        raise ValueError(f"Bad record: {line}")
    return match.groups()


def lexical(phrase):
    """Fixed, non-LLM high-precision baseline. Unknown wording abstains."""
    patterns = (
        ("N", r"\b(?:does not know|is unaware|no knowledge|has not learned)\b"),
        ("K", r"\b(?:knows|verified|confirmed|established|learned|direct evidence)\b"),
        ("B", r"\b(?:believes|suspects|thinks|assumes|convinced)\b"),
        ("R", r"\b(?:said|told|reported|claimed|relayed)\b"),
    )
    return next((r for r, pattern in patterns if re.search(pattern, phrase, re.I)), None)


def interpret(case, relation):
    time, actor, _, p = parse_visible(case["record"])
    facts = list(case["givens"])
    if relation is not None:
        facts.append(mental(relation, actor, time, p))
    return resolve(facts, case["rules"], case["query"])


def build_cases():
    rng = random.Random(SEED)
    phrase_queues = {r: [] for r in RELATIONS}
    cases = []
    for family, topics in TOPICS.items():
        for i, (relation, mode, revision, before, after) in enumerate(MODES):
            actor, other = rng.sample(ACTORS, 2)
            topic = topics[i]
            time = "t1"
            payload, target, givens, rules = topic, topic, [], []
            if family == "nested":
                payload = mental("K", other, "t0", topic)
            elif family == "temporal":
                payload = mental("K", other, "t0", topic)
                target = mental("K", other, "t0", topic)
                if i == 9:  # The later time is distinct; no persistence.
                    target = mental("K", other, "t2", topic)
            elif family == "hypothesis":
                target = "support_for_explanation_" + topic
                context = "human_approved_discriminator_" + topic
                givens = [context]
                rules = [{"if": [topic, context], "then": target}]
            elif family == "context":
                target = "anomaly_relative_to_personal_baseline_" + topic
                context = "personal_baseline_expects_other_behavior_" + topic
                givens = [context]
                rules = [{"if": [topic, context], "then": target}]
            if mode == "own_knowledge":
                query = mental("K", actor, time, payload)
            elif mode == "own_belief":
                query = mental("B", actor, time, payload)
            elif mode == "own_report":
                query = mental("R", actor, time, payload)
            elif mode == "negative_content":
                query = target
                payload = neg(query)
            else:
                query = target
            # Explicit contradictory evidence outside target prevents simplistic
            # "any contradiction => conflict" heuristics. One per family.
            if i == 9:
                givens += ["unrelated_note", "!unrelated_note"]
            if not phrase_queues[relation]:
                phrase_queues[relation] = list(PHRASES[relation])
                rng.shuffle(phrase_queues[relation])
            wording = phrase_queues[relation].pop()
            record = f"{time} | {actor} | {wording} | {payload}"
            new = {"affirm": query, "negate": neg(query),
                   "irrelevant": "unrelated_receipt_archived"}[revision]
            pair_id = f"{family}-{i + 1:02}"
            for phase, expected, additions in ((0, before, []), (1, after, [new])):
                cases.append({"case_id": f"{pair_id}-r{phase}", "pair_id": pair_id,
                              "family": family, "phase": phase, "record": record,
                              "givens": givens + additions, "rules": rules, "query": query,
                              "gold_relation": relation, "expected": expected})
    # Randomize and approximately balance the correct letter within each gold
    # class. Other positions are independently shuffled. This avoids a class-
    # correlated deterministic rotation and large accidental letter imbalance.
    def assign_mappings(items, key, options):
        for gold in options:
            group = [c for c in items if c[key] == gold]
            positions = list(LETTERS) * (len(group) // 4)
            positions += rng.sample(list(LETTERS), len(group) % 4)
            rng.shuffle(positions)
            for case, pos in zip(group, positions):
                wrong = iter(rng.sample([x for x in options if x != gold], 3))
                yield case, {letter: gold if letter == pos else next(wrong) for letter in LETTERS}
    for case, mapping in assign_mappings(cases, "expected", LABELS):
        case["answer_mapping"] = mapping
    pair_items = [c for c in cases if c["phase"] == 0]
    for item, mapping in assign_mappings(pair_items, "gold_relation", RELATIONS):
        for case in cases:
            if case["pair_id"] == item["pair_id"]:
                case["parse_mapping"] = mapping.copy()
    rng.shuffle(cases)
    return cases


def freeze(path):
    cases = build_cases()
    for case in cases:
        observed, _ = interpret(case, case["gold_relation"])
        assert observed == case["expected"], (case, observed)
    Path(path).write_text(json.dumps(cases, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    freeze(Path(__file__).with_name("cases.json"))
