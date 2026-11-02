
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static unsigned order;
void mark(int32_t n) { assert(n == (int32_t)++order); }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
int main(void) {
  for (int i = 0; i < 1000; i++) { order = 0; assert(legacy_instant(i) == i + 1); assert(order == 3); assert(test_heap_empty()); }
  return 0;
}
