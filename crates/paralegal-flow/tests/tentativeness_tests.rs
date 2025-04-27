#![feature(rustc_private)]
#[macro_use]
extern crate lazy_static;

use paralegal_flow::test_utils::*;
use paralegal_spdg::Identifier;

const CRATE_DIR: &str = "tests/tentativeness";

lazy_static! {
    static ref TEST_CRATE_ANALYZED: bool = {
        simple_logger::init_with_level(log::Level::Debug);
        run_paralegal_flow_with_flow_graph_dump(CRATE_DIR)
    };
}

macro_rules! define_test {
    ($($t:tt)*) => {
        paralegal_flow::define_flow_test_template!(TEST_CRATE_ANALYZED, CRATE_DIR, $($t)*);
    };
}
// TODO: how do I get marked struct?
// define_test!(deletes_in_both_cases: graph -> {
//     let source = graph.srcs_with_type(Identifier::new_intern("user_data"));
//     let sink = graph.marked(Identifier::new_intern("sink"));
//     assert!(!source.is_empty());
//     assert!(!sink.is_empty());
//     assert!(!source.flows_to_any(&sink));
// });
// The original version of Paralegal should pass this test since we have img 
// connected to the img in the if-else and the one that passes into delete

// Since the img.delete is only in one branch, there is no tentativeness
// There is no edge from img.delete to the Image in the if-case
define_test!(deletes_in_dummy_case: graph -> {
    let source = graph.marked(Identifier::new_intern("source"));
    let sink = graph.marked(Identifier::new_intern("sink"));
    assert!(!source.is_empty());
    assert!(!sink.is_empty());
    assert!(source.flows_to_any(&sink));
});

/// Here, Paralegal says there EXISTS a flow but our system says there should not
define_test!(delete_without_args: graph -> {
    let source = graph.marked(Identifier::new_intern("source"));
    let sink = graph.marked(Identifier::new_intern("sink"));
    assert!(!source.is_empty());
    assert!(!sink.is_empty());
    assert!(source.flows_to_any(&sink));
    // assert!(sink.always_depends_on_data(&source));
});

define_test!(simple_int_increment: graph -> {
    let source = graph.marked(Identifier::new_intern("source"));
});

/** 
 * ASK JUSTUS: How to write this like the quantifier but use the source code
 * Or remodel to use define_test! 
*/

define_test!(conditional_modification: graph -> {
    let source = graph.marked(Identifier::new_intern("source"));
    let sink = graph.marked(Identifier::new_intern("sink"));
    assert!(!source.is_empty());
    assert!(!sink.is_empty());
    assert!(source.flows_to_any(&sink));
    // assert!(!source.flows_to_any(&sink));
});
// Here, source shouldn't flow to sink because there is tentativeness - if it 
// enters the conditional branch, img is overwritten and doesn't get deleted.

define_test!(modified_in_loop: graph -> {
    let source = graph.marked(Identifier::new_intern("source"));
    let sink = graph.marked(Identifier::new_intern("sink"));
    assert!(!source.is_empty());
    assert!(!sink.is_empty());
    assert!(source.flows_to_any(&sink));
    // assert!(!source.flows_to_any(&sink));
});

define_test!(modifying_helper: graph -> {
    let source = graph.marked(Identifier::new_intern("source"));
    let sink = graph.marked(Identifier::new_intern("sink"));
    assert!(!source.is_empty());
    assert!(!sink.is_empty());
    assert!(source.flows_to_any(&sink));
    // assert!(!source.flows_to_any(&sink));
});

