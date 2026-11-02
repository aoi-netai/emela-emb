
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
int main(void) { for(int i=0;i<1000;i++) {assert(legacy_attempt(i%2)==42);assert(test_heap_empty());} }
