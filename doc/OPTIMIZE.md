# Memory optimization

This document evaluates techniques to reduce runtime memory consumption in
shamrocq, drawing from three reference small-Lisp implementations:
**TinyScheme**, **PicoLisp**, and **FemtoLisp**.

## Current memory layout

The VM operates in a single caller-provided buffer (target: 30–64 KB):

```
┌──────────────────────────────────────────────┐
│  Heap ──▶                    ◀── Stack       │
│  (bump alloc, grows up)      (grows down)    │
└──────────────────────────────────────────────┘
```

- **Heap**: tuples and closures, bump-allocated, never freed.
- **Stack**: operand stack for the bytecode interpreter.
- **Globals**: `[Value; 64]` — 256 B, fixed, on the Rust stack.
- **Call stack**: `[CallFrame; 256]` — 2 KB on Cortex-M4, on the Rust stack.

### Value representation

Every value is a 32-bit tagged word:

| Kind (bits 31–30) | Layout | Heap cost |
|--------------------|--------|-----------|
| `00` — Immediate | 6-bit tag, 24-bit unused | 0 |
| `01` — Tuple | 6-bit tag, 24-bit heap offset | `arity × 4` B |
| `10` — Closure | 30-bit heap offset | `(1 + n_captures) × 4` B |
| `11` — Bare fn | 16-bit code address | 0 |

Tuples store no header on the heap — the tag and arity come from the
bytecode / the `Value` word itself.  Closures store a 4-byte header
(`code_addr:u16 | n_captures:u16`) followed by capture values.

### Where memory goes

Stats from the integration tests reveal that **closures dominate the heap**:

| Test | Tuples | Closures | Total heap |
|------|--------|----------|------------|
| `load_program` | 2 | 63 | 260 B |
| `hforest_merge_basic` | 26 | 193 | 1 748 B |
| `hforest_lifecycle` | 92 | 664 | 6 576 B |
| `tree_to_list_sorted` | 32 | 125 | 1 044 B |

The closure-to-tuple ratio is 3:1 to 8:1.  This is a direct consequence of
the fully-curried calling convention: `(lambdas (a b c) body)` desugars into
`Lambda(Lambda(Lambda(body)))`, and each partial application allocates a new
closure with its own copy of captures.

The heap is bump-only — every intermediate closure and tuple produced during
computation stays on the heap forever, even after becoming unreachable.

---

## Reference implementations

### TinyScheme

- **Value representation**: tagged-union `cell` — a `_flag` word (5-bit type +
  GC/immutability bits) and a union holding `{car, cdr}`, `{string_ptr, len}`,
  a `num`, or a `foreign_func`.  ~24 B per cell on 64-bit.
- **GC**: mark-and-sweep with **Schorr–Deutsch–Waite link-inversion** for the
  mark phase — traverses the object graph in **O(1) auxiliary stack space** by
  temporarily reversing pointers.
- **Closures**: a cons cell tagged `T_CLOSURE` — `car` = code, `cdr` =
  environment chain.  Maximally compact: 2 pointers per closure, environments
  are shared chains.
- **Symbols**: interned in a hash table, no duplication.
- **Notable trick**: `get_consecutive_cells` finds N adjacent free cells in the
  free list for vector allocation.

### PicoLisp

- **Value representation**: everything is a **2-word cell** (CAR + CDR).  Type
  information is encoded in the **low 4 bits of pointers** (all cells are
  even-aligned).  No union, no type field, no separate header.
- **GC**: mark-and-sweep using **bit 0 of CDR** as the mark bit (stolen from
  the pointer field).  No separate mark bitmap needed.
- **Closures/environments**: PicoLisp uses **dynamic binding** with an explicit
  bind-frame stack.  No closures in the traditional sense — avoids all closure
  allocation overhead entirely.
- **Heap**: linked blocks of fixed-size cells.  Free cells form an intrusive
  free list via CAR.  Uniform cell size → zero fragmentation, trivial
  allocator.

### FemtoLisp

