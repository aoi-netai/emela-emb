
int test_children(void) {
  int children = 0;
  if (!emela_heap_initialized) return 0;
  for (unsigned char *at = emela_heap_storage; at < emela_heap_end();) {
    emela_heap_block *block = (emela_heap_block *)(void *)at;
    if (!block->is_free && block->kind == EMELA_CHILD_KIND) children++;
    at += block->size;
  }
  return children;
}
void test_cancel(EmelaFiber h) { emela_fiber_get(h)->cancelled = 1; }
static emela_child *test_child(emela_scope *scope) {
  emela_child *child = emela_heap_allocate(sizeof(*child), EMELA_CHILD_KIND);
  *child = (emela_child){.scope = scope, .next = scope->children, .status = 1};
  if (scope->children != NULL) scope->children->previous = child;
  scope->children = child;
  emela_heap_retain(scope);
  return child;
}
void test_weak_child_links(void) {
  emela_scope *scope = emela_heap_allocate(sizeof(*scope), EMELA_SCOPE_KIND);
  *scope = (emela_scope){0};
  emela_child *tail = test_child(scope), *middle = test_child(scope), *head = test_child(scope);
  emela_heap_release(middle);
  if (scope->children != head || head->next != tail || tail->previous != head) emela_scheduler_invalid();
  emela_heap_release(head);
  if (scope->children != tail || tail->previous != NULL) emela_scheduler_invalid();
  emela_heap_release(tail);
  if (scope->children != NULL) emela_scheduler_invalid();
  tail = test_child(scope); head = test_child(scope);
  head->result.payload.any_box = tail;
  emela_heap_retain(tail);
  emela_heap_release(tail);
  scope->closed = 1;
  emela_scope_finish(scope);
  if (scope->children != NULL || head->result.payload.any_box != NULL) emela_scheduler_invalid();
  emela_heap_release(head);
  emela_heap_release(scope);
  scope = emela_heap_allocate(sizeof(*scope), EMELA_SCOPE_KIND);
  *scope = (emela_scope){0};
  tail = test_child(scope); head = test_child(scope);
  head->result.payload.any_box = tail; emela_heap_retain(tail);
  tail->result.payload.any_box = head; emela_heap_retain(head);
  emela_heap_release(tail); emela_heap_release(head);
  scope->closed = 1;
  emela_scope_finish(scope);
  if (scope->children != NULL) emela_scheduler_invalid();
  emela_heap_release(scope);
}
