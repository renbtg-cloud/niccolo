"""Epistemic Challenge 004: deterministic generation of 1,000 frozen synthetic cases.

No model or network is required to generate or score the corpus. The committed
source + seed + FROZEN_MANIFEST.json define the corpus. The raw 1,000-case JSON
is intentionally generated locally rather than committed as a multi-megabyte
artifact; its canonical SHA-256 is pinned in the manifest.
"""
from __future__ import annotations

import hashlib
import itertools
import json
import random
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
SEED = 2026111604
BASE_SITUATIONS = 250
REVISIONS = 4
TOTAL_CASES = BASE_SITUATIONS * REVISIONS
HOLDOUT_BASES = 50
LABELS = ("SUPPORTED", "REFUTED", "UNRESOLVED", "CONFLICT")
RELATIONS = ("K", "B", "R", "N", "A")
LETTERS_REL = "VWXYZ"
LETTERS_VERDICT = "WXYZ"

ACTORS = (
    "Ari", "Bela", "Cai", "Dara", "Eli", "Fara", "Gio", "Hana", "Ivo", "Jia",
    "Kai", "Lena", "Miro", "Nia", "Omar", "Pia", "Quin", "Rui", "Sana", "Tao",
    "Uma", "Vik", "Wen", "Xara", "Yuri", "Zed", "Noa", "Mina", "Paz", "Rosa",
)
FAMILIES = (
    "long_chain", "conjunction", "nested", "provenance", "competition",
    "temporal", "context_shift", "noise", "parser_trap", "mixed",
)

LEXICON = {
    "K": {
        "knows", "verified", "confirmed", "established", "has direct evidence",
        "has firsthand confirmation", "personally verified",
        "confirmed only that someone else alleged",
        "knows only as a working assumption",
    },
    "B": {"believes", "suspects", "thinks", "assumes", "is convinced", "has a hunch"},
    "R": {"said", "reported", "claimed", "relayed", "told the team"},
    "N": {"does not know", "is unaware", "has no knowledge", "has not learned"},
}
KNOWN = {
    "K": tuple(sorted(LEXICON["K"] - {"confirmed only that someone else alleged", "knows only as a working assumption"})),
    "B": tuple(sorted(LEXICON["B"])),
    "R": tuple(sorted(LEXICON["R"])),
    "N": tuple(sorted(LEXICON["N"])),
}
UNFAMILIAR = {
    "K": (
        "authenticated the source and can vouch for", "personally inspected the record and found",
        "has direct firsthand verification of", "checked the primary evidence and established",
    ),
    "B": (
        "would bet that", "leans toward", "has a working theory that", "regards as plausible",
    ),
    "R": (
        "passed along somebody else's assertion about", "circulated an unverified account of",
        "repeated what another source said about", "forwarded the allegation that",
    ),
    "N": (
        "remains without access to the information about", "has not been made privy to",
        "cannot yet know", "has not obtained the information needed to know",
    ),
}
AMBIGUOUS = (
    "cannot tell whether the source knows or merely suspects",
    "could be quoting or confirming; the distinction is unstated",
    "is described as somehow aware, but the relation is unspecified",
    "has an unclear epistemic stance toward",
)
TRAPS = {
    "R": "confirmed only that someone else alleged",
    "B": "knows only as a working assumption",
}
NONFACTIVE = ("B", "R", "N", "A")


def neg(p: str) -> str:
    return p[1:] if p.startswith("!") else "!" + p


def mental(relation: str, actor: str, time: str, payload: str) -> str:
    if relation == "N":
        return neg(mental("K", actor, time, payload))
    if relation not in ("K", "B", "R"):
        raise ValueError(relation)
    return f"{relation}({actor},{time},{payload})"


def factive_payload(statement: str):
    match = re.fullmatch(r"K\([^,()]+,[^,()]+,(.+)\)", statement)
    return match.group(1) if match else None


