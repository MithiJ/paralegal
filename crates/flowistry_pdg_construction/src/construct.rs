
use std::{borrow::Cow, rc::Rc};

use either::Either;
use flowistry::mir::FlowistryInput;
use log::trace;
use petgraph::graph::DiGraph;
use crate::graph::Tentativeness;
use flowistry_pdg::{CallString, GlobalLocation};
use std::collections::HashSet;
// use std::collections::HashMap;

use log::{debug, log_enabled, Level};


use df::{AnalysisDomain, Results, ResultsVisitor};
use rustc_hash::FxHashMap;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_index::IndexVec;
use rustc_middle::{
    mir::{visit::Visitor, AggregateKind, Location, Place, ProjectionElem, Rvalue, Terminator, TerminatorKind},
    ty::{Instance, TyCtxt},
};
use rustc_mir_dataflow::{self as df};

use rustc_utils::cache::Cache;

use crate::{
    async_support::*,
    body_cache::{self, BodyCache, CachedBody},
    calling_convention::PlaceTranslator,
    graph::{
        push_call_string_root, DepEdge, DepGraph, DepNode, PartialGraph, SourceUse, TargetUse,
    },
    local_analysis::{CallHandling, InstructionState, LocalAnalysis},
    mutation::{ModularMutationVisitor, Mutation, Time, MutationStatus},
    utils::{manufacture_substs_for, try_resolve_function},
    CallChangeCallback,
};

/// A memoizing constructor of PDGs.
///
/// Each `(LocalDefId, GenericArgs)` pair is guaranteed to be constructed only
/// once.
pub struct MemoPdgConstructor<'tcx> {
    pub(crate) tcx: TyCtxt<'tcx>,
    pub(crate) call_change_callback: Option<Rc<dyn CallChangeCallback<'tcx> + 'tcx>>,
    pub(crate) dump_mir: bool,
    pub(crate) async_info: Rc<AsyncInfo>,
    pub(crate) pdg_cache: PdgCache<'tcx>,
    pub(crate) body_cache: Rc<body_cache::BodyCache<'tcx>>,
}

impl<'tcx> MemoPdgConstructor<'tcx> {
    /// Initialize the constructor.
    pub fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self {
            tcx,
            call_change_callback: None,
            dump_mir: false,
            async_info: AsyncInfo::make(tcx).expect("Async functions are not defined"),
            pdg_cache: Default::default(),
            body_cache: Rc::new(BodyCache::new(tcx)),
        }
    }

    /// Initialize the constructor.
    pub fn new_with_cache(tcx: TyCtxt<'tcx>, body_cache: Rc<body_cache::BodyCache<'tcx>>) -> Self {
        Self {
            tcx,
            call_change_callback: None,
            dump_mir: false,
            async_info: AsyncInfo::make(tcx).expect("Async functions are not defined"),
            pdg_cache: Default::default(),
            body_cache,
        }
    }

    /// Dump the MIR of any function that is visited.
    pub fn with_dump_mir(&mut self, dump_mir: bool) -> &mut Self {
        self.dump_mir = dump_mir;
        self
    }

    /// Register a callback to determine how to deal with function calls seen.
    /// Overwrites any previously registered callback with no warning.
    pub fn with_call_change_callback(
        &mut self,
        callback: impl CallChangeCallback<'tcx> + 'tcx,
    ) -> &mut Self {
        self.call_change_callback.replace(Rc::new(callback));
        self
    }

    /// Construct the intermediate PDG for this function. Instantiates any
    /// generic arguments as `dyn <constraints>`.
    pub fn construct_root<'a>(&'a self, function: LocalDefId) -> &'a PartialGraph<'tcx> {
        let generics = manufacture_substs_for(self.tcx, function.to_def_id())
            .map_err(|i| vec![i])
            .unwrap();
        let resolution = try_resolve_function(
            self.tcx,
            function.to_def_id(),
            self.tcx.param_env_reveal_all_normalized(function),
            generics,
        )
        .unwrap();

        self.construct_for(resolution)
            .expect("Invariant broken, entrypoint cannot have been recursive.")
    }

    /// Construct a  graph for this instance of return it from the cache.
    ///
    /// Returns `None` if this is a recursive call trying to construct the graph again.
    pub(crate) fn construct_for<'a>(
        &'a self,
        resolution: Instance<'tcx>,
    ) -> Option<&'a PartialGraph<'tcx>> {
        self.pdg_cache.get_maybe_recursive(resolution, |_| {
            let g = LocalAnalysis::new(self, resolution).construct_partial_with_tentativeness();
            trace!("Computed new for {resolution:?}");
            g.check_invariants();
            g
        })
    }

    /// Has a PDG been constructed for this instance before?
    pub fn is_in_cache(&self, resolution: PdgCacheKey<'tcx>) -> bool {
        self.pdg_cache.is_in_cache(&resolution)
    }

    /// Construct a final PDG for this function. Same as
    /// [`Self::construct_root`] this instantiates all generics as `dyn`.
    ///
    /// Additionally if this is an `async fn` or `#[async_trait]` it will inline
    /// the closure as though the function were called with `poll`.
    pub fn construct_graph(&self, function: LocalDefId) -> DepGraph<'tcx> {
        if let Some((generator, loc, _ty)) = determine_async(
            self.tcx,
            function.to_def_id(),
            self.body_cache.get(function.to_def_id()).body(),
        ) {
            // TODO remap arguments

            // Note that this deliberately register this result in a separate
            // cache. This is because when this async fn is called somewhere we
            // don't want to use this "fake inlined" version.
            return push_call_string_root(
                self.construct_root(generator.def_id().expect_local()),
                GlobalLocation {
                    function: function.to_def_id(),
                    location: flowistry_pdg::RichLocation::Location(loc),
                },
            )
            .to_petgraph();
        }
        self.construct_root(function).to_petgraph()
    }

    /// Try to retrieve or load a body for this id.
    ///
    /// Returns `None` if the loading policy forbids loading from this crate.
    pub fn body_for_def_id(&self, key: DefId) -> &'tcx CachedBody<'tcx> {
        self.body_cache.get(key)
    }

    /// Access to the underlying body cache.
    pub fn body_cache(&self) -> &Rc<BodyCache<'tcx>> {
        &self.body_cache
    }

    /// Used for testing.
    pub fn take_call_changes_policy(&mut self) -> Option<Rc<dyn CallChangeCallback<'tcx> + 'tcx>> {
        self.call_change_callback.take()
    }
}

