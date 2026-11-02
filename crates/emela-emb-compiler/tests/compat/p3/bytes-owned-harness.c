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
int main(void) {
  for(unsigned i=0;i<500;i++) {
    char text[]={'a',0,'z'}; EmelaBytes b=bytes_make((EmelaString){text,3,NULL});
    memset(text,'x',3);assert(b.owner && b.length==3 && b.data[0]=='a' && b.data[1]==0);
    EmelaBytes c=bytes_copy(b),s=bytes_part(b,INT32_MIN,INT32_MAX),j=bytes_joined(b);
    void *list=bytes_listed(b);emela_embedded_release_bytes(&b);
    assert(c.data[2]=='z' && s.length==3 && j.length==6 && j.data[4]==0);
    EmelaBytes from_list=bytes_first(list);emela_embedded_release(list);
    assert(from_list.owner && from_list.length==3 && from_list.data[2]=='z');
    emela_embedded_release_bytes(&from_list);emela_embedded_release_bytes(&c);
    emela_embedded_release_bytes(&s);emela_embedded_release_bytes(&j);
    assert(test_heap_empty());
    uint8_t borrowed[]={0,128,255};c=bytes_copy((EmelaBytes){borrowed,3,NULL});
    memset(borrowed,1,3);assert(c.owner && c.data[1]==128 && c.data[2]==255);
    emela_embedded_release_bytes(&c);assert(test_heap_empty());
    assert(bytes_round_trip(0) && test_heap_empty());
    assert(bytes_round_trip(128) && test_heap_empty());
    assert(bytes_round_trip(255) && test_heap_empty());
    assert(bytes_round_trip(-1) && test_heap_empty());
    assert(bytes_round_trip(256) && test_heap_empty());
    bytes_discard((EmelaString){"abc",3,NULL});assert(test_heap_empty());
  }
  static uint8_t input[70000];for(unsigned i=0;i<sizeof input;i++) input[i]=(uint8_t)i;
  for(unsigned i=0;i<3;i++) {
    EmelaBytes out=bytes_part((EmelaBytes){input,sizeof input,NULL},-1,100000);
    assert(out.length==sizeof input && out.owner && memcmp(out.data,input,sizeof input)==0);
    emela_embedded_release_bytes(&out);assert(test_heap_empty());
  }
  return 0;
}
