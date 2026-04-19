# veric — The Aski Verifier + Linker

Reads per-module rkyv (produced by askic, conforming to aski-core
types) and produces one `program.rkyv` containing the verified,
linked program (conforming to veri-core types).

## Role in the Pipeline

```
askic        — .aski source → per-module rkyv (aski-core types)
veric        — per-module rkyv → program.rkyv (veri-core types) — THIS REPO
domainc      — program.rkyv → Rust domain types (proc macro)
semac        — program.rkyv + domain types → .sema (pure binary)
```

## What veric Does

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

## v0.19 Status: BROKEN — Pending Port (doubly blocked)

veric's source code references the PRE-v0.18 aski-core type names:
`ModuleDef`, `EnumDef`, `StructDef`, `TraitDeclDef`, `MethodDef`,
`TypeExpr`, `GenericParamDef`, and old field names.

**Double blockage now in v0.19**: aski-core has changed again —
LocalDecl unified, Loop is a struct, Module.Exports flat
`Vec<TypeName>`, InstanceType retired, etc. veric's ~75 compile
errors from v0.18 are now compounded by v0.19 shape changes.

**Mass-rename doesn't suffice** — the shapes of many entities
changed. veric needs a port-level pass.

**Also: veric is now blocked on askic.** Porting veric against
guessed v0.19 shapes means rewriting twice. Wait until askic
produces real per-module rkyv, then port against observed output.

Test count pre-redesign: **45 unit tests + 4 nix integration tests
passing**. They'll need corresponding updates or replacements.

## After the Port

Once v0.19-compatible, veric will output veri-core types that match
the D6 design:
- Each entity carries its own `Vec<EntityRef>` of things it relates to
- No separate Scope wrapper type — scope info embedded on entities
- Post-resolution absolute references (no string lookups in semac)

## Dependencies

- aski-core (input types)
- veri-core (output types) — was `sema-core`, renamed 2026-04-18
- rkyv

## VCS

`jj` mandatory. Git is storage backend only.
