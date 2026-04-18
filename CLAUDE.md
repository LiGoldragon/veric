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

## v0.18 Status: BROKEN — Pending Port

veric's source code references the PRE-v0.18 aski-core type names:
`ModuleDef`, `EnumDef`, `StructDef`, `TraitDeclDef`, `MethodDef`,
`TypeExpr`, `GenericParamDef`, and old field names like
`generic_params` (now `generics`) and `Import.names` (now split
into `Import.objects` + `Import.actions`).

**Mass-rename doesn't suffice** — the shapes of many entities
changed (e.g., Param is now 7 nested variants instead of a single
enum; Type is enum-first with Borrowed/MutBorrowed; Origins and
view types exist at multiple positions). veric needs a port-level
pass.

Test count pre-redesign: **45 unit tests + 4 nix integration tests
passing**. They'll need corresponding updates or replacements.

## After the Port

Once v0.18-compatible, veric will output veri-core types that match
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
