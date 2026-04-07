## Missing Passes vs. SOTA Scheme Compilers

Comparison of shamrocq's pass pipeline against three reference compilers: **Chez Scheme** (widely considered the fastest), **Guile** (GNU's CPS-based compiler), and **Chicken Scheme** (CPS-to-C). Gaps are ordered by likely impact for our use case (Coq-extracted code on bare-metal).

---

### 1. Copy Propagation

**What it does:** Replaces `let x = y in ...x...` with `...y...` when `x` is a simple variable alias.

**Who has it:** Chez Scheme's entire source optimizer is named **CP0** — *Copy Propagation pass 0* — because this is so fundamental. It subsumes inlining, constant folding, and dead code elimination under a single cost-model-driven traversal ([cp0.ss, lines 36-58](https://github.com/dybvig/ChezScheme/blob/0747a5d5/s/cp0.ss#L36-L58)). GRIN compiler and Futhark also implement it as a standalone pass.

**Why it matters for shamrocq:** After beta-reduction (`p01`), you often get chains like `let x = y in let z = x in ...z...`. Your dead-binding pass (`p04`) removes *unused* bindings but doesn't propagate *trivial* ones. Coq extraction produces a lot of administrative lets that become copy chains after inlining.

---

### 2. Contification

**What it does:** Identifies functions that always return to the same continuation (caller) and converts them into direct jumps (gotos/loops) rather than closure calls.

**Who has it:** Guile's manual explicitly states: *"Contification is the major optimization pass performed on CPS"* — it converts procedure calls into `goto` and transforms recursive function nests into loops ([Guile Ref Manual, Compiling CPS](https://www.gnu.org/software/guile/manual/html_node/Compiling-CPS.html)). The foundational paper is Fluet & Weeks, *"Contification Using Dominators"* (ICFP 2001), which proves an optimal algorithm and shows significant runtime improvements on realistic programs.

**Why it matters for shamrocq:** Coq extraction generates many helper functions that are only ever called from one place (e.g., match eliminators, fixpoint combinators). Without contification, each becomes a closure + `CALL_DYNAMIC`. With contification, they'd become inline jumps — no allocation, no call overhead. This is especially valuable on your bare-metal target where the arena is constrained.

**Implementation status:** Level 1 implemented (`p04b_contify`): single-use call-only `Let`-bound lambdas are inlined with immediate beta-reduction. Pass is disabled pending Coq-extracted test inputs. Infrastructure in place: `debruijn::analyze_binding` collects `BindingUsage { ref_count, call_only }` in a single walk, `debruijn::let_subst` handles substitution. Level 2 (multi-use call-only → `LetCont`/`JumpCont` IR nodes) and Level 3 (recursive → loops) not yet implemented.

---

### 3. Common Subexpression Elimination (CSE)

**What it does:** Identifies expressions computed multiple times with the same operands and reuses the first result.

