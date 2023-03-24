use flowistry::{
    extensions::RecurseSelector,
    mir::{borrowck_facts, utils::BodyExt},
};

use crate::{
    ana::algebra::{Equality, Term},
    hir::def_id::LocalDefId,
    mir::{self, Field, Location, ProjectionElem},
    rust::{
        rustc_ast,
        rustc_hir::{def_id::DefId, BodyId},
        rustc_index::vec::IndexVec,
    },
    utils::{body_name_pls, outfile_pls, AsFnAndArgs, DisplayViaDebug, LocationExt},
    Either, HashMap, HashSet, Symbol, TyCtxt,
};

use std::{
    borrow::Cow,
    cell::RefCell,
    fmt::{Display, Write},
    rc::Rc,
};

newtype_index!(
    pub struct ArgumentIndex {
        DEBUG_FORMAT = "arg{}"
    }
);

impl Display for ArgumentIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a{}", self.as_usize())
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Hash, Copy)]
pub enum TargetPlace {
    Return,
    Argument(ArgumentIndex),
}

pub type TargetTerm = algebra::Term<TargetPlace, DisplayViaDebug<Field>>;

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct Dependency<L> {
    pub target: Target<L>,
    pub target_term: TargetTerm,
}

impl<L> Dependency<L> {
    pub fn map_locations<L0, F: FnMut(&L) -> L0>(&self, f: F) -> Dependency<L0> {
        Dependency {
            target: self.target.map_location(f),
            target_term: self.target_term.clone(),
        }
    }
}

impl<L: Display> Display for Dependency<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} @ {}", self.target, self.target_term)
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum Target<L> {
    Call(L),
    Argument(ArgumentIndex),
}

impl<L> Target<L> {
    pub fn map_location<L0, F: FnMut(&L) -> L0>(&self, mut f: F) -> Target<L0> {
        match self {
            Target::Argument(a) => Target::Argument(*a),
            Target::Call(l) => Target::Call(f(l)),
        }
    }
}

impl<L: Display> Display for Target<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Call(loc) => write!(f, "{loc}"),
            Target::Argument(a) => a.fmt(f),
        }
    }
}

type Dependencies<L> = HashSet<Dependency<L>>;

#[derive(Debug)]
pub struct Call<D> {
    pub function: DefId,
    pub arguments: IndexVec<ArgumentIndex, D>,
}

impl<L> Call<Dependencies<L>> {
    pub fn map_locations<L0: std::hash::Hash + Eq, F: FnMut(&L) -> L0>(
        &self,
        mut f: F,
    ) -> Call<Dependencies<L0>> {
        let arguments = self
            .arguments
            .iter()
            .map(|set| set.iter().map(|d| d.map_locations(&mut f)).collect())
            .collect();
        Call {
            function: self.function,
            arguments,
        }
    }
    pub fn flat_map_dependencies<
        L0: Eq + std::hash::Hash,
        F: FnMut(&Dependency<L>) -> I,
        I: Iterator<Item = Dependency<L0>>,
    >(
        &self,
        mut f: F,
    ) -> Call<Dependencies<L0>> {
        Call {
            function: self.function,
            arguments: self
                .arguments
                .iter()
                .map(|a| a.iter().flat_map(&mut f).collect())
                .collect(),
        }
    }

    pub fn expand_arguments<
        F: FnMut(ArgumentIndex, &TargetTerm) -> I,
        I: Iterator<Item = Dependency<L>>,
    >(
        &self,
        mut f: F,
    ) -> Self
    where
        L: Eq + std::hash::Hash + Clone,
    {
        self.flat_map_dependencies(|d| {
            if let Target::Argument(u) = d.target {
                Either::Right(f(u, &d.target_term))
            } else {
                let c: Dependency<L> = d.clone();
                Either::Left(std::iter::once(c))
            }
            .into_iter()
        })
    }
}

fn fmt_deps<L: Display>(
    deps: &Dependencies<L>,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    f.write_char('{')?;
    let mut first_dep = true;
    for dep in deps {
        if first_dep {
            first_dep = false;
        } else {
            f.write_str(", ")?;
        }
        write!(f, "{dep}")?;
    }
    f.write_char('}')
}

