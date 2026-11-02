//! Generated experiment program. DO NOT run user-supplied Rust in this harness.
//! Protocol: case_id|id:parent,...|observed_id,... => case_id|COUNT:n or UNKNOWN
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead};
const STRATEGY: u8 = __STRATEGY__;

fn count_roots(edges: &BTreeMap<u32, u32>, reports: &[u32]) -> Option<usize> {
    if STRATEGY == 0 { return Some(reports.len()); }
    let mut roots = BTreeSet::new();
    for &id in reports {
        if STRATEGY == 1 {
            // Deliberately defective: follows at most one transmission link.
            let parent = *edges.get(&id)?;
            roots.insert(if parent == 0 { id } else { parent });
            continue;
        }
        let mut at = id;
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(at) { return None; }
            let parent = *edges.get(&at)?;
            if parent == 0 { roots.insert(at); break; }
            at = parent;
        }
    }
    Some(roots.len())
}
fn parse_case(line: &str) -> Option<(&str, BTreeMap<u32,u32>,Vec<u32>)> {
    let mut parts = line.split('|');
    let label = parts.next()?;
    let rels = parts.next()?;
    let obs = parts.next()?;
    if label.is_empty() || parts.next().is_some() || obs.is_empty() { return None; }
    let mut edges = BTreeMap::new();
    for edge in rels.split(',') {
        let (id, parent) = edge.split_once(':')?;
        let id: u32 = id.parse().ok()?;
        let parent: u32 = parent.parse().ok()?;
        if id == 0 || edges.insert(id,parent).is_some() { return None; }
    }
    let mut reports = Vec::new();
    for raw in obs.split(',') {
        let id: u32 = raw.parse().ok()?;
        if id == 0 { return None; }
        reports.push(id);
    }
    Some((label,edges,reports))
}
fn main() {
    for line in io::stdin().lock().lines() {
        match line {
            Ok(line) => {
                if let Some((id,edges,reports)) = parse_case(&line) {
                    match count_roots(&edges, &reports) {
                        Some(n) => println!("{id}|COUNT:{n}"),
                        None => println!("{id}|UNKNOWN"),
                    }
                } else { println!("INVALID|UNKNOWN"); }
            }
            Err(_) => std::process::exit(2),
        }
    }
}
