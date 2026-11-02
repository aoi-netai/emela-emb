
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask task;
static int marks, waits;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t,int32_t n) { assert(n==++waits);task=t; }
void mark(int32_t n) { assert(n==++marks); }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
int main(void) {
 for(int i=0;i<1000;i++) {
  marks=waits=0;int32_t result=-1;
  EmelaFiber f=legacy_attempt_start(i%2,&result);
  while(emela_scheduler_run(1)) {}
  assert(waits==1 && marks==0);
  assert(emela_task_complete_int(task,42));
  while(emela_scheduler_run(1)) {}
  assert(waits==2 && marks==1 && result==-1);
  assert(emela_task_complete_int(task,0));
  while(!emela_fiber_done(f)) assert(emela_scheduler_run(1));
  assert(marks==3 && result==42);emela_fiber_release(f);assert(test_heap_empty());
 }
}