type LocalAnalysisResults<'tcx, 'mir> = Results<'tcx, &'mir LocalAnalysis<'tcx, 'mir>>;

impl<'mir, 'tcx> ResultsVisitor<'mir, 'tcx, LocalAnalysisResults<'tcx, 'mir>>
    for PartialGraph<'tcx>
{
    type FlowState = <&'mir LocalAnalysis<'tcx, 'mir> as AnalysisDomain<'tcx>>::Domain;

    fn visit_statement_before_primary_effect(
        &mut self,
        results: &LocalAnalysisResults<'tcx, 'mir>,
        state: &Self::FlowState,
        statement: &'mir rustc_middle::mir::Statement<'tcx>,
        location: Location,
    ) {
        let mut vis = self.modular_mutation_visitor(results, state);
        // TODOM: at this point, check when last modiifed

        vis.visit_statement(statement, location)
    }

    /// We handle terminators during graph construction generally in the before
    /// state, because we're interested in what the dependencies of our read
    /// places are before the modification pass overwrites the read places from
    /// any mutable arguments.
    ///
    /// There is one exception which is that non-inlined function calls are
    /// handled in two steps. Before the primary effects we generate edges from
    /// the dependencies to the input arguments. After the primary effect we
    /// insert edges from each argument to each modified location. It is cleaner
    /// to do this afterwards, because the logic that resolves a place to a
    /// graph node assumes that you are reading all of your inputs from the
    /// "last_modification". In the "before" state that map contains the
    /// "original" dependencies of each argument, e.g. we haven't combined them
    /// with the reachable places yet. So this ordering means we can reuse the
    /// same logic but just have to run it twice for every non-inlined function
    /// call site.
    fn visit_terminator_before_primary_effect(
        &mut self,
        results: &LocalAnalysisResults<'tcx, 'mir>,
        state: &Self::FlowState,
        terminator: &'mir rustc_middle::mir::Terminator<'tcx>,
        location: Location,
    ) {
        if let TerminatorKind::SwitchInt { discr, .. } = &terminator.kind {
            debug!("Hitting a case of definitely mutated for synthetic assignment at terminator");
            // TODOM: definitely mutated - synthetic assignment
            //TODOM shouldn't this be a tentative edge? This is just the conditional in the switch int being assigned a new spot
            if let Some(place) = discr.place() {
                self.register_mutation_with_tentativeness(
                    results,
                    state,
                    Inputs::Unresolved {
                        places: vec![(place, None)],
                    },
                    Either::Left(place),
                    location,
                    TargetUse::Assign,
                    Tentativeness::Certain 
                );
            }
            return;
        }

        if self.handle_as_inline(results, state, terminator, location) {
            return;
        }
        trace!("Handling terminator {:?} as not inlined", terminator.kind);
        let mut arg_vis =
            ModularMutationVisitor::new(&results.analysis.place_info, move |location, mutation| {
                // let tentativeness = match mutation.status {
                //     MutationStatus::Possibly => {
                //         debug!("possibly mutation case");
                //         Tentativeness::ControlFlowInduced
                //     },
                //     MutationStatus::Definitely => {
                //         debug!("definitely mutation case");
                //         Tentativeness::Certain
                //     }
                // };

                self.register_mutation_with_tentativeness(
                    results,
                    state,
                    Inputs::Unresolved {
                        places: mutation.inputs,
                    },
                    Either::Left(mutation.mutated),
                    location,
                    mutation.mutation_reason,
                    Tentativeness::Certain
                )
            });
        arg_vis.set_time(Time::Before);
        arg_vis.visit_terminator(terminator, location);
    }

    fn visit_terminator_after_primary_effect(
        &mut self,
        results: &LocalAnalysisResults<'tcx, 'mir>,
        state: &Self::FlowState,
        terminator: &'mir rustc_middle::mir::Terminator<'tcx>,
        location: Location,
    ) {
        if let TerminatorKind::Call { func, args, .. } = &terminator.kind {
            let constructor = results.analysis;

            if matches!(
                constructor.determine_call_handling(
                    location,
                    Cow::Borrowed(func),
                    Cow::Borrowed(args),
                    terminator.source_info.span
                ),
                Some(CallHandling::Ready { .. })
            ) {
                return;
            }
        }

        trace!("Handling terminator {:?} as not inlined", terminator.kind);
        // Read mut status from mutation
        let mut arg_vis =
            ModularMutationVisitor::new(&results.analysis.place_info, move |location, mutation| {
                debug!("{:?}",location);
                // let tentativeness = match mutation.status {
                //     MutationStatus::Possibly => {
                //         debug!("possibly mutation case");
                //         Tentativeness::ControlFlowInduced
                //     },
                //     MutationStatus::Definitely => {
                //         debug!("definitely mutation case");
                //         Tentativeness::Certain
                //     }
                // };

                self.register_mutation_with_tentativeness(
                    results,
                    state,
                    Inputs::Unresolved {
                        places: mutation.inputs,
                    },
                    Either::Left(mutation.mutated),
                    location,
                    mutation.mutation_reason,
                    Tentativeness::Certain
                )
            });
        arg_vis.set_time(Time::After);
        arg_vis.visit_terminator(terminator, location);
    }
}

