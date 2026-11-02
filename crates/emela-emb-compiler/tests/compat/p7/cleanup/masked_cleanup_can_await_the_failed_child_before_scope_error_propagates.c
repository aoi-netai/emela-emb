#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask tasks[2];static int waits, marks, joined;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t,int32_t n) { assert(n>=1 && n<=2);tasks[n-1]=t;waits++; }
void mark(int32_t n) { if(n==1) { assert(!joined && marks++==0); } else { assert(n==2 && marks++==!joined); } }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
int test_heap_empty(void);
int main(void) {
 for(int i=0;i<1000;i++) {
  waits=marks=0;joined=i%2;int32_t result=-1;EmelaFiber f=legacy_work_start(joined,&result);
  while(emela_scheduler_run(1)) {}assert(waits==2);
  assert(emela_task_complete_int(tasks[0],0));
  while(!emela_fiber_done(f)) assert(emela_scheduler_run(1));
  assert(result==42 && marks==2-joined && !emela_task_complete_int(tasks[1],0));emela_fiber_release(f);
 }
}
