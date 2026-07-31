//! 같은 표에서 산출물을 뽑는다 — mermaid 다이어그램과 전수 커버리지 매트릭스.

use std::any::TypeId;
use std::fmt::Write as _;

use super::{Domain, Edge, Goto, Ignore, OnUnknown};

/// `(상태 × 이벤트)` 전수 검사 결과.
#[derive(Debug, Default)]
pub struct Coverage {
    /// 엣지도 `ignore`도 없는 조합. **CI에서 0이어야 한다.**
    pub holes: Vec<(String, String)>,
    /// 같은 조합에 엣지가 2개 이상인 경우. 선언 순서가 우선순위이므로
    /// 반드시 오류는 아니지만 리뷰 대상이다.
    pub overlaps: Vec<(String, String, Vec<&'static str>)>,
    /// 도달 불가 상태.
    pub unreachable: Vec<String>,
    /// 이름이 겹치는 조건 노드. Memo 키가 이름이므로 유일해야 한다.
    pub duplicate_node_names: Vec<&'static str>,
}

impl Coverage {
    pub fn is_clean(&self) -> bool {
        self.holes.is_empty()
            && self.unreachable.is_empty()
            && self.duplicate_node_names.is_empty()
    }
}

/// 표를 검사한다.
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

    // Tag는 Hash를 요구하지 않으므로(Copy + Eq + Debug만 요구) 문자열 대신
    // 선형 탐색으로 비교한다 — 상태 개수가 작아 부담이 없고, Debug 출력이
    // 우연히 겹치는 두 상태를 같은 상태로 오판하는 일도 없앤다.
    let mut reached: Vec<D::Tag> = vec![initial];
    // 고정점까지 반복 — 엣지 수가 작아 단순 반복으로 충분하다.
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

    // 같은 노드를 여러 엣지에서 쓰는 건 정상이다. 잡아야 하는 것은
    // **이름은 같은데 타입이 다른** 노드다 — Memo 키가 이름이므로 서로의 캐시를 오염시킨다.
    let mut ids: Vec<(&'static str, TypeId)> = Vec::new();
    for e in edges {
        e.check.node_ids(&mut ids);
    }
    ids.sort_unstable_by_key(|(n, _)| *n);
    ids.dedup();
    for w in ids.windows(2) {
        if w[0].0 == w[1].0 && !out.duplicate_node_names.contains(&w[0].0) {
            out.duplicate_node_names.push(w[0].0);
        }
    }

    out
}

/// mermaid `stateDiagram-v2` 문자열을 만든다.
///
/// `Goto::Internal` 엣지는 기본적으로 생략한다 — 상태를 바꾸지 않는 전이가
/// self-loop로 그려지면 다이어그램을 읽을 수 없게 된다. 대신 [`internal_table`]로 뽑는다.
pub fn to_mermaid<D: Domain>(initial: D::Tag, edges: &'static [Edge<D>]) -> String {
    let mut s = String::from("stateDiagram-v2\n");
    let _ = writeln!(s, "    [*] --> {initial:?}");

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

/// 상태를 바꾸지 않는 전이를 표로 뽑는다.
pub fn internal_table<D: Domain>(edges: &'static [Edge<D>]) -> String {
    let mut s = String::from("| 상태 | 이벤트 | 조건 | 액션 | edge id |\n|---|---|---|---|---|\n");
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

fn join_actions<A: std::fmt::Debug>(actions: &[A]) -> String {
    actions
        .iter()
        .map(|a| format!("{a:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