impl<L: Display> Display for Call<Dependencies<L>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char('(')?;
        let mut first = true;
        for arg in self.arguments.iter() {
            if first {
                first = false;
            } else {
                f.write_str(", ")?;
            }
            fmt_deps(arg, f)?;
        }
        write!(f, ")   {:?}", self.function)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Body<L> {
    pub calls: HashMap<L, Call<Dependencies<L>>>,
    pub return_deps: Dependencies<L>,
    pub return_arg_deps: Vec<Dependencies<L>>,
}

impl<L> Body<L> {
    pub fn calls(&self) -> &HashMap<L, Call<Dependencies<L>>> {
        &self.calls
    }

    pub fn at(&self, l: &L) -> Option<&Call<Dependencies<L>>>
    where
        L: Eq + std::hash::Hash,
    {
        self.calls.get(l)
    }
}

impl<L: Display + Ord> Display for Body<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ordered = self.calls.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|t| t.0);
        for (loc, call) in ordered {
            writeln!(f, "{:<6}: {}", format!("{}", loc), call)?
        }
        write!(f, "return: ")?;
        fmt_deps(&self.return_deps, f)?;
        writeln!(f)?;
        write!(f, "return args: (")?;
        let mut first_arg = true;
        for arg in &self.return_arg_deps {
            if first_arg {
                first_arg = false;
            } else {
                f.write_str(", ")?;
            }
            fmt_deps(arg, f)?;
        }
        f.write_char(')')?;
        Ok(())
    }
}

pub struct MemoizingConstructor<'tcx, 'g> {
    tcx: TyCtxt<'tcx>,
    gli: GLI<'g>,
    memoizer: RefCell<HashMap<BodyId, Rc<Body<DisplayViaDebug<Location>>>>>,
}

struct NeverInline;

impl RecurseSelector for NeverInline {
    fn is_selected<'tcx>(&self, _tcx: TyCtxt<'tcx>, _tk: &mir::TerminatorKind<'tcx>) -> bool {
        false
    }
}

pub fn compute_from_body_id(
    body_id: BodyId,
    tcx: TyCtxt,
    gli: GLI,
) -> Body<DisplayViaDebug<Location>> {
    let hir = tcx.hir();
    let target_name = hir.name(hir.body_owner(body_id));
    let local_def_id = tcx.hir().body_owner_def_id(body_id);
    let body_with_facts = borrowck_facts::get_body_with_borrowck_facts(tcx, local_def_id);
    let body = body_with_facts.simplified_body();
    let flow = df::compute_flow_internal(tcx, gli, body_id, body_with_facts, &NeverInline);
    mir::pretty::write_mir_fn(
        tcx,
        body,
        &mut |_, _| Ok(()),
        &mut outfile_pls(&format!("{}.mir", target_name)).unwrap(),
    )
    .unwrap();
    let ref mut states_out = outfile_pls(&format!("{}.df", target_name)).unwrap();
    for l in body.all_locations() {
        writeln!(states_out, "{l:?}: {}", flow.state_at(l)).unwrap();
    }
    let place_resolver = algebra::PlaceResolver::construct(tcx, body);
    let r = Body::construct(&flow, &place_resolver);
    let mut out = outfile_pls(&format!("{}.regal", target_name)).unwrap();
    use std::io::Write;
    write!(&mut out, "{}", r).unwrap();
    r
}

impl<'tcx, 'g> MemoizingConstructor<'tcx, 'g> {
    pub fn new(tcx: TyCtxt<'tcx>, gli: GLI<'g>) -> Self {
        Self {
            tcx,
            gli,
            memoizer: Default::default(),
        }
    }

