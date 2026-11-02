#include <assert.h>
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int trace, calls, zero_calls, defects;
static jmp_buf target;
int32_t board_seed(int32_t n) { trace = trace * 10 + n; calls++; return n * 20; }
int32_t board_zero(void) { zero_calls++; return 0; }
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  static const char expected[] = "while evaluating const BROKEN: division by zero";
  if (kind != EMELA_DEFECT_DIVISION_BY_ZERO || length != sizeof expected - 1 || memcmp(message, expected, length) != 0) exit(5);
  defects++;
  longjmp(target, 1);
}
int main(void) {
  assert(read_second() == 60 && trace == 12 && calls == 2);
  for (int i = 0; i < 1000; i++) assert(read_first() == 20 && read_second() == 60);
  assert(trace == 12 && calls == 2);
  for (int i = 0; i < 1000; i++) { if (setjmp(target) == 0) { (void)read_broken(); return 2; } }
  assert(defects == 1000 && zero_calls == 1 && trace == 12 && calls == 2);
  return 0;
}
