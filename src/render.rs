//! Artifacts derived from the transition table: a mermaid diagram and an
//! exhaustive coverage matrix.

use std::any::TypeId;
use std::fmt::Write as _;

use crate::Domain;
use crate::machine::{Edge, Goto, Ignore, OnUnknown, State};

/// The result of checking every `(state × event kind)` combination.
#[derive(Debug, Default)]
pub struct Coverage {
    /// Combinations with neither an edge nor an [`Ignore`]. **Must be empty in
    /// CI.**
    pub holes: Vec<(String, String)>,
    /// Combinations matched by more than one edge. Not necessarily wrong, since
    /// declaration order is priority, but worth reviewing.
    pub overlaps: Vec<(String, String, Vec<&'static str>)>,
    /// States that cannot be reached from the initial state.
    pub unreachable: Vec<String>,
    /// Guard node names used by more than one node type. The [`crate::machine::Memo`] key
    /// is the name, so names must be unique.
    pub duplicate_node_names: Vec<&'static str>,
}

impl Coverage {
    /// Whether the table is free of defects. `overlaps` is excluded: it is a
    /// review signal, not an error.
    pub fn is_clean(&self) -> bool {
        self.holes.is_empty() && self.unreachable.is_empty() && self.duplicate_node_names.is_empty()
    }
}

/// Checks the transition table.
pub fn coverage<D: Domain>(
    initial: D::Tag,
    edges: &'static [Edge<D>],
    ignores: &'static [Ignore<D>],
) -> Coverage {
    let mut out = Coverage::default();

    for &tag in D::all_tags() {
        for &kind in D::all_kinds() {
            let hits: Vec<&'static str> = edges
                .iter()
                .filter(|e| e.when == kind && e.from.matches(tag))
                .map(|e| e.id)
                .collect();

            if hits.is_empty() {
                if !ignores.iter().any(|i| i.matches(tag, kind)) {
                    out.holes.push((format!("{tag:?}"), format!("{kind:?}")));
                }
            } else if hits.len() > 1 {
                out.overlaps
                    .push((format!("{tag:?}"), format!("{kind:?}"), hits));
            }
        }
    }

    // Tag is only required to be Copy + Eq + Debug, not Hash, so reachability uses
    // a linear scan. State counts are small, and this avoids treating two states
    // with coincidentally equal Debug output as the same state.
    let mut reached: Vec<D::Tag> = vec![initial];
    // Iterate to a fixed point; tables are small enough that this is fine.
    loop {
        let before = reached.len();
        for e in edges {
            if let Goto::To(next) = e.goto
                && !reached.contains(&next)
                && e.from.expand().iter().any(|t| reached.contains(t))
            {
                reached.push(next);
            }
        }
        if reached.len() == before {
            break;
        }
    }
    for &tag in D::all_tags() {
        if !reached.contains(&tag) {
            out.unreachable.push(format!("{tag:?}"));
        }
    }

    // Reusing one node across many edges is normal. What must be caught is two
    // *different* node types sharing a name, since they would then share a Memo
    // entry and poison each other's cached result.
    let mut ids: Vec<(&'static str, TypeId)> = Vec::new();
    for e in edges {
        e.check.node_ids(&mut ids);
    }
    ids.sort_unstable();
    ids.dedup();
    for w in ids.windows(2) {
        if w[0].0 == w[1].0 && !out.duplicate_node_names.contains(&w[0].0) {
            out.duplicate_node_names.push(w[0].0);
        }
    }

    out
}

/// Builds a mermaid `stateDiagram-v2` string.
///
/// [`Goto::Internal`] edges are omitted: drawing state-preserving transitions as
/// self-loops makes the diagram unreadable. Use [`internal_table`] for those.
///
/// Entry and exit actions are read from [`State::entry`] and [`State::exit`] and
/// drawn as state descriptions.
///
/// The output converts to PlantUML almost line for line; see
/// `scripts/mermaid_to_plantuml.sh`.
pub fn to_mermaid<D: Domain>(
    initial: D::Tag,
    edges: &'static [Edge<D>],
    states: &'static [State<D>],
) -> String {
    let mut s = String::from("stateDiagram-v2\n");
    let _ = writeln!(s, "    [*] --> {initial:?}");

    // Declaration order, not the order the nodes were passed in.
    for &tag in D::all_tags() {
        let Some(st) = states.iter().find(|s| s.tag == tag) else {
            continue;
        };
        let mut lines: Vec<String> = Vec::new();
        if !st.entry.is_empty() {
            lines.push(format!("entry / {}", join_actions(st.entry)));
        }
        if !st.exit.is_empty() {
            lines.push(format!("exit / {}", join_actions(st.exit)));
        }
        if !lines.is_empty() {
            // A mermaid description replaces the node's label, so it has to repeat
            // the state name.
            let _ = writeln!(s, "    {tag:?} : {tag:?}<br/>{}", lines.join("<br/>"));
        }
    }

    for e in edges {
        let Goto::To(next) = e.goto else { continue };
        let guard = e.check.render();
        let unknown = if e.unknown == OnUnknown::Allow {
            "<br/>unknown=Allow"
        } else {
            ""
        };
        let run = if e.run.is_empty() {
            String::new()
        } else {
            format!("<br/>/ {}", join_actions(e.run))
        };
        for from in e.from.expand() {
            let label = if guard.is_empty() {
                format!("{:?}", e.when)
            } else {
                format!("{:?}<br/>[{guard}]", e.when)
            };
            let _ = writeln!(s, "    {from:?} --> {next:?}: {label}{unknown}{run}");
        }
    }

    s
}

/// Tabulates the transitions that do not change state.
pub fn internal_table<D: Domain>(edges: &'static [Edge<D>]) -> String {
    let mut s =
        String::from("| state | event | guard | actions | edge id |\n|---|---|---|---|---|\n");
    for e in edges {
        if !matches!(e.goto, Goto::Internal) {
            continue;
        }
        let guard = e.check.render();
        for from in e.from.expand() {
            let _ = writeln!(
                s,
                "| `{from:?}` | `{:?}` | `{}` | {} | `{}` |",
                e.when,
                if guard.is_empty() { "—" } else { &guard },
                join_actions(e.run),
                e.id
            );
        }
    }
    s
}

/// Tabulates the deliberately unhandled combinations and their reasons.
pub fn ignore_table<D: Domain>(ignores: &'static [Ignore<D>]) -> String {
    let mut s = String::from("| state | event | reason |\n|---|---|---|\n");
    for i in ignores {
        for from in i.from.expand() {
            for kind in i.when {
                let _ = writeln!(s, "| `{from:?}` | `{kind:?}` | {} |", i.why);
            }
        }
    }
    s
}

fn join_actions<A: std::fmt::Debug>(actions: &[A]) -> String {
    actions
        .iter()
        .map(|a| format!("{a:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
