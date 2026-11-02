#include <assert.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
static int calls;
int32_t board_mark(int32_t n) { calls++; return n; }
static uint64_t bits(double n) { uint64_t out; memcpy(&out, &n, sizeof out); return out; }
static double from_bits(uint64_t n) { double out; memcpy(&out, &n, sizeof out); return out; }
#define TEXT(bytes) ((EmelaString){bytes, sizeof(bytes) - 1, NULL})
int main(void) {
  assert(option_default(-1) == 7 && option_default(42) == 42);
  assert(option_failure(-1) == 7 && option_failure(42) == 42);
  assert(calls == 2);
  assert(option_traits(7, TEXT("é\0x")) == 7);
  assert(checked_add_default(INT32_MAX, 1, 7) == 7);
  assert(checked_add_default(INT32_MIN, -1, 7) == 7);
  assert(checked_add_default(INT32_MAX, 0, 7) == INT32_MAX);
  assert(checked_add_default(-2, 1, 7) == -1);
  const double nan_a = from_bits(UINT64_C(0x7ff8000000000042));
  const double nan_b = from_bits(UINT64_C(0xfff8000000001234));
  assert(float_order(nan_a, nan_b) == 0);
  assert(float_order(nan_a, INFINITY) == 1 && float_order(-INFINITY, nan_a) == -1);
  assert(float_order(-0.0, 0.0) == 0 && float_order(1, 2) == -1 && float_order(2, 1) == 1);
  assert(bits(float_minimum(-0.0, 0.0)) == UINT64_C(0x8000000000000000));
  assert(bits(float_minimum(0.0, -0.0)) == 0);
  assert(bits(float_maximum(-0.0, 0.0)) == UINT64_C(0x8000000000000000));
  assert(bits(float_maximum(0.0, -0.0)) == 0);
  assert(float_minimum(nan_a, 1) == 1 && float_minimum(1, nan_a) == 1);
  assert(bits(float_minimum(nan_a, nan_b)) == bits(nan_a));
  assert(bits(float_maximum(nan_a, nan_b)) == bits(nan_a));
  assert(bits(float_maximum(nan_a, 1)) == bits(nan_a));
  assert(bits(float_maximum(1, nan_a)) == bits(nan_a));
  assert(code_point_matches(0, TEXT("\0")));
  assert(code_point_matches(0x7f, TEXT("\x7f")));
  assert(code_point_matches(0x80, TEXT("\xc2\x80")));
  assert(code_point_matches(0x7ff, TEXT("\xdf\xbf")));
  assert(code_point_matches(0x800, TEXT("\xe0\xa0\x80")));
  assert(code_point_matches(0xd7ff, TEXT("\xed\x9f\xbf")));
  assert(code_point_matches(0xe000, TEXT("\xee\x80\x80")));
  assert(code_point_matches(0x10ffff, TEXT("\xf4\x8f\xbf\xbf")));
  assert(code_point_none(-1) && code_point_none(0xd800) && code_point_none(0xdfff));
  assert(code_point_none(0x110000) && code_point_none(INT32_MAX));
  return 0;
}
