#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask tasks[4]; static int count;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t, int32_t n) { assert(n>=1 && n<=4);tasks[n-1]=t;count++; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
int main(void) {
 for(int i=0;i<1000;i++) {
  void *cause=NULL;count=0;
  EmelaFiber f=legacy_work_start(i%3 ? &cause : NULL);
  while(emela_scheduler_run(1)) {} assert(count==4);
  int order[3]={1,2,3}; if(i%2) { order[0]=3;order[2]=1; }
  for(int n=0;n<3;n++) {
   assert(emela_task_complete_int(tasks[order[n]-1],0));
   while(emela_scheduler_run(1)) {}
   assert(n==2 ? emela_fiber_done(f) : !emela_fiber_done(f));
  }
  assert(!emela_task_complete_int(tasks[3],0));emela_fiber_release(f);
  if(i%3) {
   void *again=NULL;
   if(i%2) { EmelaFiber g=legacy_rethrow_async_start(cause,&again);
    while(!emela_fiber_done(g)) { assert(emela_scheduler_run(1)); }
    emela_fiber_release(g);
   } else again=legacy_rethrow(cause);
   emela_embedded_release(cause);
   char buffer[256], expected[256];
   int length=snprintf(expected,sizeof(expected),"error: Boom(n: %d)\nnote: another fiber also failed: error: Boom(n: 2)\nnote: another fiber also failed: error: Boom(n: %d)",order[0],order[2]);
   EmelaString text=legacy_show(again,buffer,sizeof(buffer));
   assert(text.data==buffer && text.length==length && memcmp(text.data,expected,length)==0);
   assert(legacy_caught(again)==order[0]);
   emela_embedded_release(again);
  }
  assert(test_heap_empty());
 }
}
