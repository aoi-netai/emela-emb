//! Inspect fixed heap reclamation without changing the generated public ABI.
use emela_emb_compiler::Output;

pub fn with_heap_audit(mut output: Output) -> Output {
    let source = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    assert!(source.contains("emela_heap_storage"));
    assert!(!source.contains("malloc(") && !source.contains("calloc("));
    source.push_str(
        r#"
int test_heap_empty(void) {
  if (!emela_heap_initialized) return 1;
  for (unsigned char *p=emela_heap_storage;p<emela_heap_end();) {
    emela_heap_block *block=(emela_heap_block *)(void *)p;
    if (!block->is_free || block->size == 0 || block->size > (size_t)(emela_heap_end()-p)) return 0;
    p+=block->size;
  }
  return 1;
}
"#,
    );
    output.c_source = source.clone();
    output
}
