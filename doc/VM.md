# VM internals

This document describes the shamrocq runtime: how values are represented,
how memory is managed, and how the interpreter executes bytecode.

Source files: `crates/shamrocq/src/{arena,value,vm,stats}.rs`.

## Value representation

Every runtime value is a single `u32` word, split into three bit-fields:

```
 31  29  28        21  20                   0
┌──────┬────────────┬─────────────────────────┐
│ kind │    tag     │       payload           │
│  3b  │    8b     │        21b              │
└──────┴────────────┴─────────────────────────┘
```

| Kind | Bits 31:29 | Meaning |
|------|------------|---------|
| Ctor | `000` | Constructor — 8-bit tag, 21-bit word offset (0 for nullary) |
| Integer | `001` | Signed integer — tag bits extend payload to 29 bits |
| Bytes | `010` | Byte string — 8-bit length, 21-bit word offset to data |
| Closure | `110` | Heap-allocated closure — 21-bit word offset |
| Bare fn | `111` | Zero-capture function — code address in payload, no heap |

Offsets are stored as **word indices** (byte offset / 4). With 21 bits this
addresses up to 8 MB of heap, far beyond the target buffer sizes.

Integers use the 8 tag bits as an extension of the 21-bit payload, giving a
29-bit signed range (+/-268M).

This encoding keeps values register-sized and avoids pointer tagging tricks
that would break on non-32-bit targets.

### Hardcoded tags

The following tags are shared between the compiler and the VM and are always
available (defined in `value::tags`):

| Name | Value | Arity |
|------|-------|-------|
| `TRUE`  | 0 | 0 |
| `FALSE` | 1 | 0 |
| `NIL`   | 2 | 0 |
| `CONS`  | 3 | 2 |
| `O`     | 4 | 0 |
| `S`     | 5 | 1 |
| `LEFT`  | 6 | 1 |
| `RIGHT` | 7 | 1 |
| `PAIR`  | 8 | 2 |

Additional tags are assigned at compile time by the resolver's `TagTable`
and emitted in `ctors.rs`.

## Arena

The VM does zero dynamic allocation.  The caller provides a `&mut [u8]`
buffer; the arena partitions it into two regions that grow toward each other:

```
 0                              buf.len()
 ├──── heap ────►      ◄──── stack ────┤
 │ constructors, closures    │  values (LIFO) │
 └──────────────────────────────────────┘
       heap_top ──┘    └── stack_bot
```

- **Heap** grows upward from offset 0.  Bump-only: allocations never free.
  All allocations are 4-byte aligned.
- **Stack** grows downward from the end.  Each slot is one `u32` word (4 bytes).
- When `heap_top` meets `stack_bot` → `OutOfMemory`.
- `arena.reset()` reclaims everything (sets `heap_top = 0`,
  `stack_bot = buf.len()`).

### Heap objects

**Constructor** — `N` consecutive words (one per field):

```
offset+0:  field_0 (raw Value u32)
offset+4:  field_1
  ...
offset+4*(N-1): field_{N-1}
```

The tag lives in the `Value` pointer, not on the heap — zero overhead per
object.

**Closure** — header word + captured values:

```
offset+0:  code_addr:u16 << 16 | n_captures:u16
offset+4:  capture_0 (raw Value u32)
offset+8:  capture_1
  ...
```

## Execution model

### Globals

A program has up to 64 global slots.  On `load_program`, the VM evaluates
each global's code in declaration order (executing its initializer expression)
and stores the result in a fixed `[Value; 64]` array.

Most globals evaluate to closures, but a global can be any value (e.g. a
constructor constant).

### Stack frames

Each function call establishes a frame within the arena stack:

```
frame_base ──►  ┌─────────────┐
                │ capture_0   │  slot 0
                │ capture_1   │  slot 1
                │   ...       │
                │ capture_N-1 │  slot N-1
                │ param       │  slot N
                │ let_bind_0  │  slot N+1
                │   ...       │    (grows with BIND / let)
                └─────────────┘  ◄── stack top
```

`LOAD(idx)` reads slot `idx` counting from `frame_base` upward.

### Call stack

The VM maintains a Rust-side `[CallFrame; 256]` array (not in the arena)
storing `(return_pc, frame_base)` for each active non-tail call.

- `CALL`: saves the frame, increments depth, jumps.
- `TAIL_CALL`: truncates the current frame and reuses it — **no depth
  increase**, which is how recursive Scheme functions stay bounded.
- `CALL_DIRECT`: like `CALL` but the target code address and argument count
  are encoded in the bytecode. No function Value is on the stack and no
  closure unpacking is needed. Used for fully-applied calls to known
  multi-arity globals.
