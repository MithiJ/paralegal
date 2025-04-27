//! Utilities for traversing an SPDG

use std::collections::HashSet;
use log::debug;

use petgraph::visit::{Control, Data, DfsEvent, EdgeFiltered, EdgeRef, IntoEdgeReferences, IntoNeighbors};

use crate::{EdgeInfo, EdgeKind, Node};

use super::SPDG;

/// Which type of edges should be considered for a given traversal
#[derive(Clone, Copy, Eq, PartialEq, strum::EnumIs)]
pub enum EdgeSelection {
    /// Consider only edges with [`crate::EdgeKind::Data`]
    Data,
    /// Consider only edges with [`crate::EdgeKind::Control`]
    Control,
    /// Consider both data and control flow edges in any combination
    Both,
}

impl EdgeSelection {
    /// Does this selection admit edges of type [`crate::EdgeKind::Control`]
    pub fn use_control(self) -> bool {
        matches!(self, EdgeSelection::Control | EdgeSelection::Both)
    }
    /// Does this selection admit edges of type [`crate::EdgeKind::Data`]
    pub fn use_data(self) -> bool {
        matches!(self, EdgeSelection::Data | EdgeSelection::Both)
    }

    /// Is this edge kind admissible?
    pub fn conforms(self, kind: EdgeKind) -> bool {
        matches!(
            (self, kind),
            (EdgeSelection::Both, _)
                | (EdgeSelection::Data, EdgeKind::Data)
                | (EdgeSelection::Control, EdgeKind::Control)
        )
    }

    /// Create a graph adaptor that implements this edge selection.
    /// TODOM: Come back here for traversal
    pub fn filter_graph<G: IntoEdgeReferences + Data<EdgeWeight = EdgeInfo>>(
        self,
        g: G,
    ) -> EdgeFiltered<G, fn(G::EdgeRef) -> bool> {
        fn data_only<E: EdgeRef<Weight = EdgeInfo>>(e: E) -> bool {
            e.weight().is_data()
        }
        fn control_only<E: EdgeRef<Weight = EdgeInfo>>(e: E) -> bool {
            e.weight().is_control()
        }
        fn all_edges<E: EdgeRef<Weight = EdgeInfo>>(_: E) -> bool {
            true
        }

        match self {
            EdgeSelection::Data => EdgeFiltered(g, data_only as fn(G::EdgeRef) -> bool),
            EdgeSelection::Control => EdgeFiltered(g, control_only as fn(G::EdgeRef) -> bool),
            EdgeSelection::Both => EdgeFiltered(g, all_edges as fn(G::EdgeRef) -> bool),
        }
    }
}

/// A primitive that queries whether we can reach from one set of nodes to
/// another
pub fn generic_flows_to(
    from: impl IntoIterator<Item = Node>,
    edge_selection: EdgeSelection,
    spdg: &SPDG,
    other: impl IntoIterator<Item = Node>,
) -> bool {
    let targets = other.into_iter().collect::<HashSet<_>>();
    let mut from = from.into_iter().peekable();
    if from.peek().is_none() || targets.is_empty() {
        return false;
    }

    let graph = edge_selection.filter_graph(&spdg.graph);

    let result = petgraph::visit::depth_first_search(&graph, from, |event| match event {
        DfsEvent::Discover(d, _) if targets.contains(&d) => Control::Break(()),
        // or can we ensure for discover that it somehow checks its parent node, checks tentativeness, all children satisfying etc. 
        // for a tentative tree edge, check that all children of source node have been discovered and then break
        _ => Control::Continue,
    });
    matches!(result, Control::Break(()))
}



// pub fn individual_depends_on(
//     node: Node,
//     depends_on: Node,
//     spdg: &SPDG,
//     // edge selection is always DATA
//     edge_selection: EdgeSelection,
// ) {
//     // let graph = edge_selection.filter_graph(&spdg.graph);
//     let graph = &spdg.graph.edges_directed(node, petgraph::Direction::Incoming)
// }