impl<'tcx> PartialGraph<'tcx> {
    fn modular_mutation_visitor<'a, 'mir>(
        &'a mut self,
        results: &'a LocalAnalysisResults<'tcx, 'mir>,
        state: &'a InstructionState<'tcx>,

    ) -> ModularMutationVisitor<'a, 'tcx, impl FnMut(Location, Mutation<'tcx>) + 'a> {
        /// TODOM: here also pass in the mutation status and depEdge. What are 
        /// the other places register_mutation is called and it should be 
        /// evident where the information comes from!
        ModularMutationVisitor::new(&results.analysis.place_info, move |location, mutation| {
            debug!("{:?}",location);
            // let tentativeness = match mutation.status {
            //     MutationStatus::Possibly => {
            //         debug!("possibly mutation case");
            //         Tentativeness::ControlFlowInduced
            //     },
            //     MutationStatus::Definitely => {
            //         debug!("definitely mutation case");
            //         Tentativeness::Certain
            //     }
            // };

            self.register_mutation_with_tentativeness(
                results,
                state,
                Inputs::Unresolved {
                    places: mutation.inputs,
                },
                Either::Left(mutation.mutated),
                location,
                mutation.mutation_reason,
                Tentativeness::Certain
            )
        })
    }

    /// returns whether we were able to successfully handle this as inline
    fn handle_as_inline<'a>(
        &mut self,
        results: &LocalAnalysisResults<'tcx, 'a>,
        state: &'a InstructionState<'tcx>,
        terminator: &Terminator<'tcx>,
        location: Location,
    ) -> bool {
        let TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } = &terminator.kind
        else {
            return false;
        };
        let constructor = &results.analysis;
        let gloc = GlobalLocation {
            location: location.into(),
            function: constructor.def_id,
        };

