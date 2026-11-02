#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask tasks[3];
static int marks,waits;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t,int32_t n) { assert(n==++waits);tasks[n-1]=t; }
void mark(int32_t n) { assert(n==++marks); }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
int test_heap_empty(void);
int main(void) {
 for(int i=0;i<1000;i++) {
  marks=waits=0;int32_t result=-1;
  EmelaFiber f=legacy_caught_start(i%2, &result);
  while(emela_scheduler_run(1)) {}
  if(i%2) {
   assert(waits==2 && marks==0 && !emela_fiber_done(f));
   assert(emela_task_complete_int(tasks[0],0));
   while(emela_scheduler_run(1)) {}
  }
  assert(waits==3 && marks==1 && result==-1 && !emela_fiber_done(f));
  assert(!emela_task_complete_int(tasks[0],99));
  assert(!emela_task_complete_int(tasks[1],99));
  assert(emela_task_complete_int(tasks[2],0));
  while(!emela_fiber_done(f)) assert(emela_scheduler_run(1));
  assert(result==42 && marks==3);emela_fiber_release(f);
 }
}
