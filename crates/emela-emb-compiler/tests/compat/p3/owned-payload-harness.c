#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  fprintf(stderr, "defect %d: %.*s\n", kind, length, message); abort();
}
static volatile unsigned char saved;
void board_smash(void) {
  volatile unsigned char scratch[4096];
  for (unsigned i=0; i<sizeof scratch; i++) scratch[i]=(unsigned char)i;
  saved=scratch[sizeof scratch-1];
}
static void equal_text(EmelaString value, const char *expected, size_t length) {
  assert(value.length==length);
  assert(!memcmp(value.data,expected,length));
}
int main(void) {
  char output[256];
  for (int i=0; i<1000; i++) {
    void *list=store_owned(INT32_MIN);
    EmelaString left=read_owned(list), right=read_option_owned(list);
    assert(left.owner && right.owner);
    emela_embedded_release(list); board_smash();
    equal_text(left,"-2147483648",11); equal_text(right,"-2147483648",11);
    emela_embedded_release(left.owner); emela_embedded_release(right.owner);
    assert(test_heap_empty());
    char original[]={'a',0,'b'};
    list=store_borrowed((EmelaString){original,3,NULL});
    memset(original,'z',3);
    left=read_owned(list);right=read_option_owned(list);emela_embedded_release(list);
    assert(left.owner && right.owner);
    equal_text(left,"a\0b",3);equal_text(right,"a\0b",3);
    emela_embedded_release(left.owner);emela_embedded_release(right.owner);
    assert(test_heap_empty());
    list=store_owned(42);
    void *full=prepend_owned(list,(EmelaString){"head",4,NULL});
    void *tail=shared_tail(full);
    emela_embedded_release(list);emela_embedded_release(full);
    left=read_owned(tail);emela_embedded_release(tail);board_smash();
    assert(left.owner);equal_text(left,"42",2);emela_embedded_release(left.owner);
    assert(test_heap_empty());
    list=nested_owned(INT32_MAX);left=nested_first(list);emela_embedded_release(list);
    assert(left.owner);
    equal_text(left,"2147483647",10);emela_embedded_release(left.owner);
    assert(test_heap_empty());
    assert(batch_length(INT32_MIN)==12 && test_heap_empty());
    left=batch_string(INT32_MIN,output,sizeof output);board_smash();equal_text(left,"-2147483648",11);
    assert(!left.owner && left.data==output && test_heap_empty());
  }
  return 0;
}
