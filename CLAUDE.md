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
   `Module.Exports`/`Module.Ffis`, and others that no longer exist.

3. **Pre-v0.19 shape changes** — Param became 7 nested variants,
   Type is enum-first with 6 variants, LocalDecl unified, Loop is
   a struct, Module.Exports flat Vec<TypeName>, InstanceType retired.
   veric's assumptions don't match any of this.

4. **Pre-v0.20 shape changes** — AssociatedTypes added to TraitDecl,
   AssociatedTypeBindings to TraitImpl, Type.SelfAssoc variant,
   Expr.SelfRef variant, Module.Exports retired entirely,
   Module.Ffis retired, Visibility is now declaration-local.
   `.ffi` surface handles FFI declarations separately.

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
