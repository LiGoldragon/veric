# veric — The Aski Verifier + Linker

Reads per-module rkyv (produced by askic, conforming to aski-core
types) and produces one `program.rkyv` containing the verified,
linked program (conforming to veri-core types).

## ⚠️ STATUS: STALE — Triply Broken (pre-v0.18, needs port to v0.20)

**Every source file in `src/` is stale and will not compile against
current aski-core.** Do not attempt to fix incrementally.

### Why it's stale

1. **Pre-v0.18 type names** — references `ModuleDef`, `EnumDef`,
   `StructDef`, `TraitDeclDef`, `MethodDef`, `TypeExpr`,
   `GenericParamDef` throughout. These names were retired in v0.18
   when the `Def` suffix was dropped (StructDef → Struct, etc.).

2. **Pre-v0.18 field names** — uses `generic_params`, `Import.names`,
   `Module.Exports`/`Module.Rfis`, and others that no longer exist.

3. **Pre-v0.19 shape changes** — Param became 7 nested variants,
   Type is enum-first with 6 variants, LocalDecl unified, Loop is
   a struct, Module.Exports flat Vec<TypeName>, InstanceType retired.
   veric's assumptions don't match any of this.

4. **Pre-v0.20 shape changes** — AssociatedTypes added to TraitDecl,
   AssociatedTypeBindings to TraitImpl, Type.SelfAssoc variant,
   Expr.SelfRef variant, Module.Exports retired entirely,
   Module.Rfis retired, Visibility is now declaration-local.
   `.rfi` surface handles RFI declarations separately.

5. **~75 compile errors** on origin. Untouched since aski-core v0.18
   redesign landed. Every v0.19 and v0.20 change added to the gap.

### How to update (when the time comes)

**DO NOT PORT YET.** Two preconditions must hold first:

1. **askic-assemble exists** and generates correct `aski_core::Entity`
   output from the v0.20 dsls.rkyv.
2. **askic produces real per-module rkyv** from actual .aski source
   files.

Without these, you'd be porting against guessed shapes of aski-core
entities and would need to rewrite again when askic's actual output
differs. That's the triple-rewrite trap — avoid it.

When both preconditions hold:

1. Read a real per-module .rkyv produced by askic and confirm the
   aski-core::Module (v0.20 shape) structure you receive.
2. **Full rewrite** of veric's parser/linker — not a rename pass.
   Shape changes are too deep (Param variants, Type variants,
   LocalDecl variants, AssociatedTypes, etc.).
3. Produce output conforming to veri-core types (which themselves
   need a D6 redesign — parallel typed entities with EntityRef
   resolution baked in).
4. Re-port 45 unit tests + 4 nix integration tests against the
   new shape.

### What veric Does (design, unchanged)

- **Collects** all per-module rkyv files (one per .aski source file).
- **Builds the global name table** — which module declares what.
- **Resolves cross-module imports/exports** — every `:Foo` becomes
  an absolute reference to the actual entity.
- **Attaches scope information** — every entity carries the
  lexical scope in which it lives, with shadowing already resolved.
- **Verifies**:
  - Import sources exist
  - No circular imports
  - Every referenced name resolves
  - Type-graph well-formedness (no dangling refs)
  - Basic trait-implementation sanity

## Verification Architecture — Reference from Pre-v0.18 Implementation

The pre-v0.18 veric (deleted 2026-04-20 along with its src/ and
test fixtures after v0.20 staleness rendered them unbuildable) is
preserved here as design reference for the port. These are the
load-bearing ideas; the Rust is regenerable.

### Pipeline split: loader → index → verify → emit

Four crisp stages, each with a single responsibility:

- **Loader** — rkyv deserialize, one module at a time.
- **Index** — build lookup tables over all modules (run once).
- **Verify** — read-only; accumulates errors across tiers, returns
  `Vec<Error>`.
- **Emit** — consumes a clean `Vec<Error>` (empty) and produces
  `Program { modules, resolution }`. If verification failed,
  emission never runs.

The split is "verification is read-only; emission builds the
ResolutionTable." Keep this separation.

### Five verification tiers