pub fn always_depends(
    start_node: Node,
    edge_selection: EdgeSelection, // always DATA only
    spdg: &SPDG,
    target_node: Node, 
) -> bool {
    if edge_selection != EdgeSelection::Data {
        return false // we only handle DATA. 
        // For control or for data and control, use generic_flows_to
    }
    let mut discovered = HashSet::new();
    discovered.insert(start_node);
    // TODOM: modify: if tentative edge then do for all
    let mut tentative = false; // assume definite edges 
    let mut results = Vec::new();
    if start_node == target_node {
        debug!("DFS: base case where start: {:?} and end : {:?}", start_node, target_node);
        return true;
    }
    // check if target is start or if target is immediate parent
    for edge in spdg.graph.edges_directed(start_node, petgraph::Direction::Incoming) {
        debug!{"DFS: Incoming edge from {:?} to {:?}", &edge.source(), start_node}
        let edge_info = edge.weight();
        if edge_info.is_control() {
            debug!{"control nvm"}
            continue
        }
        if edge.target() != start_node {
            debug!("DFS: edge : {:?}->{:?} vs {:?}", edge.source(), edge.target(), start_node);
        }
        // for all incoming data edges
        if discovered.contains(&edge.source()) {
            continue
        }
        if edge_info.is_tentative() {
            // debug!{"DFS: tentative edge"}
            tentative = true;
        }
        // that haven't been visited
        let res =  dfs_helper(spdg, edge.source(), target_node, & mut discovered);
        if !tentative && res{
            return res;
        }
        results.push(res);
    };
    // Q: how far up should you go?
    //TODOM: any to all for tentative case
    if tentative {
        return results.iter().any(|&x| x);
    } else {
        return results.iter().any(|&x| x);
    }
}

fn dfs_helper(
    spdg:&SPDG,
    node: Node,
    target: Node,
    discovered: &mut HashSet<Node>,
) -> bool {
    discovered.insert(node);
    let mut tentative = false; // assume definite edges 
    let mut results = Vec::new();
    if node == target {
        return true;
    }
    for edge in spdg.graph.edges_directed(node, petgraph::Direction::Incoming)  {
        let edge_info = edge.weight();
        if edge_info.is_control() {
            continue
        }
        // if edge.target() != node {
        //     debug!("edge : {:?}->{:?} vs {:?}", edge.source(), edge.target(), node);
        // }
        // for all incoming data edges
        if discovered.contains(&edge.source()) {
            continue
        }
        if edge_info.is_tentative() {
            tentative = true;
        }
        // Here, if there are 2 tentative incoming edges - from Node A and B-
        // both A and B must individually be connected to target. So, we need to
        // give each recursive call its own discovered list. As an optimization,
        // once a node is known to be connected to target, we can cache that 
        // result. Later if we encounter it for another recursion, we can X early
        let res = dfs_helper(spdg, edge.source(), target, &mut discovered.clone());
        if !tentative && res {
            return res
        }
        results.push(res);
    };
    //TODOM:
    if tentative {
        return results.iter().any(|&x| x);
    } else {
        return results.iter().any(|&x| x);
    }
}



// / A primitive that queries whether we can reach from one set of nodes to
// / another
// / This shows the flow when we only want to consider definite edges
// pub fn generic_flows_to(
//     from: impl IntoIterator<Item = Node>,
//     edge_selection: EdgeSelection,
//     spdg: &SPDG,
//     other: impl IntoIterator<Item = Node>,
// ) -> bool {
//     let targets = other.into_iter().collect::<HashSet<_>>();
//     let mut from = from.into_iter().peekable();
//     if from.peek().is_none() || targets.is_empty() {
//         return false;
//     }

//     let graph = edge_selection.filter_graph(&spdg.graph);

