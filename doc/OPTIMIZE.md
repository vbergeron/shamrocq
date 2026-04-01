# Performance analysis and roadmap

## Done

### Jump-table MATCH (bytecode v3)

MATCH uses a dense jump table indexed by `scrutinee_tag - base_tag`.
O(1) dispatch instead of O(n) linear scan.  Entry format is 3 bytes
(arity + offset) instead of 4 (tag + arity + offset).  Gap entries
point to an ERROR instruction.

### CALL_N / TAIL_CALL_N (bytecode v4)

When the compiler sees `App^N(Global(g), args)` where N equals the known
arity of g, it emits all N arguments followed by `CALL_N flat_entry N`.
The VM jumps directly to a flat entry point compiled with `frame_depth =
arity`, bypassing the curried closure chain entirely.  No PAP allocations,
no intermediate CALL1 dispatches.  Curried CALL1 remains for partial
application, unknown callees, and arity-1 functions.

### Compiler optimization passes

Fixed-point iteration over expr-level and resolved-level passes:
inline small globals, beta-reduce, constant fold, if→match,
dead binding elimination, case-of-known-ctor, eta-reduce.
See [CODEGEN.md](CODEGEN.md#3½-optimization-passes-pass) for details.

### Unified call stack

Frame headers (`saved_fb`, `saved_pc`, `saved_env`, `saved_heap_top`) are
stored on the arena operand stack rather than in a separate fixed-size
`[CallFrame; 256]` array.  This removes the hard 256-depth limit and
eliminates one data structure from the VM.

### Ctor arity headers

Heap-allocated constructors now carry a 1-word arity header before their
fields.  This makes the heap self-describing (enables `ctor_arity()` without
bytecode context) and is a prerequisite for heap traversal / GC.  Nullary
constructors remain immediate values — no overhead.

### Frame-local heap reclamation

Each frame header saves `heap_top` at call entry.  On return, if the result
value does not reference heap memory allocated during the call, the region
`[saved_heap, current_heap_top)` is reclaimed by resetting `heap_top`.

This is sound because the language is pure: older heap objects never contain
pointers to newer allocations.  The optimization is especially effective for
functions that build intermediate data structures (e.g. merge/dedup in
hforest) but return a value allocated before the call.

---

## Known issues

### 1. Residual Curried Overhead

CALL_N handles exact-arity calls to known globals, but several cases still
go through the curried CALL1 pipeline:

- **Partial application** — `(map f)` where `map` has arity 2.
- **Unknown callees** — higher-order calls like `(f x)` where `f` is a
  closure argument.
- **Over-application** — `(compose f g x)` where compose has arity 2 but
  receives 3 arguments.

Each of these creates PAP (Application) objects on the heap.  A GRAB/multi-arg
calling convention (see Roadmap) would handle all three.

### 2. No GC

The bump allocator never frees anything except via frame-local reclamation.
Frame-local reclamation only recovers memory when the return value has no
reference into the frame's heap region.  Functions that return a freshly
allocated result (the common case for constructors) do not benefit.

For deep recursion over large data structures, heap exhaustion remains the
primary failure mode.

### 3. Dispatch Overhead

The main loop is a `match opcode { ... }` with 30+ arms. The CPU branch
predictor sees a single indirect branch site for all opcodes, so it cannot
predict the next handler. Each iteration also checks `pc >= code.len()`.

### 4. Bounds Checking on Every Memory Access

`read_word` goes through `try_into().unwrap()` with slice bounds checks.
`stack_push` checks `heap_top + 4` every time. These are safe but costly
in the hot loop.

---

## Roadmap

### Tier 0: Low Effort

| Change | Impact | Effort |
|--------|--------|--------|
| **Remove `pc` bounds check** from the hot loop (trust bytecode, validate at load time) | Medium | Low |
| **`unsafe` aligned reads** for `read_word`/`write_word` on LE targets behind a `cfg` | Medium | Low |

### Tier 1: Full Multi-Arg Calling Convention (GRAB)

Phase 1 (CALL_N for exact-arity known calls) is done.  Remaining phases:

**Phase 2 — GRAB for unknown callees.**
Add `GRAB K` at the entry of all multi-arity functions.  CALL_N then works
with closures and higher-order calls, not just known globals.  GRAB handles
under-application (builds PAP) so the curried CALL1 path is no longer
needed for that case.

**Phase 3 — Over-application in RET.**
Extend the frame header with extra_args.  RET checks extra_args and
re-dispatches.  This handles cases like `(compose f g x)` where compose
has arity 2 but is called with 3 args.

### Tier 2: Interpreter Dispatch

| Change | Impact | Effort |
|--------|--------|--------|
| **Tail-call threaded dispatch** | High | Medium |
| **Superinstructions** (LOAD+CALL, PACK(0)+RET, MATCH+BIND, etc.) | Medium | Medium |

### Tier 3: Memory Management

| Change | Impact | Effort |
|--------|--------|--------|
| **Deep reclamation** — scan result fields recursively to reclaim more aggressively | Medium | Medium |
| **Copying/compacting GC** — semi-space collector fits the single-buffer model | Critical for real workloads | High |

### Tier 4: Aspirational

| Change | Impact | Effort |
|--------|--------|--------|
| **Register-based bytecode** | High | Very High |
| **Template JIT** | Very High | Very High |
| **NaN-boxing** on 64-bit host targets | Medium | Medium |

---

## Priority order

1. **GRAB / full multi-arg calls** (Tier 1 phases 2–3)
2. **Unsafe word access** (Tier 0)
3. **Deep reclamation or GC** (Tier 3)
4. **Threaded dispatch** (Tier 2)
