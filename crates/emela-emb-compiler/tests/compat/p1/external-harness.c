#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  (void)kind; (void)message; (void)length; abort();
}
int64_t board_integer(int64_t n) { return n; }
double board_mix(int64_t n, double x) { assert(n == INT64_MIN); return x; }
static uint64_t bits(double n) { uint64_t out; memcpy(&out, &n, sizeof out); return out; }
int main(void) {
  const int64_t integers[] = {INT64_MIN, INT64_MAX, -INT64_C(9007199254740993), INT64_C(9007199254740993), -1, 0, 1};
  const uint64_t floats[] = {UINT64_C(0), UINT64_C(0x8000000000000000), UINT64_C(1), UINT64_C(0x8000000000000001), UINT64_C(0x0010000000000000), UINT64_C(0x7fefffffffffffff), UINT64_C(0x7ff0000000000000), UINT64_C(0xfff0000000000000), UINT64_C(0x7ff8000000000042), UINT64_C(0xfff8000000001234), UINT64_C(0x3ff123456789abcd)};
  for (int iteration = 0; iteration < 1000; iteration++) {
    for (unsigned i = 0; i < sizeof integers / sizeof *integers; i++) assert(integer_bits(integers[i]) == integers[i]);
    for (unsigned i = 0; i < sizeof floats / sizeof *floats; i++) {
      double n; memcpy(&n, &floats[i], sizeof n);
      assert(bits(float_bits(INT64_MIN, n)) == floats[i]);
    }
  }
  return 0;
}