        let Some(handling) = constructor.determine_call_handling(
            location,
            Cow::Borrowed(func),
            Cow::Borrowed(args),
            terminator.source_info.span,
        ) else {
            return false;
        };

        let (child_descriptor, calling_convention, precise) = match handling {
            CallHandling::Ready {
                calling_convention,
                descriptor,
                precise,
            } => (descriptor, calling_convention, precise),
            CallHandling::ApproxAsyncFn => {
                // Register a synthetic assignment of `future = (arg0, arg1, ...)`.
                let rvalue = Rvalue::Aggregate(
                    Box::new(AggregateKind::Tuple),
                    IndexVec::from_iter(args.iter().cloned()),
                );
                debug!("We are registering a synthetic assignment here. Perhaps also narrowing? #1");
                self.modular_mutation_visitor(results, state).visit_assign(
                    destination,
                    &rvalue,
                    location,
                );
                return false;
            }
            CallHandling::ApproxAsyncSM(how) => {
                how(
                    constructor,
                    &mut self.modular_mutation_visitor(results, state),
                    args,
                    *destination,
                    location,
                );
                return false;
            }
        };

        let child_graph = push_call_string_root(child_descriptor, gloc);

        trace!("Child graph has generics {:?}", child_descriptor.generics);

        let is_root = |n: CallString| n.len() == 2;

        let translator = PlaceTranslator::new(
            constructor.async_info(),
            constructor.def_id,
            &constructor.mono_body,
            constructor.tcx(),
            *destination,
            &calling_convention,
            precise,
        );

        // For each source node CHILD that is parentable to PLACE,
        // add an edge from PLACE -> CHILD. TODOM
        // For a struct arg it must connect all the struct fields that are seprate places
        // YOu can always have tentativeness in indirection
        // aliases- reachable places : 
        // Can you have 2 aliases that are both modified mutable borrow
        trace!("PARENT -> CHILD EDGES:");
        for (child_src, _kind) in child_graph.parentable_srcs(is_root) {
            if let Some(translation) = translator.translate_to_parent(child_src.place) {
                debug!("TODOM: traslating to parent");
                self.register_mutation_with_tentativeness(
                    results,
                    state,
                    Inputs::Unresolved {
                        places: vec![(translation, None)],
                    },
                    Either::Right(child_src),
                    location,
                    TargetUse::Assign,
                    // Tentativeness::FunctionNotAnalyzed
                    Tentativeness::Certain
                );
            }
        }

        // For each destination node CHILD that is parentable to PLACE,
        // add an edge from CHILD -> PLACE.
        //
        // PRECISION TODO: for a given child place, we only want to connect
        // the *last* nodes in the child function to the parent, not *all* of them.
        // Also tentative
        trace!("CHILD -> PARENT EDGES:");
        for (child_dst, kind) in child_graph.parentable_dsts(is_root) {
            if let Some(parent_place) = translator.translate_to_parent(child_dst.place) {
                self.register_mutation_with_tentativeness(
                    results,
                    state,
                    Inputs::Resolved {
                        node: child_dst,
                        node_use: SourceUse::Operand,
                    },
                    Either::Left(parent_place),
                    location,
                    kind.map_or(TargetUse::Return, TargetUse::MutArg),
                    // Tentativeness::FunctionNotAnalyzed
                    Tentativeness::Certain
                );
            }
        }
        self.nodes.extend(child_graph.nodes);
        self.edges.extend(child_graph.edges);
        true
    }
}

