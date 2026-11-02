#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *s,uint16_t n) { (void)k;(void)s;(void)n;abort(); }
int main(void) {
  for(int i=0;i<1000;i++) {
    assert(finite_recursive(0)==42 && test_heap_empty());
    assert(finite_recursive(1)==3 && test_heap_empty());
    assert(finite_recursive(2)==2 && test_heap_empty());
  }
  for(int i=0;i<5;i++) {
    assert(deep_release(20000)==1 && test_heap_empty());
    assert(deep_release(0)==0 && test_heap_empty());
  }
  return 0;
}
