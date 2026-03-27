# Bytecode format

This document describes the binary format produced by `shamrocq-compiler` and
consumed by the shamrocq VM at runtime.

All multi-byte integers are **little-endian**.

## Blob layout

A compiled blob is a single contiguous byte array split into two sections:

```
┌───────────────────────┐
│  Header               │  global table (variable length)
├───────────────────────┤
│  Code                 │  instruction stream
└───────────────────────┘
```

### Header

```
n_globals : u16le            number of top-level defines

For each global (repeated n_globals times):
  name_len : u8              length of the name in bytes
  name     : [u8; name_len]  UTF-8 name (e.g. "negb", "append")
  offset   : u16le           byte offset into the Code section
```

The index of a global in this table is its **slot index**, used by `GLOBAL`
instructions and by the Rust-side `funcs::` constants.

### Code

A flat byte stream of instructions starting immediately after the header.
All code offsets (jump targets, closure addresses) are absolute byte positions
within this section.

## Instruction set

Each instruction is a 1-byte opcode followed by zero or more inline operands.

### `IMM` (0x01) — push immediate

```
01 tag:u8
```

Push a nullary constructor value onto the stack.  No heap allocation.

### `TUPLE` (0x02) — allocate tagged tuple

```
02 tag:u8 arity:u8
```

Pop `arity` values from the stack (top = last field), heap-allocate a tuple
with the given tag, push the result.

### `LOAD` (0x03) — load local

```
03 idx:u8
```

Copy the value at frame slot `idx` and push it.  Slot 0 is the bottom of the
current frame (first capture or parameter).

### `GLOBAL` (0x04) — load global

```
04 idx:u16le
```

Push the already-evaluated value of global slot `idx`.

### `CLOSURE` (0x05) — allocate closure

```
05 code_addr:u16le n_captures:u8
```

Pop `n_captures` values from the stack (top = last capture), heap-allocate a
closure object pointing to `code_addr` with those captures, push the result.

Heap layout of a closure:

```
word 0:  code_addr:u16 << 16 | n_captures:u16
word 1…: capture values (raw u32 each)
```

### `APPLY` (0x06) — non-tail call

```
06
```

Pop `arg`, pop `func`.  Save `(return_pc, frame_base)` on the call stack.
Unpack the closure's captures and `arg` into a new frame, jump to the
closure's code address.

### `TAIL_APPLY` (0x07) — tail call

```
07
```

Pop `arg`, pop `func`.  **Truncate** the current frame (reuse call depth).
Unpack captures and `arg` into the recycled frame, jump.  No call-stack
growth — this is how tail recursion stays bounded.

### `RET` (0x08) — return

```
08
```

Pop the result, discard the current frame.  If call depth is zero, return to
the Rust caller.  Otherwise restore `(return_pc, frame_base)` from the call
stack and push the result in the caller's frame.

### `MATCH` (0x09) — tag dispatch

```
09 n_cases:u8
   [tag:u8 arity:u8 offset:u16le] × n_cases
```

Pop the scrutinee.  Scan the case table for a matching tag.  If `arity > 0`,
re-push the scrutinee (for a subsequent `BIND`).  Jump to `offset`.

If no case matches → `MatchFailure`.

### `JMP` (0x0A) — unconditional jump

```
0A offset:u16le
```

Set `pc = offset`.  Used after non-tail match branches to skip remaining
cases.

### `BIND` (0x0B) — destructure tuple

```
0B n:u8
```

Pop the scrutinee tuple, push its first `n` fields (field 0 first).  This
makes constructor fields available as local bindings after a `MATCH`.

### `DROP` (0x0C) — discard stack slots

```
0C n:u8
```

Remove the top `n` values from the stack.

### `SLIDE` (0x0E) — keep result, drop bindings

```
0E n:u8
```

Pop the result, drop `n` values below it, re-push the result.  Used to clean
up `let` and `match` bindings in non-tail position.

### `ERROR` (0x0D) — abort

```
0D
```

Halt with `MatchFailure(tag=0xFF)`.  Emitted for exhaustiveness-checked match
arms that should be unreachable.

### `FIXPOINT` (0x0F) — tie recursive knot

```
0F cap_idx:u8
```

Peek the closure at TOS.  If `cap_idx != 0xFF`, mutate
`closure.captures[cap_idx]` to point to the closure itself (self-reference).
Then overwrite slot 1 (the `letrec` dummy) with the closure and pop TOS.

This is the only mutation in the entire VM — it makes `letrec` work without
a GC or indirection cell.

## Opcode summary

| Mnemonic     | Hex    | Operands                                   | Size |
|--------------|--------|--------------------------------------------|------|
| `IMM`        | `0x01` | `tag:u8`                                   | 2    |
| `TUPLE`      | `0x02` | `tag:u8 arity:u8`                          | 3    |
| `LOAD`       | `0x03` | `idx:u8`                                   | 2    |
| `GLOBAL`     | `0x04` | `idx:u16le`                                | 3    |
| `CLOSURE`    | `0x05` | `code_addr:u16le n_captures:u8`            | 4    |
| `APPLY`      | `0x06` | —                                          | 1    |
| `TAIL_APPLY` | `0x07` | —                                          | 1    |
| `RET`        | `0x08` | —                                          | 1    |
| `MATCH`      | `0x09` | `n:u8 [tag:u8 arity:u8 off:u16le]*n`       | 2+4n |
| `JMP`        | `0x0A` | `offset:u16le`                             | 3    |
| `BIND`       | `0x0B` | `n:u8`                                     | 2    |
| `DROP`       | `0x0C` | `n:u8`                                     | 2    |
| `ERROR`      | `0x0D` | —                                          | 1    |
| `SLIDE`      | `0x0E` | `n:u8`                                     | 2    |
| `FIXPOINT`   | `0x0F` | `cap_idx:u8`                               | 2    |

## Generated companion files

The compiler (both the CLI and `build.rs`) also emits two Rust source files
alongside `bytecode.bin`:

- **`funcs.rs`** — one `pub const NAME: u16 = idx;` per global, mapping
  function names to their slot index.
- **`ctors.rs`** — one `pub const NAME: u8 = id;` per Scheme-defined
  constructor tag.

These are meant to be `include!`'d in the consuming crate so that call sites
use symbolic names (`funcs::APPEND`) rather than raw integers.