def resolve(facts, rules, query):
    """Paraconsistent forward closure; no explosion, contraposition or persistence."""
    known = set(facts)
    trace = [{"rule": "stipulated", "conclusion": x} for x in facts]
    while True:
        changes = []
        for x in sorted(known):
            payload = factive_payload(x)
            if payload is not None and payload not in known:
                changes.append(("factivity", [x], payload))
        for item in rules:
            if set(item["if"]) <= known and item["then"] not in known:
                changes.append(("explicit_rule", list(item["if"]), item["then"]))
        if not changes:
            break
        for rule_name, parents, conclusion in changes:
            if conclusion not in known:
                known.add(conclusion)
                trace.append({"rule": rule_name, "premises": parents, "conclusion": conclusion})
    yes, no = query in known, neg(query) in known
    verdict = {(True, False): "SUPPORTED", (False, True): "REFUTED",
               (False, False): "UNRESOLVED", (True, True): "CONFLICT"}[yes, no]
    return verdict, trace


def parse_visible(line: str):
    match = re.fullmatch(r"(t\d+) \| ([A-Za-z]+) \| (.+?) \| (.+)", line)
    if not match:
        raise ValueError(f"Bad record: {line}")
    return match.groups()


def lexical(phrase: str) -> str:
    lowered = phrase.casefold()
    for relation, phrases in LEXICON.items():
        if lowered in {p.casefold() for p in phrases}:
            return relation
    return "A"


def nested_payload(atom: str, actors: list[str], phase: int, depth: int) -> str:
    out = atom
    for i in range(depth):
        out = mental("K", actors[(phase + i) % len(actors)], f"t{max(0, phase-i)}", out)
    return out


def record(time: str, actor: str, wording: str, payload: str, gold: str, role: str) -> dict:
    return {"text": f"{time} | {actor} | {wording} | {payload}", "gold": gold, "role": role}


def rule(*premises: str, conclusion: str) -> dict:
    return {"if": list(premises), "then": conclusion}


def canonical_json_bytes(value) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n").encode("utf-8")


def corpus_sha256(cases) -> str:
    return hashlib.sha256(canonical_json_bytes(cases)).hexdigest()


def _mapping(rng: random.Random, gold: str, values: tuple[str, ...], letters: str) -> dict:
    correct = rng.choice(list(letters))
    distractors = iter(rng.sample([x for x in values if x != gold], len(values) - 1))
    return {letter: gold if letter == correct else next(distractors) for letter in letters}


def _nonfactive_relation(base: int, phase: int, side: int) -> str:
    return NONFACTIVE[(base * 3 + phase * 5 + side) % len(NONFACTIVE)]


def _wording(rng: random.Random, relation: str, mode: str) -> str:
    if relation == "A":
        return rng.choice(AMBIGUOUS)
    if mode == "known":
        return rng.choice(KNOWN[relation])
    if mode == "trap" and relation in TRAPS:
        return TRAPS[relation]
    return rng.choice(UNFAMILIAR[relation])


def _chain(prefix: str, seed: str, query: str, depth: int, context: list[str]) -> list[dict]:
    rules = []
    prev = seed
    for step in range(depth - 1):
        nxt = f"{prefix}_step{step+1}"
        premises = [prev]
        if context and step % 2 == 0:
            premises.append(context[step % len(context)])
        rules.append(rule(*premises, conclusion=nxt))
        prev = nxt
    premises = [prev]
    if context:
        premises.append(context[-1])
    rules.append(rule(*premises, conclusion=query))
    return rules


