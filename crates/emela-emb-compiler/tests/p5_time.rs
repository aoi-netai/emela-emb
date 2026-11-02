mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn duration_and_instant_keep_wrap_arithmetic_opaque_values_and_gregorian_formatting() {
    let source = r#"
import Std.Duration.{Duration}
import Std.Instant.{Instant}
@external(c,"checkpoint")
fn checkpoint(index:Int)->Unit
@export(c,"time_values")
pub fn run()->Int {
  checkpoint(1)
  assert Duration.to_milliseconds(Duration.seconds(Int64.MAX)) == -1000
  checkpoint(2)
  assert Duration.to_milliseconds(Duration.add(Duration.milliseconds(Int64.MAX),Duration.milliseconds(1))) == Int64.MIN
  checkpoint(3)
  assert Duration.to_seconds(Duration.milliseconds(-1999)) == -1
  checkpoint(4)
  assert Duration.to_milliseconds(Duration.hours(2)) == 7200000
  checkpoint(5)
  assert Duration.to_milliseconds(Duration.minutes(2)) == 120000
  checkpoint(6)
  assert Duration.to_string(Duration.subtract(Duration.seconds(2),Duration.milliseconds(500))) == "1500ms"
  origin = Instant.from_unix_milliseconds(0)
  later = Instant.add(origin,Duration.seconds(42))
  checkpoint(7)
  assert Instant.to_unix_milliseconds(later) == 42000
  checkpoint(8)
  assert Duration.to_seconds(Instant.since(later,origin)) == 42
  checkpoint(9)
  assert Instant.subtract(later,Duration.seconds(42)) == origin
  checkpoint(10)
  assert Ord.compare(origin,later) == Less
  checkpoint(11)
  assert inspect(Duration.milliseconds(-42)) == "-42ms"
  checkpoint(12)
  assert Instant.to_string(origin) == "1970-01-01T00:00:00.000Z"
  checkpoint(13)
  assert Instant.to_string(Instant.from_unix_milliseconds(-1)) == "1969-12-31T23:59:59.999Z"
  checkpoint(14)
  assert Instant.to_string(Instant.from_unix_milliseconds(951782400000)) == "2000-02-29T00:00:00.000Z"
  checkpoint(15)
  assert Instant.to_string(Instant.from_unix_milliseconds(253402300800000)) == "+010000-01-01T00:00:00.000Z"
  checkpoint(16)
  assert Instant.to_string(Instant.from_unix_milliseconds(-62167219200000)) == "0000-01-01T00:00:00.000Z"
  checkpoint(17)
  assert Instant.to_string(Instant.from_unix_milliseconds(-62167219200001)) == "-000001-12-31T23:59:59.999Z"
  42
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_string_capacity(256),
    )
    .unwrap_or_else(|errors| panic!("time source rejected: {errors:#?}"));
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int step;void checkpoint(int32_t value){step=value;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"step %d defect %d: %.*s\n",step,k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++){assert(time_values()==42);}return 0;}
"#,
    );
}

#[test]
fn duration_and_instant_cannot_be_constructed_or_projected_outside_their_modules() {
    for (module, expression) in [
        ("Duration", "Duration(milliseconds:1)"),
        ("Duration", "Duration.milliseconds(1).milliseconds"),
        ("Instant", "Instant(milliseconds:1)"),
        ("Instant", "Instant.from_unix_milliseconds(1).milliseconds"),
    ] {
        let source = format!("import Std.{module}.{{{module}}}\nfn invalid(){{{expression}}}");
        let errors = emela_emb_compiler::compile(&source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.message.contains("opaque")),
            "{errors:#?}"
        );
    }
}