- **Value representation**: **3-bit tagged pointers** (not NaN-boxing).
  `value_t` is a machine word.  Fixnums exploit `x & 3 == 0` (both `0b000`
  and `0b100`), giving **30-bit signed integers** with zero heap cost.
- **Closures**: a function object is exactly **4 words**: `{bcode, vals, env,
  name}`.  The `env` is a captured environment shared by all closures from the
  same scope.
- **GC**: **Cheney-style copying/semispace collector** — copies all live data
  from `fromspace` to `tospace` via `relocate()`, installs forwarding
  pointers.  Automatically compacts; zero fragmentation after GC.
- **Stack**: separate `value_t` array with explicit `SP` and `curr_frame`.
  Frame metadata is pushed directly onto the value stack.
- **Notable tricks**:
  - `leafp(a) = (a & 3) != 3` — O(1) "no heap references" test, skips GC
    recursion.
  - `cons_reserve(n)` — batch-allocates N cons cells in one bump.
  - `alloc_words(n)` always aligns to 2-word boundaries.

---

## Optimization opportunities

### Implemented

#### ~~1. Zero-capture closures as bare function pointers~~ ✓

Kind `0b11` encodes a bare function pointer: the code address is stored
directly in the `Value` word with no heap allocation.  The existing `CLOSURE`
bytecode with `n_captures=0` produces a bare-fn value at runtime — no
compiler changes were needed.

Measured impact (before → after):

| Test | Heap before | Heap after | Savings |
|------|-------------|------------|---------|
| `load_program` | 260 B | 8 B | 97% |
| `negb(true)` | 260 B | 8 B | 97% |
| `compose(negb,negb)` | 276 B | 28 B | 90% |
| `tree_insert_and_member` | 1 072 B | 820 B | 23% |
| `tree_to_list_sorted` | 1 124 B | 872 B | 22% |
| `hforest_merge_basic` | 1 748 B | 1 500 B | 14% |
| `hforest_lifecycle` | 6 576 B | 6 328 B | 4% |

Biggest win is at program load (63 zero-capture globals) and for simple
function calls.  Heavier workloads see 14–23% reduction from zero-capture
closures created during partial application.

### Rejected

#### ~~Arena watermarking / checkpoint-reset~~

Watermarking (save `heap_top`, reset after a call) was investigated and
rejected.  The simple case — result is immediate, just reset — only helps
trivial boolean predicates.  The heavy workloads (map, merge, forest
operations) all return heap-allocated structures, requiring full relocation.

Relocation is blocked by a structural issue: **tuples have no heap header**.
A tuple on the heap is just its fields — no length word, no tag.  The arity
was known at compile time from the `TUPLE tag arity` bytecode operand but is
not stored anywhere at runtime.  This means:

- **Can't determine tuple size** from a heap offset alone — needed to copy it.
- **Can't place forwarding pointers** — no header word to overwrite, so shared
  references would be duplicated (increasing memory) or require an external
  side table.
- **Self-referential closures** (`FIXPOINT`) create cycles that require
  forwarding pointers to handle correctly.

Fixing this would require adding a header word to every tuple (+4 B each),
building a compile-time tag→arity table, or encoding arity in the Value word.
Each option partially negates the savings and constitutes a significant design
change.  Furthermore, watermarking only works at `vm.call()` boundaries —
internal recursion garbage within a single call is not reclaimable.

### Tier 1 — high impact

#### 1. Uncurried / multi-argument closures

The most structurally impactful change.  Currently
`(lambdas (a b c) body)` → `Lambda(a, Lambda(b, Lambda(c, body)))`.  Calling
with 3 args produces 2 intermediate closures that are immediately consumed:

```
Closure(Lambda(b, Lambda(c, body)), captures=[a])   → 2 words
Closure(Lambda(c, body), captures=[a, b])            → 3 words
                                                total: 5 words wasted
```

With multi-argument closures:

```
CLOSURE code_addr n_captures arity
APPLY_N arity
```

