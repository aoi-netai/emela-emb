mod support;
use emela_emb_compiler::compile;
#[test]
fn const_patterns_use_literal_coverage_and_runtime_cache_with_custom_equality() {
    let source = r#"
type Plain(value:Int)
type Key(value:Int)
impl Key:Eq{fn eq(self,other){self.value%10==other.value%10}}
@external(c,"seed")
fn seed()->Int use {}
const LIMIT=42
const WORD="é"
const YES=True
const NO=False
const PLAIN=Plain(7)
const KEY=Key(7)
const DYNAMIC=seed()
@export(c,"number_pattern")
pub fn number_pattern(n:Int)->Int{n match {LIMIT->1
DYNAMIC->2
_->3}}
@export(c,"bool_pattern")
pub fn bool_pattern(n:Bool)->Int{n match{YES->1
NO->0}}
@export(c,"text_pattern")
pub fn text_pattern(s:String)->Int{s match{WORD->1
_->0}}
@export(c,"data_pattern")
pub fn data_pattern()->Int{a=Plain(7) match{PLAIN->20
_->0}
b=Key(17) match{KEY->22
_->0}
a+b}
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int calls;
int32_t seed(void){calls++;return 7;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(number_pattern(42)==1&&calls==0);assert(number_pattern(7)==2&&calls==1);assert(number_pattern(8)==3&&calls==1);assert(bool_pattern(1)==1&&bool_pattern(0)==0);assert(text_pattern((EmelaString){"é",2,NULL})==1);assert(text_pattern((EmelaString){"x",1,NULL})==0);assert(data_pattern()==42);return 0;}
"#,
    );
}
#[test]
fn never_const_pattern_stops_before_later_pattern_or_body_evaluation() {
    let source = r#"
const STOP=panic("stop")
@export(c,"stop_pattern")
pub fn stop_pattern()->Int{1 match{STOP->0
_->42}}
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf jump;static int defects;
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;assert(n>=4);int found=0;for(uint16_t i=0;i+4<=n;i++)if(!memcmp(m+i,"stop",4))found=1;assert(found);defects++;longjmp(jump,1);}
int main(void){if(setjmp(jump)==0){stop_pattern();abort();}if(setjmp(jump)==0){stop_pattern();abort();}assert(defects==2);return 0;}
"#,
    );
}
