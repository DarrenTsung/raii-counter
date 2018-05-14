# raii-counter
Rust type for a RAII Counter (counts number of held instances, decrements count on `Drop`), implemented with `Arc<AtomicUsize>`.

Useful for tracking the number of holders exist for a handle, tracking the number of transactions that are in-flight, etc.

## Demo

```rust
extern crate raii_counter;
use raii_counter::Counter;

let counter = Counter::new();
assert_eq!(counter.count(), 1);

let weak = counter.downgrade();
assert_eq!(weak.count(), 0);

{
    let _counter1 = weak.spawn_upgrade();
    assert_eq!(weak.count(), 1);
    let _counter2 = weak.spawn_upgrade();
    assert_eq!(weak.count(), 2);
}

assert_eq!(weak.count(), 0);
```