1. **Imports** — every import source exists; import graph has no cycles.
2. **Type graph** — every type-expression references an existing type;
   arities match; no dangling refs.
3. **Trait structure** — trait impls match their decls (see below).
4. **Scopes** — generic params in scope where referenced; self-params
   coherent; lexical nesting resolved.
5. **Literals** — const values within their declared type's range.

**Critical invariant: all five tiers run even when earlier tiers
fail.** Errors accumulate; no short-circuit. A broken import graph
shouldn't hide type errors — users see every failure in one pass.

### Index data structures

Three key schemas, all using module-index as a compound key:

- `HashMap<(module_idx, name), TypeInfo>` — per-module type lookups.
- `HashMap<module_idx, HashSet<name>>` — visible / exported names per
  module.
- `HashMap<module_idx, HashSet<module_idx>>` — import graph for DFS.

### Verification algorithms worth preserving

- **Nested-type sentinel** — a type declared inside another type
  gets index `usize::MAX` rather than a positional index in
  `module.enums` / `module.structs`. Distinguishes "nested, lookup
  through parent" from "positional at module root."
- **Circular import detection** — 3-state DFS (visited / on_stack /
  path-stack). When a back-edge hits a node on the stack, extract
  the cycle via `position()` on the stack.
- **Generic scope stack** — `generic_scope: Vec<String>` saved/
  restored via RAII closure. Enum generics wrap variant walks; method
  generics push atop impl-level generics.
- **Trait impl signature consistency** — per-method, four checks:
  name presence, param-count match, return-type-presence parity
  (both declare none, or both declare a type), self-kind match (one
  of Owned / Borrowed / MutBorrowed / None). Associated types are
  required unless they carry a default.
- **type_exists fallback through primitives** — the index doesn't
  register primitives (U32/Vec/etc.); `type_exists(name)` calls
  `Primitive::is_primitive(name)` as a first check, then the
  visible-types set.
- **Nested-name collision rule** — nested types collide against
  top-level names in the same module, not just against other nested
  types. Detected separately because nested types aren't in the
  top-level Vec.
- **find_trait_decl cross-module** — local module first, then every
  import whose exported names include the trait.
- **Const range validation matrix** — type/value compatibility
  table: `LiteralValue::Int` ↔ U8/U16/.../I64; signed-range bounds;
  String / Bool / Char handled separately.

### ResolutionTable shape

Sort every list by name for binary-search lookup downstream. Fields
needed: modules, types, traits, impls, generics (arity > 0 only),
imports. `GenericEntry` appears only when the entity has generics.

**Known schema gap**: imported traits had to be stored in the types
list with `kind: TypeKind::Enum` as a placeholder, because the
pre-v0.18 ResolutionTable used a single location type for
types-and-traits. A clean veri-core (D6) redesign should give traits
their own location kind.

### Test fixture inventory (what the verifier should catch)

Pre-v0.18 tests covered: single-module no-imports; export
nonexistent; import missing module; import unexported; valid import;
circular (2-cycle, 3-cycle, diamond non-cycle); name collision
across imports; type not found in field; primitive always valid;
generic arity mismatch; param in/out of scope; self-typed field;
trait impl missing/extra method; return-type mismatch; param-count
mismatch; self-param kind mismatch; missing required associated
type; duplicate type name; nested collision; duplicate generic;
const type-mismatch, in-range, out-of-range (U8/I8), String, Bool,
Char. **Use this as the re-test checklist.**

## Role in the Pipeline

```
askic        — .aski source → per-module rkyv (aski-core types)
veric        — per-module rkyv → program.rkyv (veri-core types) — THIS REPO
domainc      — program.rkyv → Rust domain types (proc macro)
semac        — program.rkyv + domain types → .sema (pure binary)
```

## After the Port

Once v0.20-compatible, veric will output veri-core types that match
the D6 design:
- Each entity carries its own `Vec<EntityRef>` of things it relates to
- No separate Scope wrapper type — scope info embedded on entities
- Post-resolution absolute references (no string lookups in semac)

## Dependencies

- aski-core (input types, v0.20)
- veri-core (output types, D6 redesign pending)
- rkyv

## VCS

`jj` mandatory. Git is storage backend only.