    pub fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }

    pub fn gli(&self) -> GLI<'g> {
        self.gli
    }

    pub fn get(&self, body_id: BodyId) -> Rc<Body<DisplayViaDebug<Location>>> {
        if let Some(b) = self.memoizer.borrow().get(&body_id) {
            return b.clone();
        }
        let r = compute_from_body_id(body_id, self.tcx(), self.gli());
        let rc = Rc::new(r);
        self.memoizer.borrow_mut().insert(body_id, rc.clone());
        rc
    }
}

use crate::ana::{algebra, df};

use super::GLI;

impl Body<DisplayViaDebug<Location>> {
    pub fn construct(
        flow_analysis: &df::FlowResults<'_, '_, '_>,
        place_resolver: &algebra::PlaceResolver,
    ) -> Self {
        let body = flow_analysis.analysis.body;
        let dependencies_for = |location: DisplayViaDebug<_>, arg| {
            let ana = flow_analysis.state_at(*location);
            let aliases = &flow_analysis.analysis.aliases;
            ana.deps(arg)
                .flat_map(|&(dep_loc, _dep_place)| {
                    let dep_loc = DisplayViaDebug(dep_loc);
                    let (target, places) = if dep_loc.is_real(body) {
                        let (operands, target_ret) =
                            if let mir::TerminatorKind::Call {
                                args, destination, ..
                            } = &body.stmt_at(*dep_loc).right().unwrap().kind
                            {
                                (args, destination)
                            } else {
                                unreachable!()
                            };

                        let places = flowistry::mir::utils::arg_places(operands.as_slice())
                            .into_iter()
                            .map(|(idx, p)| {
                                (TargetPlace::Argument(ArgumentIndex::from_usize(idx)), p)
                            })
                            .chain(std::iter::once({
                                let p = target_ret.unwrap().0;
                                (TargetPlace::Return, p)
                            }));
                        (Target::Call(dep_loc), Either::Left(places))
                    } else {
                        (
                            Target::Argument(ArgumentIndex::from_usize(
                                dep_loc.statement_index - 1,
                            )),
                            Either::Right(std::iter::once((
                                TargetPlace::Return,
                                mir::Local::from_usize(dep_loc.statement_index).into(),
                            ))),
                        )
                    };
                    places.into_iter().flat_map(
                        move |(abstract_target_place, concrete_target_place)| {
                            // The below code is an alternate implementation
                            // that includes children in the resolution

                            // let children = aliases.reachable_values(arg, rustc_ast::Mutability::Mut);
                            // children.into_iter()
                            //     .cloned()
                            //     .chain(std::iter::once(arg))
                            //     .filter_map(move |child| {
                            //         // In theory we do not have to replace the
                            //         // base here (because it gets substituted)
                            //         // but the types force it.
                            //         debug!("Resolving child {child:?} of {arg:?}");

                            //         // TODO I think this resolution should always
                            //         // succeed but it doesn't. Should investigate.
                            //         let mut target_term = place_resolver.try_resolve(arg, child)?.replace_base(abstract_target_place.clone());
                            //         debug!("Resolving {child:?} to {concrete_target_place:?}");
                            //         let inner_term = place_resolver
                            //             .try_resolve(child, concrete_target_place)?
                            //             .replace_base(abstract_target_place.clone());
                            //         target_term.sub(inner_term);
                            //         Some(Dependency {
                            //             target_term,
                            //             target,
                            //         })
                            //     })
                            place_resolver
                                .try_resolve(arg, concrete_target_place)
                                .into_iter()
                                .map(move |terms| Dependency {
                                    target_term: Term::from_raw(
                                        abstract_target_place.clone(),
                                        terms,
                                    ),
                                    target,
                                })
                        },
                    )
                })
                .collect()
        };
        let calls = body
            .basic_blocks()
            .iter_enumerated()
            .filter_map(|(bb, bbdat)| {
                let (function, simple_args, _) = bbdat.terminator().as_fn_and_args().ok()?;
                let bbloc = DisplayViaDebug(body.terminator_loc(bb));
                let arguments = IndexVec::from_raw(
                    simple_args
                        .into_iter()
                        .map(|arg| {
                            arg.map_or_else(Dependencies::default, |a| dependencies_for(bbloc, a))
                        })
                        .collect(),
                );
                Some((
                    bbloc,
                    Call {
                        function,
                        arguments,
                    },
                ))
            })
            .collect();
        let mut return_arg_deps: IndexVec<mir::Local, _> =
            IndexVec::from_raw(body.args_iter().map(|_| HashSet::new()).collect());
        let return_deps = body
            .all_returns()
            .map(DisplayViaDebug)
            .flat_map(|loc| {
                return_arg_deps.iter_enumerated_mut().for_each(|(i, s)| {
                    for d in dependencies_for(loc, i.into()) {
                        s.insert(d);
                    }
                });
                dependencies_for(loc, mir::Place::return_place())
                    .clone()
                    .into_iter()
            })
            .collect();
        Self {
            calls,
            return_deps,
            return_arg_deps: return_arg_deps.raw,
        }
    }
}

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct RelativePlace<L> {
    pub location: L,
    pub place: TargetPlace,
}