impl<'tcx> PartialGraph<'tcx> {
    /// This inserts most of the edges! Maybe it's better to find 
    /// Local analysis- for each place, what was the last time it was modified?
    /// Instruction State
    /// Construct.rs - spans the whole call tree in partial graph
    fn register_mutation_with_tentativeness(
        &mut self,
        results: &LocalAnalysisResults<'tcx, '_>,
        state: &InstructionState<'tcx>,
        inputs: Inputs<'tcx>,
        mutated: Either<Place<'tcx>, DepNode<'tcx>>,
        location: Location,
        target_use: TargetUse,
        tentativeness: Tentativeness
    ) {
        debug!("Registering mutation to {mutated:?} with inputs {inputs:?} at {location:?}");
        let constructor = &results.analysis;
        let ctrl_inputs = constructor.find_control_inputs_with_tentativeness(location);

        debug!("  Found control inputs {ctrl_inputs:?}");

        let (input_place, data_inputs) = match inputs {
            Inputs::Unresolved { places } => {
                let input_nodes = places
                .iter()
                .flat_map(|(input, input_use)| {
                    constructor
                        .find_data_inputs(state, *input)
                        .into_iter()
                        .map(move |input| {
                            (
                                input,
                                input_use.map_or(SourceUse::Operand, SourceUse::Argument),
                            )
                        })
                })
                .collect::<Vec<_>>();
            let place = places.first().map(|(p, _)| p.clone());
            (place, input_nodes)
            },
            Inputs::Resolved { node_use, node } => {
                let place_ref = &(node.place.clone());
                (Some(node.place), vec![(node, node_use)])
                },
        };
        trace!("  Data inputs: {data_inputs:?}");
        // any different base local then tentative
        // empty projection overlaps everything else
        // S.0 and S.1 is distinct projections of the same base local
        // TODO: test case for S.0.1
        let mut seen = HashSet::new();
        for (node, _) in data_inputs.iter() {
            if seen.len() == 0 {
                seen.insert(node);
            } else if !seen.iter().all(|&x| places_distinct(x.place, node.place)) {
                // only insert non overlapping nodes
                //technically also have to remove x from set()
                seen.insert(node);
            }
            // match input_place {
            //     Some(input) => {
            //         // 15.1, 15.2 -> 15 -> overlapping
            //         // 15.1 vs 15.2 -> DISTINCT
            //         if input.local.as_u32() == node.place.local.as_u32() {
            //             // When they have the same base local
            //             // we can check for fields recombined into a struct
            //             // when flowing into an un-analyzed function
            //             match input.projection.first() {
            //                 Some(pelt) => {unique_inputs += 1},
            //                 None => {
            //                     if !node.place.projection.first()
            //                     .is_some_and(|proj| 
            //                         matches!(proj, ProjectionElem::Field(..))) {
            //                             unique_inputs += 1
            //                         }
            //                 }
            //             }
            //         } else {
            //             // If they don't have the same base local, cannot be proven distinct
            //             // Thus, can be marked tentative.
            //             // _16 vs _15 can be DISTINCt or not.
            //             unique_inputs += 1
            //         }
            // },
            //     None => {unique_inputs += 1},
            // }
        }
        debug!("{:?}", seen);

        let outputs = match mutated {
            Either::Right(node) => vec![node],
            Either::Left(place) => results
                .analysis
                .find_outputs(place, location)
                .into_iter()
                .map(|t| t.1)
                .collect(),
        };
        trace!("  Outputs: {outputs:?}");

        for output in &outputs {
            trace!("  Adding node {output}");
            self.nodes.insert(*output);
        }
        let num_inputs = seen.len();
        // debug!("{unique_inputs} is how many edges we have");
        for (data_input, source_use) in data_inputs {
            // debug!("At location{loc} seen: {seen_ct} \n");
            let mut tent = Tentativeness::Certain;
            if num_inputs > 1 { 
                tent = Tentativeness::ControlFlowInduced;
            }

            let data_edge = DepEdge::data(
                constructor.make_call_string(location),
                source_use,
                target_use,
                tent
            );
            for output in &outputs {
                trace!("  Adding edge {data_input} -> {output}");
                self.edges.insert((data_input, *output, data_edge));
            }
        }

        // Add control dependencies: ctrl_input -> output
        for (ctrl_input, edge) in &ctrl_inputs {
            for output in &outputs {
                self.edges.insert((*ctrl_input, *output, *edge));
            }
        }
    }

