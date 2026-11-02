#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static int trace;
int32_t board_record_mark(int32_t n) { trace=trace*10+n;return n; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort(); }
int main(void) {
  for(int i=0;i<1000;i++) {
    trace=0;assert(record_order()==21 && trace==123 && test_heap_empty());
    assert(record_generic(i) && test_heap_empty());
    trace=0;assert(record_base_failure(i) && trace==12 && test_heap_empty());
    trace=0;assert(record_field_failure(i) && trace==14 && test_heap_empty());
    trace=0;assert(record_never_base(i) && trace==1 && test_heap_empty());
    assert(record_error(i) && test_heap_empty());
    char original[]={'a',0,'b'};
    void *updated=record_return_updated((EmelaString){original,3,NULL});
    void *items=record_return_items((EmelaString){original,3,NULL});
    memset(original,'z',3);
    EmelaString text=record_items_first(updated);emela_embedded_release(updated);
    EmelaString first=record_items_first(items);emela_embedded_release(items);
    assert(text.owner && text.length==8 && !memcmp(text.data,"copy a\0b",8));
    assert(first.owner && first.length==3 && !memcmp(first.data,"a\0b",3));
    emela_embedded_release(text.owner);emela_embedded_release(first.owner);
    assert(test_heap_empty());
    assert(record_handler()==42 && test_heap_empty());
    assert(record_enum_method()==42 && test_heap_empty());
  }
  return 0;
}