impl<L: Display> Display for RelativePlace<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} @ {}", self.location, self.place)
    }
}

pub type Dependencies2<L> = HashSet<Target<L>>;

fn fmt_deps2<L: Display>(
    deps: &Dependencies2<L>,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    f.write_char('{')?;
    let mut first_dep = true;
    for dep in deps {
        if first_dep {
            first_dep = false;
        } else {
            f.write_str(", ")?;
        }
        write!(f, "{dep}")?;
    }
    f.write_char('}')
}

impl<L: Display> Display for Call<Dependencies2<L>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char('(')?;
        let mut first = true;
        for arg in self.arguments.iter() {
            if first {
                first = false;
            } else {
                f.write_str(", ")?;
            }
            fmt_deps2(arg, f)?;
        }
        write!(f, ")   {:?}", self.function)?;
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SimpleLocation<C> {
    Return,
    Argument(ArgumentIndex),
    Call(C),
}

impl<L> SimpleLocation<L> {
    pub fn map_location<L0, F: FnMut(&L) -> L0>(&self, mut f: F) -> SimpleLocation<L0> {
        use SimpleLocation::*;
        match self {
            Argument(a) => Argument(*a),
            Call(l) => Call(f(l)),
            Return => Return,
        }
    }
}

impl<D: std::fmt::Display> std::fmt::Display for SimpleLocation<(D, DefId)> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use SimpleLocation::*;
        match self {
            Return => f.write_str("ret"),
            Argument(a) => write!(f, "{a:?}"),
            Call((gloc, did)) => write!(f, "{gloc} ({did:?})"),
        }
    }
}

impl<D: std::fmt::Display> std::fmt::Display for SimpleLocation<RelativePlace<D>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use SimpleLocation::*;
        match self {
            Return => f.write_str("ret"),
            Argument(a) => write!(f, "{a:?}"),
            Call(c) => write!(f, "{c}"),
        }
    }
}
#[derive(Debug)]
pub struct Body2<L> {
    pub calls: HashMap<L, Call<Dependencies2<L>>>,
    pub return_deps: Dependencies2<L>,
    pub return_arg_deps: Vec<Dependencies2<L>>,
    pub equations: Vec<algebra::Equality<SimpleLocation<RelativePlace<L>>, DisplayViaDebug<Field>>>,
}

impl<L: Display + Ord> Display for Body2<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ordered = self.calls.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|t| t.0);
        for (loc, call) in ordered {
            writeln!(f, "{:<6}: {}", format!("{}", loc), call)?
        }
        write!(f, "return: ")?;
        fmt_deps2(&self.return_deps, f)?;
        writeln!(f)?;
        write!(f, "return args: (")?;
        let mut first_arg = true;
        for arg in &self.return_arg_deps {
            if first_arg {
                first_arg = false;
            } else {
                f.write_str(", ")?;
            }
            fmt_deps2(arg, f)?;
        }
        f.write_char(')')?;
        writeln!(f)?;
        writeln!(f, "equations:")?;
        for eq in self.equations.iter() {
            writeln!(f, "  {eq}")?;
        }
        Ok(())
    }
}

