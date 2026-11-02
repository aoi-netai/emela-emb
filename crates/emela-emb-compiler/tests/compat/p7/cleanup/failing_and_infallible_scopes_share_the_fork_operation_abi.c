#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
 for(int i=0;i<1000;i++) {
  for(int which=0;which<3;which++) {
   int32_t result=-1;EmelaFiber f=which==2 ? legacy_nested_start(i,&result):(which ? legacy_bad_start(i,&result):legacy_good_start(i,&result));
   while(!emela_fiber_done(f)) { assert(emela_scheduler_run(1)); }
   assert(result==(which==2 ? 2*i+1:i+!which));emela_fiber_release(f);assert(test_heap_empty());
  }
 }
}
