#include <assert.h>
#include <math.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  (void)kind; (void)message; (void)length; abort();
}
int main(void) {
  assert(add32(INT32_MAX, 1) == INT32_MIN);
  assert(sub32(INT32_MIN, 1) == INT32_MAX);
  assert(mul32(INT32_MAX, 2) == -2);
  assert(neg32(INT32_MIN) == INT32_MIN);
  assert(div32(-7, 3) == -2 && rem32(-7, 3) == -1);
  assert(div32(INT32_MIN, -1) == INT32_MIN && rem32(INT32_MIN, -1) == 0);
  assert(add64(INT64_MAX, 1) == INT64_MIN);
  assert(sub64(INT64_MIN, 1) == INT64_MAX);
  assert(mul64(INT64_MAX, 2) == -2);
  assert(neg64(INT64_MIN) == INT64_MIN);
  assert(div64(-7, 3) == -2 && rem64(-7, 3) == -1);
  assert(div64(INT64_MIN, -1) == INT64_MIN && rem64(INT64_MIN, -1) == 0);
  assert(bits32(-1, INT32_C(0x12345678)) == -2);
  assert(bits64(-1, INT64_C(0x123456789abcdef0)) == -2);
  assert(left32(-1, 1) == -2 && left32(1, 32) == 0);
  assert(right32(-9, 2) == -3 && right32(-9, 32) == -1 && right32(9, 32) == 0);
  assert(unsigned32(-1, 1) == INT32_MAX && unsigned32(-1, 32) == 0);
  assert(left64(-1, 1) == -2 && left64(1, 64) == 0);
  assert(right64(-9, 2) == -3 && right64(-9, 64) == -1 && right64(9, 64) == 0);
  assert(unsigned64(-1, 1) == INT64_MAX && unsigned64(-1, 64) == 0);
  assert(min32() == INT32_MIN && max32() == INT32_MAX);
  assert(min64() == INT64_MIN && max64() == INT64_MAX);
  assert(float_arithmetic(3.0, 2.0) == -2.5);
  assert(isnan(float_arithmetic(NAN, 0.0)));
  assert(isinf(float_arithmetic(INFINITY, 0.0)));
  unit_value();
  assert(eq32(42, 42) && !eq32(42, 41));
  assert(ne32(42, 41) && !ne32(42, 42));
  assert(lt32(-2, -1) && !lt32(-1, -2));
  assert(le32(2, 2) && !le32(3, 2));
  assert(gt32(3, 2) && !gt32(2, 3));
  assert(ge32(-1, -1) && !ge32(-2, -1));
  assert(eq64(42, 42) && !eq64(42, 41));
  assert(ne64(42, 41) && !ne64(42, 42));
  assert(lt64(-2, -1) && !lt64(-1, -2));
  assert(le64(2, 2) && !le64(3, 2));
  assert(gt64(3, 2) && !gt64(2, 3));
  assert(ge64(-1, -1) && !ge64(-2, -1));
  assert(eqf(42, 42) && !eqf(42, 41));
  assert(nef(42, 41) && !nef(42, 42));
  assert(ltf(-2, -1) && !ltf(-1, -2));
  assert(lef(2, 2) && !lef(3, 2));
  assert(gtf(3, 2) && !gtf(2, 3));
  assert(gef(-1, -1) && !gef(-2, -1));
  assert(!eqf(NAN, 0) && nef(NAN, 0));
  assert(!ltf(NAN, 0) && !lef(NAN, 0) && !gtf(NAN, 0) && !gef(NAN, 0));
  assert(addf(1.25, 2.75) == 4.0 && subf(4.5, 1.25) == 3.25);
  assert(mulf(1.5, 2.0) == 3.0 && divf(7.0, 2.0) == 3.5 && remf(7.0, 2.0) == 1.0);
  assert(band32(-1, INT32_C(0x12345678)) == INT32_C(0x12345678));
  assert(bor32(INT32_C(0x40000000), 1) == INT32_C(0x40000001));
  assert(bxor32(-1, -1) == 0 && bnot32(0) == -1);
  assert(band64(-1, INT64_C(0x123456789)) == INT64_C(0x123456789));
  assert(bor64(INT64_C(0x4000000000000000), 1) == INT64_C(0x4000000000000001));
  assert(bxor64(-1, -1) == 0 && bnot64(0) == -1);
  return 0;
}