impl Body2<DisplayViaDebug<Location>> {
    pub fn construct<'tcx>(
        flow_analysis: &df::FlowResults<'_, 'tcx, '_>,
        place_resolver: &algebra::PlaceResolver,
        tcx: TyCtxt<'tcx>,
        def_id: LocalDefId,
        body_with_facts: &'tcx flowistry::mir::borrowck_facts::CachedSimplifedBodyWithFacts<'tcx>,
    ) -> Self {
        let body = flow_analysis.analysis.body;
        let non_transitive_aliases =
            crate::ana::non_transitive_aliases::compute(tcx, def_id, body_with_facts);
        let mut place_table: HashMap<
            mir::Local,
            Vec<SimpleLocation<RelativePlace<DisplayViaDebug<Location>>>>,
        > = body
            .args_iter()
            .enumerate()
            .map(|(idx, l)| {
                (
                    l,
                    vec![
                        SimpleLocation::Argument(ArgumentIndex::from_usize(idx)),
                        //SimpleLocation::Return(Some(ArgumentIndex::from_usize(idx)))
                    ],
                )
            })
            .chain([(mir::RETURN_PLACE, vec![SimpleLocation::Return])])
            .collect();
        let dependencies_for = |location: DisplayViaDebug<_>, arg, is_mut_arg| {
            use rustc_ast::Mutability;
            let ana = flow_analysis.state_at(*location);
            let mutability = if false && is_mut_arg {
                    Mutability::Mut
                } else {
                    Mutability::Not
                };
            // Not sure this is necessary anymore because I changed the analysis
            // to transitively propagate in cases where a subplace is modified
            let reachable_values = non_transitive_aliases.reachable_values(
                arg,
                mutability,
            );
            debug!("Reachable values for {arg:?} are {reachable_values:?}");
            debug!("  Transitive reachable values are {:?}", flow_analysis.analysis.aliases.reachable_values(arg, mutability));
            let deps = reachable_values
                .into_iter()
                .flat_map(|p| non_transitive_aliases.children(*p))
                // Commenting out this filter because reachable values doesn't
                // always contain all relevant subplaces
                //.filter(|p| !is_mut_arg || p != &arg)
                .flat_map(|place| ana.deps(place))
                .map(|&(dep_loc, _dep_place)| {
                    let dep_loc = DisplayViaDebug(dep_loc);
                    if dep_loc.is_real(body) {
                        Target::Call(dep_loc)
                    } else {
                        Target::Argument(ArgumentIndex::from_usize(dep_loc.statement_index - 1))
                    }
                })
                .collect();
            debug!("  Registering dependencies {deps:?}");
            deps
        };
        let calls = body
            .basic_blocks()
            .iter_enumerated()
            .filter_map(|(bb, bbdat)| {
                let (function, simple_args, _) = bbdat.terminator().as_fn_and_args().ok()?;
                let bbloc = DisplayViaDebug(body.terminator_loc(bb));

                let mk_rp = |place| {
                    SimpleLocation::Call(RelativePlace {
                        location: bbloc,
                        place,
                    })
                };

                let (operands, target_ret) =
                    if let mir::TerminatorKind::Call {
                        args, destination, ..
                    } = &body.stmt_at(*bbloc).right().unwrap().kind
                    {
                        (args, destination)
                    } else {
                        unreachable!()
                    };

                for (idx, place) in flowistry::mir::utils::arg_places(operands.as_slice()) {
                    assert!(place.projection.is_empty());
                    place_table
                        .entry(place.local)
                        .or_insert_with(Vec::new)
                        .push(mk_rp(TargetPlace::Argument(ArgumentIndex::from_usize(idx))));
                }
                let target_ret = target_ret.unwrap().0;
                assert!(target_ret.projection.is_empty());
                place_table
                    .entry(target_ret.local)
                    .or_insert_with(Vec::new)
                    .push(mk_rp(TargetPlace::Return));

                let arguments = IndexVec::from_raw(
                    simple_args
                        .into_iter()
                        .map(|arg| {
                            arg.map_or_else(Dependencies2::default, |a| {
                                dependencies_for(bbloc, a, false)
                            })
                        })
                        .collect(),
                );
                Some((
                    bbloc,
                    Call {
                        function,
                        arguments,
                    },
                ))
            })
            .collect();
        let mut return_arg_deps: Vec<(mir::Place<'tcx>, _)> = body
            .args_iter()
            .map(|a| (a.into(), HashSet::new()))
            .collect();
        debug!("Return arguments are {return_arg_deps:?}");
        let return_deps = body
            .all_returns()
            .map(DisplayViaDebug)
            .flat_map(|loc| {
                return_arg_deps.iter_mut().for_each(|(i, s)| {
                    debug!("Return arg dependencies for {i:?} at {loc}");
                    for d in dependencies_for(loc, *i, true) {
                        s.insert(d);
                    }
                });
                dependencies_for(loc, mir::Place::return_place(), false)
                    .clone()
                    .into_iter()
            })
            .collect();

        debug!(
            "Equations before simplify:\n{}",
            crate::utils::Print(|f: &mut std::fmt::Formatter<'_>| {
                for eq in place_resolver.equations().iter() {
                    writeln!(f, "  {eq}")?;
                }
                Ok(())
            })
        );
        debug!(
            "And place table\n{}",
            crate::utils::Print(|f: &mut std::fmt::Formatter<'_>| {
                for (k, v) in place_table.iter() {
                    write!(f, "  {k:?}: ")?;
                    let mut first = true;
                    for t in v {
                        if first {
                            first = false;
                        } else {
                            f.write_str(", ")?;
                        }
                        t.fmt(f)?;
                    }
                    writeln!(f)?;
                }
                Ok(())
            })
        );
        let equations = algebra::rebase_simplify(
            place_resolver.equations().iter().map(Cow::Borrowed).chain(
                place_table
                    .keys()
                    .map(|k| DisplayViaDebug(*k))
                    .map(|k| Cow::Owned(Equality::new(Term::new_base(k), Term::new_base(k)))),
            ),
            |base| {
                place_table
                    .get(base)
                    .cloned()
                    .map(Either::Left)
                    .unwrap_or(Either::Right(*base))
            },
        );
        debug!(
            "Equations after simplify:\n{}",
            crate::utils::Print(|f: &mut std::fmt::Formatter<'_>| {
                for eq in equations.iter() {
                    writeln!(f, "  {eq}")?;
                }
                Ok(())
            })
        );
        Self {
            calls,
            return_deps,
            return_arg_deps: return_arg_deps.into_iter().map(|(_, s)| s).collect(),
            equations,
        }
    }
}

