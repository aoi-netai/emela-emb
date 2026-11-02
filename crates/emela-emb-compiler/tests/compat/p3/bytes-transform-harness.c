#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  fprintf(stderr,"defect %d: %.*s\n",kind,length,message);abort();
}
#define BYTES(s) ((EmelaBytes){(const uint8_t *)(s),sizeof(s)-1,NULL})
#define TEXT(s) ((EmelaString){s,sizeof(s)-1,NULL})
static void equal_text(EmelaString got,const char *text) { assert(got.length==strlen(text) && !memcmp(got.data,text,got.length)); }
int main(void) {
  char output[512];
  for(int i=0;i<500;i++) {
    assert(decode_matches(BYTES("é👍\xef\xbb\xbf\0x"),TEXT("é👍\xef\xbb\xbf\0x")));
    assert(decode_matches(BYTES(""),TEXT("")));
    assert(decode_matches(BYTES("\xf4\x8f\xbf\xbf"),TEXT("\xf4\x8f\xbf\xbf")));
    assert(decode_invalid(BYTES("\xff")) && decode_invalid(BYTES("\xc0\xaf")));
    assert(decode_invalid(BYTES("\xe0\x80\x80")) && decode_invalid(BYTES("\xed\xa0\x80")));
    assert(decode_invalid(BYTES("\xf4\x90\x80\x80")) && decode_invalid(BYTES("\xe2\x82")));
    assert(decode_invalid(BYTES("\x80")));
    assert(bytes_compare(BYTES("\xff"),BYTES("\0"))==1);
    assert(bytes_compare(BYTES("\x01"),BYTES("\x01\x02"))==-1);
    assert(bytes_compare(BYTES("\x01\x02"),BYTES("\x01\x02"))==0);
    assert(bytes_hex(TEXT("DEADbeef"),BYTES("\xde\xad\xbe\xef")));
    assert(bytes_hex(TEXT(""),BYTES("")));
    assert(bytes_bad_hex(TEXT("abc")) && bytes_bad_hex(TEXT("zz")) && bytes_bad_hex(TEXT("ab cd")));
    equal_text(bytes_to_hex(BYTES("\0\x01\x0f\x10\xab"),output,sizeof output),"00010f10ab");
    equal_text(bytes_show(BYTES("\0\x80\xff"),output,sizeof output),"Bytes.hex(\"0080ff\")");
    assert(test_heap_empty());
  }
  return 0;
}
