#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

const BOARD: &str = r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) { fprintf(stderr,"defect %d: %.*s\n",kind,length,message); abort(); }
"#;

fn output(source: &str) -> emela_emb_compiler::Output {
    heap::with_heap_audit(
        compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(131072)
                .with_string_capacity(256),
        )
        .unwrap_or_else(|errors| panic!("{errors:?}")),
    )
}

#[test]
fn map_balancing_persistence_order_and_set_data_operations_reuse_a_fixed_heap() {
    let output = output(include_str!("compat/p3/collections.emel"));
    assert!(!output.c_source.contains("malloc("));
    support::compile_and_run(
        &output,
        &format!(
            "{BOARD}\nint main(void) {{ for (int i = 0; i < 1000; ++i) {{ assert(collections() == 42); assert(test_heap_empty()); }} return 0; }}"
        ),
    );
}

#[test]
fn map_and_set_dispatch_key_comparison_through_custom_traits() {
    let output = output(
        r#"
import Std.Map.{Map}
import Std.Set.{Set}
type Key(n: Int)
impl Key: Eq { fn eq(self, other) { self.n % 10 == other.n % 10 } }
impl Key: Ord { fn compare(self, other) { Ord.compare(self.n % 10, other.n % 10) } }
@export(c, "custom_keys")
pub fn custom_keys() -> Int {
  m = Map.from_list([(Key(12), 2), (Key(1), 1), (Key(21), 3)])
  assert Map.size(m) == 2 && Map.get(m, Key(11)) == Some(3)
  assert Map.keys(m) == [Key(21), Key(12)]
  assert Map.remove(m, Key(2)) == Map.from_list([(Key(31), 3)])
  s = Set.from_list([Key(12), Key(1), Key(21)])
  assert Set.size(s) == 2 && Set.contains(s, Key(31))
  assert Set.to_list(s) == [Key(1), Key(2)]
  42
}
"#,
    );
    support::compile_and_run(
        &output,
        &format!(
            "{BOARD}\nint main(void) {{ for (int i = 0; i < 1000; ++i) {{ assert(custom_keys() == 42); assert(test_heap_empty()); }} return 0; }}"
        ),
    );
}

#[test]
fn collection_internals_remain_opaque_and_key_bounds_are_checked() {
    for source in [
        "import Std.Map.{Map}\nfn leak() -> Map[Int, Int] { Map.Empty }",
        "import Std.Set.{Set}\nfn leak(s: Set[Int]) { s.inner }",
        "import Std.Map.{Map}\ntype Key(n: Int)\nfn invalid() { Map.insert(Map.empty(), Key(1), 42) }",
        "import Std.Set.{Set}\ntype Key(n: Int)\nfn invalid() { Set.insert(Set.empty(), Key(1)) }",
    ] {
        let errors = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default().with_heap_bytes(65536),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.source.as_deref() == Some("main.emel")),
            "{source}: {errors:?}"
        );
    }
}
