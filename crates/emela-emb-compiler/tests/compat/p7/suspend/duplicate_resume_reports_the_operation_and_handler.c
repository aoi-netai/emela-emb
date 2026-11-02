
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  static const char expected[] = "resume called twice in operation `get` of handler `Twice`";
  exit(k == EMELA_DEFECT_PANIC && n == sizeof(expected) - 1 && memcmp(m, expected, n) == 0 ? 0 : 2);
}
int main(void) { (void)legacy_work(); return 1; }
