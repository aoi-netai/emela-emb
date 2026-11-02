#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static int trace;
static char *input;
int32_t board_mark(int32_t n) { trace=trace*10+n;return n; }
void board_mutate_input(void) { memset(input,'z',3); }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort(); }
int main(void) {
  for(int i=0;i<1000;i++) {
    trace=0;assert(closure_order()==3 && trace==132 && test_heap_empty());
    assert(function_values(40)==42 && test_heap_empty());
    char original[]={'a',0,'b'};input=original;
    assert(closure_snapshot((EmelaString){original,3,NULL},(EmelaString){"a\0b",3,NULL}));
    assert(!memcmp(original,"zzz",3) && test_heap_empty());
    memcpy(original,"a\0b",3);
    assert(closure_failure((EmelaString){original,3,NULL},(EmelaString){"a\0b",3,NULL}));
    assert(!memcmp(original,"zzz",3) && test_heap_empty());
  }
  return 0;
}