//     let result = petgraph::visit::depth_first_search(&graph, from, |event| match event {
//         DfsEvent::Discover(d, _) if targets.contains(&d) => Control::Break(()),
//         DfsEvent::TreeEdge(u, v) if !edge.tentativeness {
//             Control::Prune(())
//         },
//         _ => Control::Continue,
//     });
//     matches!(result, Control::Break(()))
// }
// / This is intending to copy functionality above. The reason we cannot modify
// / generic_flows_to is that
// / 
// / 1) we cannot filter graph by edge selection. We want tentative and 
// / non-tentative edges to remain in the graph. When we encounter a tentative set
// / of edges, we want to simply run flows to on every single one of the branches
// / (i.e. run "all" instead of "any"). So we must run an augmented DFS algorithm
// / 
// / 2) we cannot use the normal DFS algorithm because if there is any tentative
// / edge, all are tentative edges and we must traverse all neighbors vs if the
// / edge is definite it is sufficient to only traverse one neighbor
// / 
// / Given Node A has tentative dependencies on Node X, Y and Z, and we want to 
// / check whether Node A always depends on Node B, we check whether Nodes X, Y,
// / AND Z always depend on Node B. If Node A has a definite dependency on Node X
// / we only check if Node X always depends on Node B, if that fails, we may
// / tentatively check other dependencies.
// / 
// / For this naive version, we impose some constraints.
// / `from` can only be one source node. To check between sets is much harder.
// / We want to ensure then that .all(flows_to(s, set_of_sinks))
// / Edge Selection is always data flow and always considering tentative edges
// pub fn very_very_naive_dfs(
//     from: impl IntoIterator<Item = Node>,
//     edge_selection: EdgeSelection,
//     spdg: &SPDG,
//     other: impl IntoIterator<Item = Node>,
// ){
//     let time = &mut Time(0);
//     let discovered = &mut graph.visit_map();
//     let finished = &mut graph.visit_map();

//     for start in starts {
//         try_control!(
//             naive_dfs_visitor(graph, start, &mut visitor, discovered, finished, time),
//             unreachable!()
//         );
//     }
//     C::continuing()
// }

// pub fn dfs_visitor<G, F, C>(
//     graph: G,
//     u: G::NodeId,
//     visitor: &mut F,
//     discovered: &mut G::Map,
//     finished: &mut G::Map,
//     time: &mut Time,
// ) -> C
// where
//     G: IntoNeighbors + Visitable,
//     F: FnMut(DfsEvent<G::NodeId>) -> C,
//     C: ControlFlow,
//     {
//     if !discovered.visit(u) {
//         return C::continuing();
//     }

//     try_control!(
//         visitor(DfsEvent::Discover(u, time_post_inc(time))),
//         {},
//         for v in graph.neighbors(u) {
//             if !discovered.is_visited(&v) {
//                 // here check about edge tentativeness
//                 try_control!(visitor(DfsEvent::TreeEdge(u, v)), continue);
//                 try_control!(
//                     dfs_visitor(graph, v, visitor, discovered, finished, time),
//                     unreachable!()
//                 );
//             } else if !finished.is_visited(&v) {
//                 try_control!(visitor(DfsEvent::BackEdge(u, v)), continue);
//             } else {
//                 try_control!(visitor(DfsEvent::CrossForwardEdge(u, v)), continue);
//             }
//         }
//     );
//     let first_finish = finished.visit(u);
//     // TODOM: can we check if all children are finished and then return, where's the bool?
//     debug_assert!(first_finish);
//     try_control!(
//         visitor(DfsEvent::Finish(u, time_post_inc(time))),
//         // TODOM: is this the time of the longest path?
//         panic!("Pruning on the `DfsEvent::Finish` is not supported!")
//     );
//     C::continuing()
// }

// Without callbacks
// pub fn dfs_visitor(
//     graph: G,
//     source: G::NodeId,
//     discovered: &mut G::Map,
//     finished: &mut G::Map,
//     time: &mut Time,
// ) {

// }