def build_cases() -> list[dict]:
    rng = random.Random(SEED)
    cases = []
    base_ids = [f"s{idx:03d}" for idx in range(BASE_SITUATIONS)]
    holdout = set(rng.sample(base_ids, HOLDOUT_BASES))

    for base_index, base_id in enumerate(base_ids):
        family = FAMILIES[base_index % len(FAMILIES)]
        actors = rng.sample(list(ACTORS), 4 + (base_index % 5))
        chain_depth = 1 + (base_index % 6)
        nested_depth = (base_index // 3) % 4 if family in ("nested", "mixed") else 0
        noise_count = 1 + (base_index % 5) if family in ("noise", "mixed") else base_index % 3
        contradiction_noise = family in ("noise", "mixed", "competition") or base_index % 7 == 0
        unknown_phase = base_index % 4
        double_unknown = base_index % 10 == 0
        trap_phase = (base_index + 2) % 4 if family == "parser_trap" or base_index % 20 == 0 else None

        stale_records: list[dict] = []
        for phase in range(REVISIONS):
            desired = LABELS[(base_index + phase) % len(LABELS)]
            query = f"q_{base_id}_t{phase}"
            pos_atom = f"pos_{base_id}_t{phase}"
            neg_atom = f"neg_{base_id}_t{phase}"
            pos_relation = "K" if desired in ("SUPPORTED", "CONFLICT") else _nonfactive_relation(base_index, phase, 0)
            neg_relation = "K" if desired in ("REFUTED", "CONFLICT") else _nonfactive_relation(base_index, phase, 1)

            context = [f"ctx_{base_id}_t{phase}_{i}" for i in range(1 + (base_index + phase) % 3)]
            givens = list(context)
            if family in ("context_shift", "mixed"):
                givens.append(f"personal_baseline_{base_id}_t{phase}")
            if contradiction_noise:
                givens += [f"noise_{base_id}_{phase}", neg(f"noise_{base_id}_{phase}")]

            pos_payload = pos_atom
            neg_payload = neg_atom
            if family in ("nested", "mixed") and nested_depth:
                pos_payload = nested_payload(pos_atom, actors, phase, nested_depth)
                neg_payload = nested_payload(neg_atom, list(reversed(actors)), phase, nested_depth)

            rules: list[dict] = []
            pos_start, neg_start = pos_atom, neg_atom
            if family == "provenance":
                source_pos = mental("R", actors[2], f"t{phase}", pos_atom)
                source_neg = mental("R", actors[3], f"t{phase}", neg_atom)
                pos_payload, neg_payload = source_pos, source_neg
                marker_p = f"independent_pos_{base_id}_t{phase}"
                marker_n = f"independent_neg_{base_id}_t{phase}"
                givens += [marker_p, marker_n]
                rules += [rule(source_pos, marker_p, conclusion=pos_atom),
                          rule(source_neg, marker_n, conclusion=neg_atom)]

            if family == "conjunction":
                aux_p = f"aux_pos_{base_id}_t{phase}"
                aux_n = f"aux_neg_{base_id}_t{phase}"
                givens += [aux_p, aux_n]
                rules += [rule(pos_atom, aux_p, conclusion=f"pos_ready_{base_id}_t{phase}"),
                          rule(neg_atom, aux_n, conclusion=f"neg_ready_{base_id}_t{phase}")]
                pos_start = f"pos_ready_{base_id}_t{phase}"
                neg_start = f"neg_ready_{base_id}_t{phase}"

            rules += _chain(f"p_{base_id}_t{phase}", pos_start, query, chain_depth, context)
            rules += _chain(f"n_{base_id}_t{phase}", neg_start, neg(query),
                            1 + ((chain_depth + 2) % 6), list(reversed(context)))

            pos_mode = "unfamiliar" if phase == unknown_phase else "known"
            neg_mode = "unfamiliar" if (double_unknown and phase == unknown_phase) else "known"
            if trap_phase == phase:
                if base_index % 2:
                    pos_relation, pos_mode = "R", "trap"
                else:
                    neg_relation, neg_mode = "B", "trap"

            current = [
                record(f"t{phase}", actors[0], _wording(rng, pos_relation, pos_mode), pos_payload, pos_relation, "positive_pivotal"),
                record(f"t{phase}", actors[1], _wording(rng, neg_relation, neg_mode), neg_payload, neg_relation, "negative_pivotal"),
            ]

            for j in range(noise_count):
                relation = ("K", "B", "R", "N")[(base_index + phase + j) % 4]
                current.append(record(
                    f"t{phase}", actors[(2 + j) % len(actors)], rng.choice(KNOWN[relation]),
                    f"distractor_{base_id}_t{phase}_{j}", relation, "distractor"))

            records = list(stale_records) + current
            stale_records = (stale_records + current[:2])[-6:]

            for item in current:
                item["mapping"] = _mapping(rng, item["gold"], RELATIONS, LETTERS_REL)

            case = {
                "case_id": f"{base_id}-r{phase}", "base_id": base_id, "phase": phase,
                "family": family, "holdout": base_id in holdout,
                "records": records, "givens": givens, "rules": rules, "query": query,
                "expected": desired,
                "difficulty": {
                    "actors": len(actors), "records": len(records), "chain_depth": chain_depth,
                    "nested_depth": nested_depth, "noise_records": noise_count,
                    "contradiction_noise": contradiction_noise,
                    "unknown_pivotal": phase == unknown_phase,
                    "double_unknown": double_unknown and phase == unknown_phase,
                    "parser_trap": trap_phase == phase,
                },
            }
            case["mapping"] = _mapping(rng, desired, LABELS, LETTERS_VERDICT)
            cases.append(case)

    rng.shuffle(cases)
    for case in cases:
        observed, _ = interpret(case, [r["gold"] for r in case["records"]])
        if observed != case["expected"]:
            raise AssertionError((case["case_id"], case["expected"], observed))
    return cases


def interpret(case: dict, relations: list[str]):
    if len(relations) != len(case["records"]):
        raise ValueError("relation count mismatch")
    facts = list(case["givens"])
    for item, relation in zip(case["records"], relations):
        time, actor, _, payload = parse_visible(item["text"])
        if relation != "A":
            facts.append(mental(relation, actor, time, payload))
    return resolve(facts, case["rules"], case["query"])


def deterministic_relations(case: dict) -> list[str]:
    return [lexical(parse_visible(item["text"])[2]) for item in case["records"]]


def sensitivity(case: dict, deterministic: list[str], chosen: list[str]):
    positions = [i for i, relation in enumerate(deterministic) if relation == "A"]
    if len(positions) > 2:
        return {"stable": False, "possible_verdicts": list(LABELS), "bounded": False,
                "chosen_verdict": interpret(case, chosen)[0]}
    options = [(relation,) if i not in positions else RELATIONS
               for i, relation in enumerate(deterministic)]
    verdicts = sorted({interpret(case, list(combo))[0] for combo in itertools.product(*options)})
    return {"stable": len(verdicts) == 1, "possible_verdicts": verdicts, "bounded": True,
            "chosen_verdict": interpret(case, chosen)[0]}


def structural_score(case: dict) -> int:
    difficulty = case["difficulty"]
    score = difficulty["chain_depth"] * 4 + difficulty["nested_depth"] * 7
    score += min(difficulty["records"], 12) * 2 + difficulty["noise_records"] * 2
    score += 8 if difficulty["contradiction_noise"] else 0
    score += 12 if difficulty["double_unknown"] else 0
    score += 15 if difficulty["parser_trap"] else 0
    score += {"mixed": 10, "provenance": 7, "competition": 6, "context_shift": 5}.get(case["family"], 0)
    return score


def escalation_sets(cases: list[dict], high_n: int = 200, max_n: int = 30):
    eligible = []
    for case in cases:
        relations = deterministic_relations(case)
        if "A" in relations:
            tie = hashlib.sha256(case["case_id"].encode()).hexdigest()
            eligible.append((structural_score(case), tie, case["case_id"]))
    eligible.sort(reverse=True)
    high = {case_id for _, _, case_id in eligible[:high_n]}
    maximum = {case_id for _, _, case_id in eligible[:max_n]}
    if not maximum <= high:
        raise AssertionError("max cohort must be subset of high")
    return high, maximum


def corpus_summary(cases: list[dict]) -> dict:
    high, maximum = escalation_sets(cases)
    return {
        "seed": SEED,
        "base_situations": BASE_SITUATIONS,
        "revisions": REVISIONS,
        "cases": len(cases),
        "holdout_bases": len({c["base_id"] for c in cases if c["holdout"]}),
        "holdout_cases": sum(c["holdout"] for c in cases),
        "label_counts": {label: sum(c["expected"] == label for c in cases) for label in LABELS},
        "family_counts": {family: sum(c["family"] == family for c in cases) for family in FAMILIES},
        "deterministic_abstention_cases": sum("A" in deterministic_relations(c) for c in cases),
        "parser_trap_cases": sum(c["difficulty"]["parser_trap"] for c in cases),
        "high_cases": len(high),
        "max_cases": len(maximum),
        "corpus_sha256": corpus_sha256(cases),
        "high_ids_sha256": hashlib.sha256("\n".join(sorted(high)).encode()).hexdigest(),
        "max_ids_sha256": hashlib.sha256("\n".join(sorted(maximum)).encode()).hexdigest(),
    }


if __name__ == "__main__":
    cases = build_cases()
    (HERE / "cases.generated.json").write_bytes(canonical_json_bytes(cases))
    print(json.dumps(corpus_summary(cases), indent=2))