A 3-argument function is a single closure.  No intermediate closures.

Estimated savings: 50–70% of closure allocations for curried code.

Requires: desugar, codegen, and VM changes (new opcodes `CLOSURE` with arity,
`APPLY_N`).

#### 2. Shared closure environments (FemtoLisp-style)

When sibling closures from the same scope capture the same variables, each
independently copies all captures:

```
Current:   Closure_A [hdr|c0|c1|c2] + Closure_B [hdr|c0|c1|c2]  = 8 words
Shared:    Env [c0|c1|c2] + Closure_A [hdr|env] + Closure_B [hdr|env]  = 7 words
```

In `fourchette.scm`, many functions create 2–4 lambdas that all capture `h`
(the ordering function).  Savings scale with capture count × number of sibling
closures.

Requires: compiler grouping of sibling lambdas, new env-block allocation, VM
env-pointer dereference.

### Tier 2 — medium impact

#### 3. Inline unary constructors

`S(n)` costs 4 bytes on the heap for one field.  When the field is itself an
immediate (like `S(O)`), the entire value fits in 32 bits with no heap
allocation.  Reserve a bit to distinguish "payload is heap offset" from
"payload is inline immediate":

```
0b01_tttttt_0_ppppppppppppppppppppppp   heap offset (23 bits → 8 MB)
0b01_tttttt_1_iiiiiiiiiiiiiiiiiiiiiii   inline immediate field (23 bits)
```

Makes Peano numbers up to ~depth 23 free (no heap), and `Some(x)`, `Left(x)`,
`Right(x)` with immediate payloads also free.

Requires: value encoding change, VM BIND/tuple_field adjustment.

#### 4. `GLOBAL_CALL` — avoid materializing closures for known targets

Many call patterns are `GLOBAL idx; LOAD arg; APPLY`.  A combined
`GLOBAL_CALL idx` opcode could jump directly to the global's code without
pushing the closure, reading it back, and unpacking captures.  Saves stack
traffic and avoids intermediate closure manipulation.

Requires: new opcode, codegen pattern detection.

#### 5. In-arena call frames (FemtoLisp-style)

The `[CallFrame; 256]` array is 2 KB on Cortex-M4.  Pushing `return_pc` and
`frame_base` as raw `u32` values onto the arena stack instead would:

- Eliminate the fixed 2 KB overhead.
- Make the call depth limit dynamic (bounded by available arena space).
- Unify all runtime state into the single buffer.

Requires: VM refactor of call/return paths.

### Tier 3 — lower impact / exploratory

#### 6. Lambda lifting (compiler pass)

Lift lambdas with few free variables to top-level functions with extra
parameters.  The caller passes captures as arguments.  Trades heap allocation
(permanent in bump model) for stack space (temporary) — almost always
favorable given bump-only allocation.

#### 7. Stack-allocated temporary tuples

Tuples that are immediately destructured by `MATCH`/`BIND` and never escape
could stay on the stack instead of the heap.  Requires escape analysis in the
compiler.

#### 8. Word-granularity heap offsets

All allocations are 4-byte aligned, but offsets are byte-addressed.  Storing
`offset / 4` doubles the effective range.  For a 64 KB buffer, 14 bits
suffice, freeing bits for inline payloads (supports technique 5).

---

## Recommended implementation order

| Priority | Technique | Complexity | Estimated savings |
|----------|-----------|------------|-------------------|
| ~~done~~ | ~~Zero-capture closures as bare fns~~ | ~~Low~~ | ~~97% load, 14–23% heavy~~ |
| 1 | Uncurried calling convention | High | ~50–70% fewer closure allocs |
| 2 | Inline unary constructors | Medium | Significant for Peano-heavy code |
| 3 | Shared environments | Medium | Good for HOF-heavy code |

The uncurried calling convention addresses the root cause of closure
proliferation but requires changes across the entire compiler pipeline and
VM instruction set.  Inline unary constructors and shared environments are
independent medium-complexity changes that can be done in any order.
