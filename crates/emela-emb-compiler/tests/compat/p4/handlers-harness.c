#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static int trace;
int32_t board_mark(int32_t n) { trace=trace*10+n;return n; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort(); }
int main(void) {
  for(int i=0;i<1000;i++) {
    trace=0;assert(operation_failure()==7 && trace==384 && test_heap_empty());
    trace=0;assert(layer_lifecycle()==42 && trace==12389 && test_heap_empty());
    trace=0;assert(overridden_arguments()==45 && trace==51289 && test_heap_empty());
    assert(capability_shadow()==12 && test_heap_empty());
    assert(label_providers()==123 && test_heap_empty());
    assert(explicit_default()==2 && label_merge()==1 && test_heap_empty());
    trace=0;int failed=handler_failure();if(failed!=7||trace!=123894||!test_heap_empty()){fprintf(stderr,"handler failure result=%d trace=%d empty=%d\n",failed,trace,test_heap_empty());return 2;}
    trace=0;assert(initializer_failure()==7 && trace==1294 && test_heap_empty());
  }
  return 0;
}
