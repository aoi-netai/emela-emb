#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  (void)kind; (void)message; (void)length; abort();
}
void emela_platform_start(void (*program)(void)) { program(); }
static int calls;
int32_t board_increment(int32_t value) { calls++; return value + 1; }
int main(void) { assert(answer() == 42); assert(calls == 1); return 0; }