    fn register_mutation(
        &mut self,
        results: &LocalAnalysisResults<'tcx, '_>,
        state: &InstructionState<'tcx>,
        inputs: Inputs<'tcx>,
        mutated: Either<Place<'tcx>, DepNode<'tcx>>,
        location: Location,
        target_use: TargetUse,
    ) {
        trace!("Registering mutation to {mutated:?} with inputs {inputs:?} at {location:?}");
        let constructor = &results.analysis;
        let ctrl_inputs = constructor.find_control_inputs_with_tentativeness(location);

        trace!("  Found control inputs {ctrl_inputs:?}");

        let data_inputs = match inputs {
            Inputs::Unresolved { places } => places
                .into_iter()
                .flat_map(|(input, input_use)| {
                    let inputs = constructor
                        .find_data_inputs(state, input);
                    inputs.into_iter()
                        .map(move |input| {
                            (
                                input,
                                input_use.map_or(SourceUse::Operand, SourceUse::Argument),
                            )
                        })
                })
                .collect::<Vec<_>>(),
            Inputs::Resolved { node_use, node } => vec![(node, node_use)],
        };
        trace!("  Data inputs: {data_inputs:?}");

        let outputs = match mutated {
            Either::Right(node) => vec![node],
            Either::Left(place) => results
                .analysis
                .find_outputs(place, location)
                .into_iter()
                .map(|t| t.1)
                .collect(),
        };
        trace!("  Outputs: {outputs:?}");

        for output in &outputs {
            trace!("  Adding node {output}");
            self.nodes.insert(*output);
        }

        // Add data dependencies: data_input -> output
        for (data_input, source_use) in data_inputs {
            let data_edge = DepEdge::data(
                constructor.make_call_string(location),
                source_use,
                target_use,
                Tentativeness::Certain
                // TODO: this is a placeholder to make the original code
                // run with the new tentativeness field within the DepEdge struct
            );
            for output in &outputs {
                trace!("  Adding edge {data_input} -> {output}");
                self.edges.insert((data_input, *output, data_edge));
            }
        }

        // Add control dependencies: ctrl_input -> output
        for (ctrl_input, edge) in &ctrl_inputs {
            for output in &outputs {
                self.edges.insert((*ctrl_input, *output, *edge));
            }
        }
    }
}

/// How we are indexing into [`PdgCache`]
pub type PdgCacheKey<'tcx> = Instance<'tcx>;
/// Stores PDG's we have already computed and which we know we can use again
/// given a certain key.
pub type PdgCache<'tcx> = Rc<Cache<PdgCacheKey<'tcx>, PartialGraph<'tcx>>>;

#[derive(Debug)]
enum Inputs<'tcx> {
    Unresolved {
        places: Vec<(Place<'tcx>, Option<u8>)>,
    },
    Resolved {
        node: DepNode<'tcx>,
        node_use: SourceUse,
    },
}

impl<'tcx> PartialGraph<'tcx> {
    pub fn to_petgraph(&self) -> DepGraph<'tcx> {
        let domain = self;
        let mut graph: DiGraph<DepNode<'tcx>, DepEdge> = DiGraph::new();
        let mut nodes = FxHashMap::default();
        macro_rules! add_node {
            ($n:expr) => {
                *nodes.entry($n).or_insert_with(|| graph.add_node($n))
            };
        }

        for node in &domain.nodes {
            let _ = add_node!(*node);
        }

        for (src, dst, kind) in &domain.edges {
            let src_idx = add_node!(*src);
            let dst_idx = add_node!(*dst);
            graph.add_edge(src_idx, dst_idx, *kind);
        }

        DepGraph::new(graph)
    }

    fn check_invariants(&self) {
        let root_function = self.nodes.iter().next().unwrap().at.root().function;
        for n in &self.nodes {
            assert_eq!(n.at.root().function, root_function);
        }
        for (_, _, e) in &self.edges {
            assert_eq!(e.at.root().function, root_function);
        }
    }
}

/// Defines whether places are distinct or overlapping for purposes of 
    /// tentativeness. If 2 distinct places flow to a single node, the edges 
    /// associated would be certain. However, if 2 overlapping nodes flow to
    /// a single node, the edges associated would be tentative (because the 
    /// nodes are distinct and multiple).
    /// 
    /// Places are said to be distinct iff they are projections of the same
    /// struct. First, they must have the same base local. Then, their sequence
    /// of projections must be the same except for the last projection element
    /// in the list. For example-
    /// 1) "15.0" vs "15.1" are distinct because they are distinct fields of the 
    /// _15 struct
    /// 2) "15" vs "15.0" are overlapping because the former refers to the 
    /// struct and the latter refers to the field. Similarly "15.0" and "15.0.1"
    /// are also overlapping/non-distinct. Here, we use the list of projection 
    /// elements to figure out if one is the parent struct of the other or if 
    /// both are simply fields
    /// 3) "16" vs "15" are non-distinct because they cannot be proven to be 
    /// distinct places (there may be aliasing involved)
    pub fn places_distinct<'tcx>(p1: Place<'tcx>, p2: Place<'tcx>) -> bool {
        // _16 vs _15
        if p1.local != p2.local {
            return false
        }
        // 15.0 vs 15.0.1 OR 15.1 vs 15.0.1
        // if p1.projection.len() != p2.projection.len() {
        //     return false
        // }
        // 15.0 vs 15.1 OR 15.0.3 vs 15.1.0
        let length = p1.projection.len().min(p2.projection.len());
        return p1.projection.iter().take(length - 1).eq(p2.projection.iter().take(length - 1))
    }