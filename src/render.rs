//! Artifacts derived from a controller's declaration: mermaid diagrams, tables,
//! and exhaustive gap reports.
//!
//! The [`crate::machine`] functions read a transition table; the [`crate::feature`]
//! ones read a list of [`FeatureInfo`].

use std::any::TypeId;
use std::fmt::Write as _;

use crate::feature::FeatureInfo;
use crate::machine::{Edge, Goto, Ignore, OnUnknown, State};
use crate::{Domain, MachineSpec};

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

/// Checks a transition table for gaps, unreachable states, and guard name
/// collisions.
///
/// - `initial`: the machine's starting state.
/// - `edges`, `ignores`: the transition table to check.
///
/// Returns a [`Coverage`] report.
pub fn coverage<M: MachineSpec>(
    initial: M::Tag,
    edges: &'static [Edge<M>],
    ignores: &'static [Ignore<M>],
) -> Coverage {
    let mut out = Coverage::default();

    for &tag in M::all_tags() {
        for &kind in <M::Domain as Domain>::all_kinds() {
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
    let mut reached: Vec<M::Tag> = vec![initial];
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
    for &tag in M::all_tags() {
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

/// The event kinds a transition table acts on. `Ignore`d kinds do not count —
/// pass this to [`crate::feature::unhandled_kinds`] alongside a feature list so
/// a controller mixing both layers is checked as one unit.
///
/// - `edges`: the transition table to scan.
///
/// Returns the event kinds matched by at least one edge.
pub fn handled_kinds<M: MachineSpec>(
    edges: &'static [Edge<M>],
) -> Vec<<M::Domain as Domain>::EventKind> {
    let mut out: Vec<_> = Vec::new();
    for e in edges {
        if !out.contains(&e.when) {
            out.push(e.when);
        }
    }
    out
}

/// Builds a mermaid `stateDiagram-v2` diagram from a transition table.
///
/// - `initial`: the machine's starting state.
/// - `edges`: the transitions to draw. [`Goto::Internal`] edges are omitted
///   (they'd draw as unreadable self-loops) — use [`internal_table`] for those.
/// - `states`: entry/exit actions, drawn as state descriptions.
///
/// Returns the diagram source. Converts to PlantUML almost line for line;
/// see `scripts/mermaid_to_plantuml.sh`.
pub fn to_mermaid<M: MachineSpec>(
    initial: M::Tag,
    edges: &'static [Edge<M>],
    states: &'static [State<M>],
) -> String {
    let mut s = String::from("stateDiagram-v2\n");
    let _ = writeln!(s, "    [*] --> {initial:?}");

    // Declaration order, not the order the nodes were passed in.
    for &tag in M::all_tags() {
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

/// Tabulates the transitions that do not change state ([`Goto::Internal`]).
///
/// - `edges`: the transition table to scan.
///
/// Returns a markdown table.
pub fn internal_table<M: MachineSpec>(edges: &'static [Edge<M>]) -> String {
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
///
/// - `ignores`: the ignore list to render.
///
/// Returns a markdown table.
pub fn ignore_table<M: MachineSpec>(ignores: &'static [Ignore<M>]) -> String {
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

// ─────────────────────────────────────────── feature layer

/// Tabulates what each feature reacts to and emits.
///
/// - `features`: the feature list to render.
///
/// Returns a markdown table.
pub fn io_table<D: Domain>(features: &[FeatureInfo<D>]) -> String {
    let mut s = String::from("| feature | handles | emits |\n|---|---|---|\n");
    for f in features {
        let _ = writeln!(
            s,
            "| `{}` | {} | {} |",
            f.name,
            join_or_dash(f.handles),
            join_or_dash(f.emits)
        );
    }
    s
}

/// Draws a mermaid flowchart of events → features → actions.
///
/// - `features`: the feature list to render.
///
/// Returns the diagram source. Node ids carry a prefix because a feature and
/// the action it emits routinely share a name, which would otherwise merge
/// them into one self-looping node.
pub fn io_flowchart<D: Domain>(features: &[FeatureInfo<D>]) -> String {
    let mut s = String::from("flowchart LR\n");
    for f in features {
        for k in f.handles {
            let _ = writeln!(s, "    ev_{k:?}[\"{k:?}\"] --> ft_{0}[\"{0}\"]", f.name);
        }
        for a in f.emits {
            let _ = writeln!(s, "    ft_{0}[\"{0}\"] --> ac_{a:?}[\"{a:?}\"]", f.name);
        }
    }
    s
}

fn join_or_dash<T: std::fmt::Debug>(items: &[T]) -> String {
    if items.is_empty() {
        return "—".into();
    }
    items
        .iter()
        .map(|i| format!("`{i:?}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