- `TAIL_CALL_DIRECT`: tail-position variant of `CALL_DIRECT`. Reuses the
  current frame like `TAIL_CALL`.
- `RET`: pops the result, restores `frame_base` and `pc`.  At depth 0,
  returns to the Rust caller.

Maximum call depth: **256**.  Exceeding it → `StackOverflow`.

### Tail call optimization

When the compiler sees an application in tail position, it emits
`TAIL_CALL` instead of `CALL`.  The VM truncates the current frame
(`stack_truncate(frame_base)`) and lays down the new captures + argument
in-place.  Since no `CallFrame` is pushed, tail-recursive loops use O(1)
call stack.

### Recursive closures (FIXPOINT)

`letrec` compiles to:

1. Push a dummy value (placeholder).
2. Compile the lambda — its captures include the `letrec` binding (de Bruijn
   index 0 from the lambda's perspective).
3. Emit `FIXPOINT(cap_idx)` — mutates `closure.captures[cap_idx]` to point
   to the closure itself, then replaces the dummy slot.

This is the **only mutable write** in the entire VM.  It avoids the need for
an indirection cell or trampoline.

### Pattern matching

`MATCH` pops the scrutinee and scans a case table by tag.  When matched:

- If arity > 0, the scrutinee is re-pushed, then `BIND(n)` destructures it
  into `n` field bindings on the stack.
- If arity = 0, execution continues directly — no stack manipulation.
- After the branch body, `SLIDE(n)` removes the bindings while keeping the
  result (non-tail only), and `JMP` skips remaining cases.

In tail position, branches emit `RET` / `TAIL_CALL` directly — no `SLIDE`
or `JMP` needed.

## Rust API

### Setup

```rust
let mut buf = [0u8; 65536];
let prog = Program::from_blob(bytecode).unwrap();
let mut vm = Vm::new(&mut buf);
vm.load_program(&prog).unwrap();
```

### Calling functions

```rust
// Direct call by global index (curried — applies one arg at a time):
let result = vm.call(funcs::ADD, &[n2, n3]).unwrap();

// Apply a closure value:
let negb_closure = vm.global_value(funcs::NEGB);
let result = vm.apply(negb_closure, &[val]).unwrap();
```

### Constructing and inspecting data

```rust
// Allocate a tagged constructor (e.g. cons cell):
let pair = vm.alloc_ctor(tags::CONS, &[head, tail]).unwrap();

// Read fields:
let head = vm.ctor_field(pair, 0);
let tail = vm.ctor_field(pair, 1);

// Nullary constructors need no allocation:
let nil = Value::ctor(tags::NIL, 0);

// Integers: Value::integer(n) creates a value; integer_value() extracts it:
let n = Value::integer(42);
let x = n.integer_value();
```

### Memory management

```rust
// Snapshot current usage:
let snap = vm.mem_snapshot();
// -> "heap   1234 B | stack    456 B | free  63346 B"

// Reclaim all arena memory between computations:
vm.reset();
```

## Error modes

| Error | Cause |
|-------|-------|
| `Oom` | Heap allocation or stack push would overlap the other region |
| `StackOverflow` | Call depth exceeds 256 |
| `MatchFailure { tag, pc }` | No case in a `MATCH` table matches the scrutinee tag |
| `NotAClosure` | `CALL` / `TAIL_CALL` target is not a closure value |
| `IndexOutOfBounds` | Byte string index out of range |
| `BytesOverflow` | Byte string concatenation would exceed 255 bytes |
| `InvalidBytecode` | Blob too short, PC out of bounds, or malformed header |

## Stats (feature `stats`)

When compiled with `--features stats`, the VM records:

| Counter | Description |
|---------|-------------|
| `peak_heap_bytes` | High-water mark of heap usage |
| `peak_stack_bytes` | High-water mark of stack usage |
| `alloc_count_ctor` | Total constructor allocations |
| `alloc_count_closure` | Total closure allocations |
| `alloc_bytes_total` | Total bytes allocated on the heap |
| `exec_instruction_count` | Total instructions executed |
| `exec_call_count` | Non-tail `CALL` count |
| `exec_tail_call_count` | `TAIL_CALL` count |
| `exec_direct_call_count` | Non-tail `CALL_DIRECT` count |
| `exec_tail_direct_call_count` | `TAIL_CALL_DIRECT` count |
| `exec_match_count` | `MATCH` dispatch count |
| `exec_peak_call_depth` | Deepest call stack reached |

Access via `vm.stats` (the `Stats` struct) and `vm.mem_snapshot()` (live
heap/stack/free snapshot, available without the feature).
