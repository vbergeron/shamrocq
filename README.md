# Shamrocq

<p align="center">
  <img src="assets/logo.png" alt="Shamrocq logo" width="300">
</p>

**Scheme + Rocq = shamrock**

A minimal, `no_std` Scheme interpreter designed to run
[Rocq](https://rocq-prover.org/) (Coq) extracted code on bare-metal
microcontrollers.

## Target

- STM32 family, Cortex-M4
- No libc, no dynamic allocation
- Memory budget: 30–64 KB (BYOB — Bring Your Own Buffer)

The caller provides a mutable byte slice; the VM does all allocation inside it
via a bump allocator.  Computations are finite data transformations — no
long-running tasks.

## Architecture

```
scheme/              Scheme sources (Rocq extractions + synthetic tests)
crates/
  shamrocq-compiler/ Build-time compiler: parse → desugar → resolve → ANF → codegen
  shamrocq/          no_std runtime: arena, value representation, bytecode VM
```

### Compiler pipeline

1. **Parser** — S-expression reader
2. **Desugarer** — expands `lambdas`, `@`, `quasiquote`/`unquote`, `match`
3. **Resolver** — de Bruijn indexing, constructor tag interning
4. **ANF normalizer** — lifts complex arguments into `let` bindings
5. **Codegen** — emits a compact bytecode blob

The compiler is available both as a library (used by `build.rs` to embed
bytecode at compile time) and as a standalone CLI.

### Runtime

- **Values** are tagged 32-bit words: constructors, integers, byte strings,
  closures, and bare function pointers
- **Heap** uses bump allocation inside the caller-provided buffer
- **Stack** grows downward from the other end of the same buffer
- **Closures** capture by value; recursive closures use `FIXPOINT`
- **Direct calls** bypass the curried closure chain for known multi-arity globals
- **Match** dispatches on constructor tags with `BIND`/`SLIDE` for field access

### Generated modules

`build.rs` produces two Rust modules from the compiled Scheme:

- `funcs` — global function indices (`funcs::APPEND`, `funcs::NAT_ORD`, …)
- `ctors` — constructor tag constants (`ctors::SOME`, `ctors::LEAF`, …)

Core tags (`True`, `False`, `Nil`, `Cons`, `O`, `S`, …) live in
`value::tags`.  Scheme-defined constructors get their constants generated
automatically — no manual registration needed.

## Fit for Rocq extraction

Rocq's extraction to Scheme produces **pure, finite, first-order functional code**: curried
lambdas, inductive types encoded as constructors, recursive functions, and pattern matching.
Shamrocq is built around exactly this profile and nothing more.

- **Constructor-based ADTs.** Rocq inductive types extract to constructor calls.  Shamrocq
  represents every value as a 32-bit tagged word — an 8-bit constructor tag plus heap-allocated
  fields — and dispatches on tags with a single `MATCH` instruction.  No symbol interning, no
  hash lookup, no boxing overhead.
- **Curried application.** Extraction emits multi-argument functions as nested lambdas.
  Shamrocq's `@` sugar and the direct-call optimisation (see below) handle this natively,
  bypassing the closure chain for fully-applied known globals and reducing instruction count by
  20–80 % on typical extracted functions.
- **Proper tail calls.** Rocq's structural recursion maps directly to Scheme tail recursion.
  The VM implements `TAIL_CALL` and `TAIL_CALL_DIRECT` instructions that reuse the current
  frame, so all recursive patterns — `fold_left`, `merge_sorted`, tree traversal — run in
  O(1) stack space.
- **Deterministic, finite heaps.** Extracted Rocq programs are pure functions: given fixed
  inputs they allocate a fixed amount of memory and terminate.  Shamrocq exploits this
  directly: a single caller-provided buffer is split between heap (bump-up) and stack
  (bump-down), with no garbage collector.  Between calls the arena resets in O(1).  There
  are never GC pauses, never stop-the-world events, never non-deterministic latency.
- **Byte strings for `ExtractionString`.** Rocq string extraction produces flat byte arrays;
  shamrocq's `bytes` value type covers `bytes-len`, `bytes-get`, `bytes-eq`, and `bytes-cat`
  directly, without a full string library.

Compared to general-purpose Scheme VMs, the design choices are deliberately narrow:

- **TinyScheme / FemtoLisp / Guile** ship a mark-and-sweep or copying GC, a symbol table,
  `call/cc`, a full numeric tower, ports, and a macro expander.  Combined flash footprint
  typically exceeds 50–100 KB.  Shamrocq's VM fits in ~12 KB of `.text` with ~3 KB of
  bytecode — 4–8× smaller — and requires no libc.
- **No fragmentation.** Bump allocation never produces holes.  The heap layout after a call
  is identical every time for identical inputs.
- **`no_std` by construction.** The runtime has zero external dependencies and no `unsafe`
  allocator trait — it can be linked into any bare-metal Cortex-M firmware without an OS or
  heap region.
- **Single representation.** All values are 32-bit words.  There is no NaN-boxing, no
  pointer tagging with alignment tricks, no 64-bit pointers.  The 32-bit layout fits
  Cortex-M registers directly.

## Limitations

Shamrocq is deliberately not a general-purpose Scheme.  The following are fixed constraints,
not planned features:

- **No garbage collector.** Allocation is bump-only and monotonic within a call.  If the
  buffer fills up the VM returns `VmError::Oom`; the caller must size the buffer for the
  expected working set.  There is no way to reclaim memory mid-call.
- **29-bit integers.** Values are 32-bit tagged words; integers occupy 29 bits
  (-268 435 456 … +268 435 455, wrapping).  Arbitrary-precision arithmetic is not supported.
- **Byte strings capped at 255 bytes.** The length occupies 8 bits of the value word.
  Concatenation that would exceed 255 bytes returns `VmError::BytesOverflow`.
- **No `call/cc` or continuations.** The stack is a plain downward-growing region of the
  arena; capturing it is not possible with the bump model.
- **No mutation.** The only write after initial construction is `FIXPOINT`, which patches a
  single capture slot of a freshly-created closure.  There is no `set!` and no mutable cells.
- **No macros.** Reader macros (`'`, `` ` ``, `,`) are hardcoded in the parser.  User-defined
  macros and `syntax-rules` are not supported.
- **Non-tail call depth capped at 256.** Only tail calls are free; non-tail mutual recursion
  or deep function composition can hit `VmError::StackOverflow`.
- **Bytecode is not sandboxed.** The VM trusts that the bytecode blob is well-formed.
  Malformed or hand-crafted bytecode (invalid jump targets, malformed `MATCH` tables) is
  not fully bounds-checked and may produce incorrect results.  Only use bytecode produced
  by the in-tree compiler.

## Usage

### 1. Compile Scheme to bytecode

```sh
cargo install --path crates/shamrocq-compiler

shamrocq-compiler -o out/ mylib.scm helpers.scm
```

```
compiled 12 globals, 8 ctors, 1024 bytes of bytecode from 2 files
  -> out/bytecode.bin
  -> out/funcs.rs
  -> out/ctors.rs
```

| Output file | Contents |
|-------------|----------|
| `bytecode.bin` | Compiled bytecode image |
| `funcs.rs` | `pub const` for each global function index |
| `ctors.rs` | `pub const` for each constructor tag ID |

Run `shamrocq-compiler --help` for all options.

### 2. Embed in your `no_std` project

Include the generated files in your firmware crate:

```rust
static BYTECODE: &[u8] = include_bytes!("path/to/bytecode.bin");

mod funcs {
    include!("path/to/funcs.rs");
}

mod ctors {
    include!("path/to/ctors.rs");
}
```

Then load and run:

```rust
use shamrocq::{Program, Vm, Value, tags};

let mut buf = [0u8; 65536];
let prog = Program::from_blob(BYTECODE).unwrap();
let mut vm = Vm::new(&mut buf);
vm.load_program(&prog).unwrap();

let result = vm.call(funcs::NEGB, &[Value::ctor(tags::TRUE, 0)]).unwrap();
assert_eq!(result.tag(), tags::FALSE);
```

### In-tree development

Within this repo, enabling the `integration` feature compiles all `.scm` files
under `scheme/` via `build.rs` and embeds the result, so the generated `funcs`
and `ctors` modules are available directly from the `shamrocq` crate.
This feature is **not** enabled by default — downstream users bring their own
compiled bytecode via the CLI.

## Footprint

Compiled sizes (release, `thumbv7em-none-eabihf`):

| Component | Size |
|-----------|------|
| VM runtime (`.text`) | ~12 KB |
| Bytecode (`hash_forest.scm`, 31 globals) | ~3 KB |
| **Total flash** | **~15 KB** |

## Optional features

- **`integration`** — compiles the in-tree `scheme/` files via `build.rs` and
  embeds the bytecode, `funcs`, and `ctors` modules. Only needed for running
  the repo's own integration tests; downstream users compile their own Scheme
  via the CLI.
- **`stats`** — enables `vm.stats` and `vm.mem_snapshot()` for tracking
  peak heap/stack usage, allocation counts, instruction counts, and call depth.

## Tests

```sh
cargo test --features integration                  # without stats
cargo test --features integration,stats            # with memory/execution statistics printed
```

## License

MIT
