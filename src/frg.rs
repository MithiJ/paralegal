extern crate pretty;

use crate::HashSet;
use pretty::{DocAllocator, DocBuilder, Pretty};

use crate::desc::{
    Annotation, AnnotationRefinement, DataSink, DataSource, Identifier, ObjectType,
    ProgramDescription, Relation,
};

trait DocLines<'a, A = ()>: DocAllocator<'a, A>
where
    A: 'a,
{
    #[inline]
    fn lines<I>(&'a self, docs: I) -> DocBuilder<'a, Self, A>
    where
        I: IntoIterator,
        I::Item: Pretty<'a, Self, A>,
        DocBuilder<'a, Self, A>: Clone,
    {
        self.intersperse(docs, self.hardline())
    }
}

impl<'a, T, A> DocLines<'a, A> for T
where
    T: DocAllocator<'a, A>,
    A: 'a,
{
}

trait DocFrgRel<'a, A = ()>: DocAllocator<'a, A>
where
    A: 'a,
{
    #[inline]
    fn forge_relation<I, LIter, RIter>(&'a self, rel: I) -> DocBuilder<'a, Self, A>
    where
        I: IntoIterator<Item = (LIter, RIter)>,
        LIter: IntoIterator,
        LIter::Item: Pretty<'a, Self, A>,
        RIter: IntoIterator,
        RIter::Item: Pretty<'a, Self, A>,
        DocBuilder<'a, Self, A>: Clone,
    {
        let mut prit = rel.into_iter().peekable();
        if prit.peek().is_some() {
            self.intersperse(
                prit.filter_map(|(l, r)| {
                    let mut pl = l.into_iter().peekable();
                    let mut pr = r.into_iter().peekable();
                    if pl.peek().is_some() && pr.peek().is_some() {
                        Some(
                            self.intersperse(pl, self.text(" + "))
                                .parens()
                                .append("->")
                                .append(self.intersperse(pr, self.text(" + ")).parens()),
                        )
                    } else {
                        None
                    }
                }),
                self.text(" +").append(self.hardline()),
            )
        } else {
            self.text("none")
        }
    }
}

impl<'a, T, A> DocFrgRel<'a, A> for T
where
    T: DocAllocator<'a, A>,
    A: 'a,
{
}

pub trait ToForge {
    fn as_forge<'b, 'a: 'b, A: DocAllocator<'b, ()>>(
        &'a self,
        alloc: &'b A,
    ) -> DocBuilder<'b, A, ()>
    where
        A::Doc: Clone;
}

impl ToForge for Identifier {
    fn as_forge<'b, 'a: 'b, A: DocAllocator<'b, ()>>(
        &'a self,
        alloc: &'b A,
    ) -> DocBuilder<'b, A, ()>
    where
        A::Doc: Clone,
    {
        alloc.text(self.as_str())
    }
}

impl<X: ToForge, Y: ToForge> ToForge for Relation<X, Y> {
    fn as_forge<'b, 'a: 'b, A: DocAllocator<'b, ()>>(
        &'a self,
        alloc: &'b A,
    ) -> DocBuilder<'b, A, ()>
    where
        A::Doc: Clone,
    {
        alloc.forge_relation(self.0.iter().map(|(src, sinks)| {
            (
                std::iter::once(src.as_forge(alloc)),
                sinks.iter().map(|sink| sink.as_forge(alloc)),
            )
        }))
    }
}

fn data_sink_as_forge<'b, A: DocAllocator<'b, ()>>(
    alloc: &'b A,
    function: &'b Identifier,
    arg_slot: usize,
) -> DocBuilder<'b, A, ()>
where
    A::Doc: Clone,
{
    function
        .as_forge(alloc)
        .append(alloc.text("_"))
        .append(alloc.as_string(arg_slot))
}

impl ToForge for DataSink {
    fn as_forge<'b, 'a: 'b, A: DocAllocator<'b, ()>>(
        &'a self,
        alloc: &'b A,
    ) -> DocBuilder<'b, A, ()>
    where
        A::Doc: Clone,
    {
        data_sink_as_forge(alloc, &self.function, self.arg_slot)
    }
}

impl<T: ToForge> ToForge for HashSet<T> {
    fn as_forge<'b, 'a: 'b, A: DocAllocator<'b, ()>>(
        &'a self,
        alloc: &'b A,
    ) -> DocBuilder<'b, A, ()>
    where
        A::Doc: Clone,
    {
        if self.is_empty() {
            alloc.text("none")
        } else {
            alloc.intersperse(self.iter().map(|w| w.as_forge(alloc)), "+")
        }
    }
}

fn data_source_as_forge<'b, A: DocAllocator<'b, ()>>(
    src: &DataSource,
    alloc: &'b A,
) -> DocBuilder<'b, A, ()> {
    match src {
        DataSource::FunctionCall(f) => alloc.text("call_").append(alloc.as_string(f)),
        DataSource::Argument(a) => alloc.text("arg_").append(alloc.as_string(a)),
    }
}

impl ToForge for DataSource {
    fn as_forge<'b, 'a: 'b, A: DocAllocator<'b, ()>>(
        &'a self,
        alloc: &'b A,
    ) -> DocBuilder<'b, A, ()>
    where
        A::Doc: Clone,
    {
        data_source_as_forge(self, alloc)
    }
}

const SRC_NAME: &'static str = "Src";
const ARG_NAME: &'static str = "Arg";
const FN_CALL_NAME: &'static str = "Call";
const FN_OBJ_NAME: &'static str = "Fn";
const FN_NAME: &'static str = "CallSite";
const CTRL_NAME: &'static str = "Ctrl";
const FLOW_NAME: &'static str = "flow";
const FLOWS_PREDICATE_NAME: &'static str = "Flows";
const OBJ_NAME: &'static str = "Object";
const LABEL_NAME: &'static str = "Label";
const LABELS_REL_NAME: &'static str = "labels";
const TYPES_NAME: &'static str = "types";
const TYPE_NAME: &'static str = "Type";
const FN_REL_NAME: &'static str = "function";
const OTYPE_REL_NAME: &'static str = "otype";

impl ToForge for ProgramDescription {
    fn as_forge<'b, 'a: 'b, A: DocAllocator<'b, ()>>(
        &'a self,
        alloc: &'b A,
    ) -> DocBuilder<'b, A, ()>
    where
        A::Doc: Clone,
    {
        /// For now the order here *must* be topological as the code gen does not reorder this automatically
        const SIGS: &[(
            &'static str,
            Option<&'static str>,
            &'static [(&'static str, &'static str)],
        )] = &[
            (LABEL_NAME, None, &[]),
            (OBJ_NAME, None, &[(LABELS_REL_NAME, "set Label")]),
            (SRC_NAME, Some(OBJ_NAME), &[]),
            (FN_OBJ_NAME, Some(OBJ_NAME), &[]),
            (FN_NAME, Some(OBJ_NAME), &[(FN_REL_NAME, "one Fn")]),
            (ARG_NAME, Some(SRC_NAME), &[]),
            (TYPE_NAME, Some(OBJ_NAME), &[(OTYPE_REL_NAME, "set Type")]),
            (FN_CALL_NAME, Some(SRC_NAME), &[]),
            (
                CTRL_NAME,
                None,
                &[
                    (FLOW_NAME, "set Src->CallSite"), // I'd have liked to define these types in terms of other string constants, but it seems rust doesn't let you concatenate strings at compile time
                    (TYPES_NAME, "set Src->Type"),
                ],
            ),
        ];

        fn make_one_sig<'b, A: DocAllocator<'b, ()>, I: Pretty<'b, A, ()>>(
            alloc: &'b A,
            inner: I,
            parent: &'static str,
        ) -> DocBuilder<'b, A, ()>
        where
            A::Doc: Clone,
        {
            alloc
                .text("one sig ")
                .append(inner)
                .append(" extends ")
                .append(parent)
                .append(" {}")
        }

        alloc.lines([
            alloc.text("#lang forge"),
            alloc.nil(),
            alloc
                .text("/* This file is auto-generated by ")
                .append(crate_name!())
                .append(" version ")
                .append(crate_version!())
                .append(". */"),
            alloc.nil(),
            alloc.lines(SIGS.iter().map(|(name, parent, fields)| {
                alloc
                    .text("abstract sig ")
                    .append(*name)
                    .append(parent.map_or(alloc.nil(), |p| alloc.text(" extends ").append(p)))
                    .append(alloc.space())
                    .append(
                        if fields.is_empty() {
                            alloc.nil()
                        } else {
                            alloc.hardline().append(
                                alloc
                                    .intersperse(
                                        fields.iter().map(|(name, typ)| {
                                            alloc.text(*name).append(": ").append(alloc.text(*typ))
                                        }),
                                        alloc.text(",").append(alloc.hardline()),
                                    )
                                    .indent(4)
                                    .append(alloc.hardline()),
                            )
                        }
                        .braces(),
                    )
            })),
            alloc.nil(),
            alloc.lines(
                self.annotations
                    .values()
                    .flat_map(|v| v.0.iter())
                    .filter_map(Annotation::as_label_ann)
                    .map(|a| a.label)
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .map(|s| make_one_sig(alloc, alloc.text(s.as_str().to_string()), LABEL_NAME)),
            ),
            alloc.nil(),
            alloc.lines(self.all_sources().into_iter().map(|a| {
                make_one_sig(
                    alloc,
                    a.as_forge(alloc),
                    match a {
                        DataSource::Argument(_) => ARG_NAME,
                        DataSource::FunctionCall(_) => FN_CALL_NAME,
                    },
                )
            })),
            alloc.nil(),
            alloc.lines(self.annotations.iter().flat_map(|(name, (anns, nums))| {
                if let ObjectType::Function(num_args) = nums {
                    Box::new(
                        std::iter::once(make_one_sig(alloc, name.as_forge(alloc), FN_OBJ_NAME))
                            .chain((0..*num_args).map(|arg_slot| {
                                make_one_sig(
                                    alloc,
                                    data_sink_as_forge(alloc, name, arg_slot),
                                    FN_NAME,
                                )
                            })),
                    ) as Box<dyn Iterator<Item = _>>
                } else {
                    Box::new(std::iter::once(make_one_sig(
                        alloc,
                        name.as_forge(alloc),
                        TYPE_NAME,
                    )))
                }
            })),
            alloc.nil(),
            alloc.lines(
                self.controllers
                    .keys()
                    .map(|e| make_one_sig(alloc, e.as_forge(alloc), CTRL_NAME)),
            ),
            alloc.nil(),
            alloc
                .text("pred ")
                .append(FLOWS_PREDICATE_NAME)
                .append(" ")
                .append(
                    alloc
                        .lines([
                            alloc.nil(),
                            alloc.lines(self.controllers.iter().map(|(e, ctrl)| {
                                alloc.lines([
                                    e.as_forge(alloc)
                                        .append(".")
                                        .append(FLOW_NAME)
                                        .append(" = ")
                                        .append(
                                            alloc
                                                .hardline()
                                                .append(ctrl.flow.as_forge(alloc).indent(4))
                                                .append(alloc.hardline())
                                                .parens(),
                                        ),
                                    e.as_forge(alloc)
                                        .append(".")
                                        .append(TYPES_NAME)
                                        .append(" = ")
                                        .append(
                                            alloc
                                                .hardline()
                                                .append(alloc.forge_relation(
                                                    ctrl.types.iter().map(|(i, desc)| {
                                                        (
                                                            std::iter::once(data_source_as_forge(
                                                                i, alloc,
                                                            )),
                                                            desc.iter()
                                                                .map(|t| alloc.text(t.as_str())),
                                                        )
                                                    }),
                                                ))
                                                .nest(4)
                                                .append(alloc.hardline())
                                                .parens(),
                                        ),
                                ])
                            })),
                            alloc.text(LABELS_REL_NAME).append(" = ").append(
                                alloc
                                    .hardline()
                                    .append(alloc.forge_relation(self.annotations.iter().flat_map(
                                        |(id, (anns, _))| {
                                            anns.iter().filter_map(Annotation::as_label_ann).map(
                                                |a| {
                                                    (
                                                        match &a.refinement {
                                                            AnnotationRefinement::None => Box::new(
                                                                std::iter::once(id.as_forge(alloc)),
                                                            )
                                                                as Box<dyn Iterator<Item = _>>,
                                                            AnnotationRefinement::Argument(
                                                                args,
                                                            ) => Box::new(args.iter().map(|i| {
                                                                id.as_forge(alloc)
                                                                    .append("_")
                                                                    .append(alloc.as_string(*i))
                                                            })),
                                                        },
                                                        std::iter::once(
                                                            alloc.text(a.label.as_str()),
                                                        ),
                                                    )
                                                },
                                            )
                                        },
                                    )))
                                    .nest(4)
                                    .append(alloc.hardline())
                                    .parens(),
                            ),
                            alloc.text(FN_REL_NAME).append(" = ").append(
                                alloc
                                    .hardline()
                                    .append(
                                        alloc
                                            .forge_relation(self.annotations.iter().filter_map(
                                                |(name, (_, num))| {
                                                    num.is_function().map(|n| {
                                                        (
                                                            (0..n).map(|arg_slot| {
                                                                data_sink_as_forge(
                                                                    alloc, name, arg_slot,
                                                                )
                                                            }),
                                                            std::iter::once(name.as_forge(alloc)),
                                                        )
                                                    })
                                                },
                                            ))
                                            .indent(4)
                                            .append(alloc.hardline()),
                                    )
                                    .parens(),
                            ),
                            alloc.text(OTYPE_REL_NAME).append(" = ").append(
                                alloc
                                    .hardline()
                                    .append(
                                        alloc
                                            .forge_relation(self.annotations.iter().map(
                                                |(o, (anns, _))| {
                                                    (
                                                        std::iter::once(o.as_forge(alloc)),
                                                        anns.iter()
                                                            .filter_map(Annotation::as_otype_ann)
                                                            .flat_map(|v| v.iter())
                                                            .map(|t| t.as_forge(alloc)),
                                                    )
                                                },
                                            ))
                                            .indent(4)
                                            .append(alloc.hardline()),
                                    )
                                    .parens(),
                            ),
                        ])
                        .nest(4)
                        .append(alloc.hardline())
                        .braces(),
                ),
        ])
    }
}