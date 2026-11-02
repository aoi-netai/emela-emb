#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask tasks[4];static int count, order;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t,int32_t n) { assert(n>=1 && n<=4);tasks[n-1]=t;count++; }
void mark(int32_t n) { assert(n==++order); }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
int main(void) {
 for(int i=0;i<1000;i++) {
  count=order=0;int32_t result=-1;EmelaFiber f=legacy_work_start(&result);
  while(emela_scheduler_run(1)) {} assert(count==3);
  int first=i%2 ? 1:3,second=i%2 ? 3:1;
  assert(emela_task_complete_int(tasks[first-1],0));while(emela_scheduler_run(1)) {}
  assert(emela_task_complete_int(tasks[second-1],0));while(emela_scheduler_run(1)) {}
  assert(count==4 && order==1 && !emela_fiber_done(f));
  assert(emela_task_complete_int(tasks[3],0));
  while(!emela_fiber_done(f)) { assert(emela_scheduler_run(1)); }
  assert(result==42 && order==3 && !emela_task_complete_int(tasks[1],0));emela_fiber_release(f);assert(test_heap_empty());
 }
}
