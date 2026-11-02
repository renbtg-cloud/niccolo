//! Generated only from the trusted domain-reasoning template.
//! Input: case_id|node:parent[+parent]:A_or_R,...|observation,...
//! Output: case_id|COUNT:n or UNKNOWN (not factual truth/independence).
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead};
const MAX_DEPTH: u8 = __DEPTH__; // 0, 1, 2 (transitive)
const REJECT_MISSING: bool = __MISSING__;
const REJECT_REVOKED: bool = __REVOKED__;

#[derive(Clone)]
struct Node { parents: Vec<u32>, revoked: bool }

fn trace_origins(
    id: u32,
    nodes: &BTreeMap<u32, Node>,
    path: &mut BTreeSet<u32>,
    hops: u32,
    origins: &mut BTreeSet<u32>,
) -> Result<(), ()> {
    if !path.insert(id) { return Err(()); }
    let result = (|| {
        let node = match nodes.get(&id) {
            Some(n) => n,
            None if REJECT_MISSING => return Err(()),
            None => { origins.insert(id); return Ok(()); }
        };
        if node.revoked && REJECT_REVOKED { return Err(()); }
        if node.parents.is_empty() || MAX_DEPTH == 0 || (MAX_DEPTH == 1 && hops >= 1) {
            origins.insert(id);
            return Ok(());
        }
        for &parent in &node.parents { trace_origins(parent, nodes, path, hops+1, origins)?; }
        Ok(())
    })();
    path.remove(&id);
    result
}

fn evaluate(edges: &str, reports: &str) -> Option<usize> {
    let mut nodes = BTreeMap::new();
    for item in edges.split(',') {
        let mut fields = item.split(':');
        let id: u32 = fields.next()?.parse().ok()?;
        let parents_text = fields.next()?;
        let status = fields.next()?;
        if fields.next().is_some() || id == 0 { return None; }
        if status != "A" && status != "R" { return None; }
        let parents = if parents_text == "0" { Vec::new() } else {
            let mut values = BTreeSet::new();
            for s in parents_text.split('+') {
                let p: u32 = s.parse().ok()?;
                if p == 0 || !values.insert(p) { return None; }
            }
            if values.is_empty() { return None; }
            values.into_iter().collect()
        };
        if nodes.insert(id, Node { parents, revoked: status == "R" }).is_some() { return None; }
    }
    let mut roots = BTreeSet::new();
    let mut any = false;
    for report in reports.split(',') {
        let id: u32 = report.parse().ok()?;
        if id == 0 { return None; }
        any = true;
        trace_origins(id, &nodes, &mut BTreeSet::new(), 0, &mut roots).ok()?;
    }
    any.then_some(roots.len())
}

fn main() {
    for line in io::stdin().lock().lines() {
        match line {
            Ok(line) => {
                let fields: Vec<_> = line.split('|').collect();
                if fields.len() != 3 || fields[0].is_empty() {
                    println!("INVALID|UNKNOWN");
                } else {
                    match evaluate(fields[1], fields[2]) {
                        Some(n) => println!("{}|COUNT:{n}", fields[0]),
                        None => println!("{}|UNKNOWN", fields[0]),
                    }
                }
            }
            Err(_) => std::process::exit(2),
        }
    }
}
