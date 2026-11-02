
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
int main(void) {
 for(int i=0;i<1000;i++) {
  void *cause=legacy_make(-2147483647-1);
  char output[256];
  EmelaString s=legacy_show(cause,output,256);
  const char *expected="error: Boom(n: -2147483648)";
  assert(s.data==output && s.length==strlen(expected) && memcmp(s.data,expected,s.length)==0);
  emela_embedded_release(cause);assert(test_heap_empty());
 }
}
