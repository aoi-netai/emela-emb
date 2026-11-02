#include <assert.h>
#include <math.h>
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int armed;
static int defects;
static jmp_buf target;
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  static const char expected[] = "cannot convert Float to Int";
  if (!armed || kind != EMELA_DEFECT_FLOAT_TO_INT || length != sizeof expected - 1 || memcmp(message, expected, length) != 0) exit(5);
  defects++;
  longjmp(target, 1);
}
#define TEXT(bytes) ((EmelaString){bytes, sizeof(bytes) - 1, NULL})
int main(void) {
  assert(int_text_matches(INT32_MIN, TEXT("-2147483648")));
  assert(int_text_matches(INT32_MAX, TEXT("2147483647")));
  assert(int_text_matches(0, TEXT("0")));
  assert(int64_text_matches(INT64_MIN, TEXT("-9223372036854775808")));
  assert(int64_text_matches(INT64_MAX, TEXT("9223372036854775807")));
  assert(int64_text_matches(0, TEXT("0")));
  assert(float_text_matches(1.0e-6, TEXT("0.000001")));
  assert(float_text_matches(-0.0000012345678901234567, TEXT("-0.0000012345678901234567")));
  assert(float_text_matches(-INFINITY, TEXT("-Infinity")));
  assert(float_text_matches(NAN, TEXT("NaN")));
  assert(float_text_size(1.0e-6) == 8 && float_text_size(-0.0000012345678901234567) == 25);
  assert(float_text_size(-INFINITY) == 9 && float_text_size(NAN) == 3);
  assert(floor_value(2.5) == 2 && floor_value(-2.5) == -3);
  assert(floor_value(2147483647.75) == INT32_MAX);
  assert(ceil_value(-2.5) == -2 && ceil_value(-2147483648.75) == INT32_MIN);
  assert(round_value(2.5) == 3 && round_value(-2.5) == -3 && round_value(-0.5) == -1);
  assert(truncate_value(-2.9) == -2);
  int32_t (*convert[])(double) = {floor_value, ceil_value, round_value, truncate_value};
  void (*discard[])(double) = {discard_floor, discard_ceil, discard_round, discard_truncate};
  const double invalid[] = {NAN, INFINITY, -INFINITY, 2147483648.0, -2147483649.0};
  armed = 1;
  for (unsigned op = 0; op < sizeof convert / sizeof *convert; op++) {
    for (unsigned value = 0; value < sizeof invalid / sizeof *invalid; value++) {
      if (setjmp(target) == 0) { (void)convert[op](invalid[value]); return 2; }
      if (setjmp(target) == 0) { discard[op](invalid[value]); return 3; }
    }
  }
  assert(defects == 40);
  return 0;
}
