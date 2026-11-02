mod support;
use emela_emb_compiler::compile;
#[test]
fn dbg_falls_back_recursively_uses_custom_show_and_returns_the_original_value_once() {
    let source = r#"
type Key(value:Int)
impl Key:Show{fn show(self){"K#{self.value}"}}
type Inner(text:String)
type Outer(key:Key,inner:Inner)
@external(c,"read_once")
fn read_once()->Int
@export(c,"debug_value")
pub fn debug_value()->Int{value=dbg(Outer(key:Key(read_once()),inner:Inner("x")))
value.key.value}
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int reads,logs;
int32_t read_once(void){reads++;return 7;}
void emela_board_debug(const char*s,uint16_t n){const char expected[]="Outer(key: K7, inner: Inner(text: \"x\"))";assert(n==sizeof(expected)-1&&!memcmp(s,expected,n));logs++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(debug_value()==7&&reads==1&&logs==1);return 0;}
"#,
    );
    assert!(compile("type Plain(value:Int)\nfn main(){inspect(Plain(1))}").is_err());
}
