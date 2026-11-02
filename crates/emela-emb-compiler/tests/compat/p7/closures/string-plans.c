
#include <assert.h>
#include <string.h>
#include "emela_embedded.h"
int main(void) {
  const int32_t inputs[] = {0, 41, -1, INT32_MIN, INT32_MAX};
  const char *expected[] = {"0:1", "41:42", "-1:0", "-2147483648:-2147483647", "2147483647:-2147483648"};
  for (int round = 0; round < 1000; ++round) {
    for (unsigned i = 0; i < sizeof inputs / sizeof inputs[0]; ++i) {
      char buffer[64];
      EmelaString value = legacy_render(inputs[i], buffer, UINT16_C(64));
      assert(value.length == strlen(expected[i]));
      assert(memcmp(value.data, expected[i], value.length) == 0);
      assert(value.data == buffer && value.owner == NULL);
    }
  }
  return 0;
}
