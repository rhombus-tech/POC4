(module
  (memory (export "memory") 1)
  (func (export "test") (param i32 i32) (result i32 i32)
    (i32.const 0)  ;; ptr
    (i32.const 0)  ;; len
  )
)