pub fn compute2_from_body_id(
    body_id: BodyId,
    tcx: TyCtxt,
    gli: GLI,
) -> Body2<DisplayViaDebug<Location>> {
    let local_def_id = tcx.hir().body_owner_def_id(body_id);
    let target_name = body_name_pls(tcx, body_id);
    debug!("Analyzing function {target_name}");
    let body_with_facts = borrowck_facts::get_body_with_borrowck_facts(tcx, local_def_id);
    let body = body_with_facts.simplified_body();
    let flow = df::compute_flow_internal(tcx, gli, body_id, body_with_facts, &NeverInline);
    mir::pretty::write_mir_fn(
        tcx,
        body,
        &mut |_, _| Ok(()),
        &mut outfile_pls(&format!("{}.mir", target_name)).unwrap(),
    )
    .unwrap();
    let ref mut states_out = outfile_pls(&format!("{}.df", target_name)).unwrap();
    for l in body.all_locations() {
        writeln!(states_out, "{l:?}: {}", flow.state_at(l)).unwrap();
    }
    let place_resolver = algebra::PlaceResolver::construct(tcx, body);
    let r = Body2::construct(&flow, &place_resolver, tcx, local_def_id, body_with_facts);
    let mut out = outfile_pls(&format!("{}.regal", target_name)).unwrap();
    use std::io::Write;
    write!(&mut out, "{}", r).unwrap();
    r
}