**Who has it:** Guile performs CSE on its CPS representation ([see Andy Wingo's blog: "revisiting common subexpression elimination in guile"](https://wingolog.org/archives/2014/08/25/revisiting-common-subexpression-elimination-in-guile)). Chez Scheme achieves similar effects through CP0's environment tracking of known values.

**Why it matters for shamrocq:** After inlining and beta-reduction, the same sub-expression may appear at multiple points. Without CSE, each is evaluated independently, wasting both instructions and stack space.

---

### 4. Singly-Referenced Variable Optimization (Use-Count Analysis)

**What it does:** Tracks how many times each variable is referenced. Variables used exactly once can always be inlined regardless of size. Variables used zero times are dead.

**Who has it:** Chez Scheme's CP0 explicitly tracks whether variables are "multiply-referenced" and more aggressively inlines singly-referenced bindings ([cp0.ss, lines 36-58](https://github.com/dybvig/ChezScheme/blob/0747a5d5/s/cp0.ss#L36-L58)). This is strictly more powerful than shamrocq's size-based inlining heuristic (`AST size ≤ 5`).

**Why it matters for shamrocq:** Your `p00_inline` uses a flat size threshold of 5 nodes. A function body of size 100 that's called exactly once should still be inlined — it doesn't increase code size. Use-count analysis would unlock this.

---

### 5. Loop Peeling and Invariant Code Motion

**What it does:** Peels the first iteration of a loop so that values known on the first entry (but not in general) can be optimized. Invariant code motion hoists expressions that don't change across loop iterations.

**Who has it:** Guile implements both `peel-loops` and invariant code motion as explicit CPS passes ([Andy Wingo, "loop optimizations in guile"](https://wingolog.org/archives/2015/07/28/loop-optimizations-in-guile)). These interact with CSE: peeling exposes constants that CSE can then propagate.

**Why it matters for shamrocq:** Coq-extracted code uses recursive functions for loops. A `letrec` body that builds a constructor on every iteration with a constant tag could have the tag allocation hoisted out. This is modest for your current workloads but becomes significant for tight numeric loops (sort, tree traversals).

---

### 6. Type / Range Inference

**What it does:** Propagates type and range information (e.g., "this is definitely a `Cons`", "this integer is in [0, 255]") to enable type-specialized code paths.

**Who has it:** Chez Scheme has a dedicated `cptypes.ss` pass for type-related optimizations. Guile performs "range and type inference" on CPS ([Guile Ref Manual](https://www.gnu.org/software/guile/manual/html_node/Compiling-CPS.html)).

**Why it matters for shamrocq:** With type information, you could eliminate redundant match dispatches when the tag is statically known from a dominating branch, or specialize integer operations. Your `p05_case_known_ctor` handles the trivial case where the scrutinee is a literal `Ctor`, but a flow-sensitive analysis could propagate tag knowledge through let-bindings and across branches.

---

### 7. Lambda Lifting / Closure Optimization

**What it does:** Converts closures that capture variables into top-level functions with extra parameters, eliminating heap allocation for the closure object entirely.

**Who has it:** Chicken Scheme supports lambda lifting via `-lambda-lift` ([Chicken wiki](https://wiki.call-cc.org/chicken-internal-structure)). GHC uses selective lambda lifting (Graf & Peyton Jones, 2019 — [PDF](https://pp.ipd.kit.edu/uploads/publikationen/graf19sll.pdf)). Wikipedia's [Lambda Lifting](https://en.wikipedia.org/wiki/Lambda_lifting) article notes it's standard in functional compilers.

**Why it matters for shamrocq:** On a `#![no_std]` bare-metal target with a fixed arena, every closure allocation is precious. If a lambda captures 2 variables and is only called via a known call site, lifting it to a top-level function with 2 extra parameters eliminates the `CLOSURE` instruction, the capture loads, and the arena allocation. Your flat-call optimization (`emit_flat_bodies`) already does something similar for globals — lambda lifting would extend this to local functions.

---

### 8. Context-Sensitive Optimization

**What it does:** Optimizes expressions differently depending on whether they appear in *value*, *test* (boolean), or *effect* (result discarded) position.

**Who has it:** Chez Scheme CP0's core design is built around four contexts: `value`, `test`, `effect`, `app`. For example, `(if (not x) a b)` in test context becomes `(if x b a)` — no allocation of the `not` result. `(begin (f x) y)` in value context can drop `(f x)` if pure.

**Why it matters for shamrocq:** Currently, all your passes treat every expression as if its value is needed. In test position, `(= a b)` returns a `True`/`False` constructor that's immediately matched — context-sensitivity could fuse the comparison and branch into a single operation, avoiding the constructor allocation entirely.

---

### Summary Table

| Pass | Chez | Guile | Chicken | shamrocq | Impact |
|---|---|---|---|---|---|
| Copy propagation | CP0 | yes | yes | **no** | High — removes administrative lets |
| Contification | partial | **primary pass** | via CPS | **L1** (disabled) | High — eliminates single-use closures |
| CSE | via CP0 | yes | partial | **no** | Medium — removes redundant computation |
| Use-count inlining | CP0 | yes | yes | **no** (size-only) | High — enables inlining large single-use fns |
| Loop peeling / LICM | partial | yes | no | **no** | Medium — helps tight loops |
| Type/range inference | cptypes.ss | yes | no | **no** | Medium — enables specialization |
| Lambda lifting | partial | via contify | yes | **no** | High on bare-metal — reduces allocations |
| Context-sensitive opt | CP0 core | partial | partial | **no** | Medium — eliminates intermediate values |

The highest-impact additions for this target (Coq extraction + bare-metal) are **copy propagation**, **use-count-based inlining**, and **contification/lambda lifting** — all three directly reduce closure and constructor allocations, which is the primary bottleneck on a fixed-size arena.