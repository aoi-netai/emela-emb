
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static int releases;
void mark(int32_t n) { assert(n==1);releases++; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { (void)k;(void)m;(void)n;abort(); }
int main(void) { for(int i=0;i<1000;i++) { assert(legacy_rethrow(42)==42);assert(releases==i+1);assert(test_heap_empty()); } }
