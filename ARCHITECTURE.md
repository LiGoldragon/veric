# veric — The Aski Verifier

veric reads per-module rkyv files produced by askic and
verifies that the program is structurally sound. It produces
a single program.rkyv containing all modules with verified
cross-references.

From Latin *verus* (true). The tool that makes the program
true.


## The Pipeline With veric

```
                    ┌─────────┐
  Elements.aski ──→ │         │──→ Elements.rkyv
      Core.aski ──→ │  askic  │──→ Core.rkyv
     Utils.aski ──→ │         │──→ Utils.rkyv
                    └─────────┘
              (per-file, parallel, cacheable)

                    ┌─────────┐
  Elements.rkyv ──→ │         │
      Core.rkyv ──→ │  veric  │──→ program.rkyv
     Utils.rkyv ──→ │         │
    ext-dep.rkyv ──→│         │
                    └─────────┘
              (cross-module verification)

                    ┌─────────┐
   program.rkyv ──→ │  semac  │──→ .sema + .aski-table.sema
                    └─────────┘
              (expression compilation)
```

askic compiles individual files. veric verifies the program.
semac compiles expressions. Three tools, three jobs, clean
separation.


## CLI

```
veric <module.rkyv>... [-o program.rkyv]
veric Elements.rkyv Core.rkyv Utils.rkyv -o astro.rkyv
```

Multiple inputs (one per module). One output (the verified
program). External dependency .rkyv files are passed the
same way — veric doesn't distinguish between own modules
and dependencies.


## Input

Each .rkyv file contains one serialized ModuleDef — the
container form produced by askic. ModuleDef holds everything
that was in the .aski file, structured:

```
ModuleDef
  name: TypeName
  exports: Vec<ExportItem>
  imports: Vec<ModuleImport>
  enums: Vec<EnumDef>
  structs: Vec<StructDef>
  newtypes: Vec<NewtypeDef>
  consts: Vec<ConstDef>
  trait_decls: Vec<TraitDeclDef>
  trait_impls: Vec<TraitImplDef>
  ffi: Vec<FfiDef>
  process: Option<Block>
  span: Span
```

The module's exports list says what it exposes. The imports
list says what it needs from other modules. The definitions
are the module's content. All names are still strings — veric
does not produce sema.


## Output

A single program.rkyv containing Vec<ModuleDef> — every
module from the input, verified and bundled. The output type
is the same as the input type (ModuleDef), just collected.

Optionally enriched with a resolution table (see "Resolution
Table" below).


## What veric Verifies

### Tier 1: Module Linking

The minimum viable verifier. This is what makes the program
a program instead of a pile of files.

**Import resolution.** Every import has a corresponding
export. For each module, for each import:
- The source module exists (by name) in the input set
- Each imported name exists in the source module's exports
- The import kind matches (Type imports a Type, Trait
  imports a Trait)

**Export validity.** Every exported name actually exists
at module scope. A module cannot export a name it doesn't
define.

**Module existence.** Every module referenced by any import
is present in the input set. Missing module = error, not
silent failure.

**Circular import detection.** A imports from B, B imports
from A — detected and reported. (Circular dependencies may
be allowed in the future with explicit forward declarations,
but initially they are errors.)

**Name collision.** If module A imports `Token` from both
Core and Utils, that's a collision. Detected and reported.


### Tier 2: Type Graph Verification

Every type reference in the program resolves to an actual
definition. This catches typos, missing definitions, and
broken refactors before semac ever sees the program.

**Type existence.** Every TypeName used in:
- struct fields (TypedField.typ, StructField.typ)
- enum variant payloads (DataVariant.payload)
- newtype wraps (NewtypeDef.wraps)
- const type annotations (ConstDef.typ)
- method params and return types
- type applications (TypeApplication.constructor)
must resolve to a type defined in the same module or imported
from another module.

**Self-typed field resolution.** When a struct has a
self-typed field `Name` (where the field name IS the type),
verify that type `Name` exists in scope.

**Trait existence.** Every TraitName used in:
- trait implementations (TraitImplDef.trait_name)
- trait bounds (TraitBound.trait_name)
- supertraits (TraitDeclDef.super_traits)
- generic param bounds (GenericParamDef.bounds)
must resolve to a trait declared in the same module or
imported from another module.

**Generic arity.** Type applications must have the right
number of arguments:
- `[Vec]` — missing argument (Vec needs 1)
- `[Option A B]` — too many (Option needs 1)
- `[Result A]` — too few (Result needs 2)
This requires counting generic_params on the target
definition.


### Tier 3: Trait Structure Verification

Trait implementations must be structurally consistent with
their declarations. This doesn't require expression
evaluation — it's pure signature matching.

**Method completeness.** A trait impl must provide every
method declared in the trait. Missing methods = error.

**No extraneous methods.** A trait impl must not provide
methods that don't exist in the trait declaration.

**Signature consistency.** For each method in an impl:
- Parameter count must match the declaration's signature
- Return type presence/absence must match
- Self parameter kind must be compatible

**Associated type satisfaction.** If the trait declares
associated types, the impl must provide them.


### Tier 4: Scope and Visibility

**Name uniqueness.** No two types share a name in the same
module scope. No two traits share a name. No two consts
share a name.

**Nested type scoping.** Nested enums/structs inside a
parent get scoped names (e.g., `Delimiter` inside `Token`
becomes reachable as `Token.Delimiter`). Verify no collision
with top-level names.

**Visibility enforcement.** Private types must not appear
in public interfaces:
- A public struct's field types must be public
- A public trait's method params/return types must be public
- An exported type must be public

**Generic parameter uniqueness.** No duplicate $Param
names in the same definition's generic_params list.

**Type parameter scope.** A TypeExpr::Param("Value") in a
field must reference a GenericParamDef declared on the
enclosing type.


### Tier 5: Literal and Const Verification

**Const value/type consistency.** `{| MaxSigns U32 12 |}`
— verify that `12` (LiteralValue::Int) is compatible with
`U32` (TypeExpr::Named). An `F64` const with an Int value
is an error.

**Literal range.** Integer literals must fit in the declared
type (a U8 const can't hold 300).


## What veric Does NOT Do

These are semac's responsibilities:

- **Expression type-checking** — evaluating whether
  `@Self.Left + @Self.Right` produces the right type
- **Method resolution** — determining which impl's method
  to call based on receiver type
- **Trait bound satisfaction** — verifying that a generic
  instantiation meets its bounds
- **Pattern exhaustiveness** — checking that match arms
  cover all variants (needs type info)
- **Generic monomorphization** — instantiating generic
  types with concrete parameters
- **Constant folding** — evaluating const expressions
- **Anything that touches Expr** — veric reads Expr nodes
  but does not evaluate them

The clean split: veric verifies the NOUNS (types, names,
structure). semac compiles the VERBS (expressions, method
bodies, implementations).


## Resolution Table (Optional Enrichment)

veric has already done the work of finding where every name
lives. It could include a resolution table in the output:

```
ResolutionTable {
    types: HashMap<(ModuleIndex, TypeName), TypeLocation>,
    traits: HashMap<(ModuleIndex, TraitName), TraitLocation>,
}

TypeLocation {
    module: ModuleIndex,
    kind: TypeKind,  // Enum, Struct, Newtype
    index: usize,    // position in module's Vec
}
```

This gives semac pre-resolved references instead of string
lookups. semac can jump directly to the definition instead
of scanning modules.

Whether to include this is a design decision. Without it,
semac rebuilds the index from the verified modules (cheap
but redundant). With it, the index is computed once and
shared.


## Implementation Structure

```
veric/
  src/
    main.rs       — CLI, reads .rkyv files, orchestrates
    loader.rs     — deserializes ModuleDef from rkyv bytes
    index.rs      — builds module index (name → exports)
    verify.rs     — all verification passes
    emit.rs       — serializes Vec<ModuleDef> to output rkyv
```

### main.rs

```rust
struct Veric {
    modules: Vec<ModuleDef>,
}
```

1. Parse CLI args (list of .rkyv files + output path)
2. For each .rkyv file: deserialize ModuleDef, push to modules
3. Build module index
4. Run verification passes (tier 1 → tier 5)
5. If all pass: serialize Vec<ModuleDef> to output
6. If any fail: report errors with source spans, exit nonzero

### loader.rs

Deserializes one ModuleDef from rkyv bytes. Uses the aski
crate for types — same types askic serialized with.

```rust
fn load_module(bytes: &[u8]) -> Result<ModuleDef, String>
```

veric fully deserializes (not zero-copy) because it needs to
own the data for the output serialization.

### index.rs

Builds the cross-module lookup structures:

```rust
struct ModuleIndex {
    modules: HashMap<String, usize>,        // name → index
    exports: HashMap<String, Vec<Export>>,   // module → exports
}

struct Export {
    name: String,
    kind: ExportKind,  // Type or Trait
}
```

Built once from all loaded modules. Used by every
verification pass.

### verify.rs

Each tier is a method on a Verifier struct:

```rust
struct Verifier<'a> {
    modules: &'a [ModuleDef],
    index: &'a ModuleIndex,
    errors: Vec<VerifyError>,
}

struct VerifyError {
    module: String,     // which module
    span: Span,         // source location
    message: String,    // what went wrong
}
```

Methods:
- `verify_imports(&mut self)` — tier 1
- `verify_type_graph(&mut self)` — tier 2
- `verify_trait_structure(&mut self)` — tier 3
- `verify_scopes(&mut self)` — tier 4
- `verify_literals(&mut self)` — tier 5

Each method pushes errors to self.errors. All tiers run
even if earlier tiers have errors (collect all errors at
once, don't stop at first failure).

### emit.rs

Serializes `Vec<ModuleDef>` with rkyv. Optionally includes
the resolution table.


## Dependencies

```toml
[dependencies]
aski = { path = "flake-crates/aski" }
rkyv = { version = "0.8", features = ["little_endian", "alloc"] }
```

veric depends only on aski (for the types) and rkyv (for
serialization). It does NOT depend on aski-core, askicc,
corec, or any other pipeline tool.


## Nix Integration

```nix
# Per-module compilation (parallel, cacheable)
elements-mod = pkgs.runCommand "elements-mod" {
  nativeBuildInputs = [ askic ];
} ''
  mkdir -p $out
  askic ${./source/Elements.aski} $out/Elements.rkyv
'';

core-mod = pkgs.runCommand "core-mod" {
  nativeBuildInputs = [ askic ];
} ''
  mkdir -p $out
  askic ${./source/Core.aski} $out/Core.rkyv
'';

# Verification (depends on all modules)
program = pkgs.runCommand "program" {
  nativeBuildInputs = [ veric ];
} ''
  mkdir -p $out
  veric ${elements-mod}/Elements.rkyv \
        ${core-mod}/Core.rkyv \
        -o $out/program.rkyv
'';
```

Change one .aski file → only that module recompiles → veric
re-runs (cheap — reads rkyv, does index lookups, no parsing).


## How askic Changes

askic currently takes one .aski file and produces Vec<RootChild>
as flat rkyv. With veric in the pipeline:

### 1. aski contract changes (root.aski)

RootChild is removed. ModuleDef becomes the container:

```aski
;; BEFORE
(RootChild
  (Module ModuleDef)
  (Enum EnumDef)
  (Struct StructDef)
  ...)

{ModuleDef
  (Name TypeName)
  (Exports [Vec ExportItem])
  (Imports [Vec ModuleImport])
  Span}

;; AFTER — RootChild removed, ModuleDef is the root type
{ModuleDef
  (Name TypeName)
  (Exports [Vec ExportItem])
  (Imports [Vec ModuleImport])
  (Enums [Vec EnumDef])
  (Structs [Vec StructDef])
  (Newtypes [Vec NewtypeDef])
  (Consts [Vec ConstDef])
  (TraitDecls [Vec TraitDeclDef])
  (TraitImpls [Vec TraitImplDef])
  (Ffi [Vec FfiDef])
  (Process [Option Block])
  Span}
```

The module declaration in root.aski also updates to list
ModuleDef instead of RootChild as the module's export set.

### 2. askic output changes

askic serializes one ModuleDef per file instead of
Vec<RootChild>:

```rust
// BEFORE (main.rs)
let root_children: Vec<RootChild> = engine.parse(&tokens)?;
let bytes = rkyv::to_bytes(&root_children)?;

// AFTER
let module: ModuleDef = engine.parse(&tokens)?;
let bytes = rkyv::to_bytes(&module)?;
```

### 3. askic builder changes

build_root() restructures from flat to container:

```rust
// BEFORE — flat Vec<RootChild>
fn build_root(&self, rules: Vec<MatchedRule>) -> ... {
    let mut children = Vec::new();
    children.push(RootChild::Module(module_def));
    for (alt_idx, values) in repeated {
        children.push(self.build_root_child(alt_idx, values)?);
    }
    Ok(DialectValue::RootChildren(children))
}

// AFTER — ModuleDef container
fn build_root(&self, rules: Vec<MatchedRule>) -> ... {
    let mut module_def = /* from rule 0 */;
    let mut enums = Vec::new();
    let mut structs = Vec::new();
    // ... other vecs

    for (alt_idx, values) in repeated {
        match alt_idx {
            0 => enums.push(self.build_enum_def(values)?),
            1 => module_def.trait_decls.push(...),
            2 => module_def.trait_impls.push(...),
            3 => structs.push(self.build_struct_def(values)?),
            4 => module_def.consts.push(...),
            5 => module_def.ffi.push(...),
            6 => module_def.process = Some(...),
            7 => module_def.newtypes.push(...),
            _ => ...
        }
    }

    module_def.enums = enums;
    module_def.structs = structs;
    Ok(DialectValue::Module(module_def))
}
```

build_root_child() is absorbed into build_root(). The eight
alternatives still exist — they just populate ModuleDef
fields instead of returning RootChild variants.

### 4. askic values.rs changes

```rust
// BEFORE
pub enum DialectValue {
    RootChildren(Vec<RootChild>),
    Module(ModuleDef),
    ...
}

// AFTER — RootChildren removed
pub enum DialectValue {
    Module(ModuleDef),  // now the root-level value
    ...
}
```

### 5. askic engine.rs changes

```rust
// BEFORE
pub fn parse(&self, tokens: &[Spanned])
    -> Result<Vec<RootChild>, String>
{
    match result {
        ParseValue::Dialect(DialectValue::RootChildren(c)) => Ok(c),
        ...
    }
}

// AFTER
pub fn parse(&self, tokens: &[Spanned])
    -> Result<ModuleDef, String>
{
    match result {
        ParseValue::Dialect(DialectValue::Module(m)) => Ok(m),
        ...
    }
}
```

### 6. askic test changes

Tests currently use `(T E)` as throwaway module
declarations and index into Vec<RootChild>:

```rust
// BEFORE
let children = parse("(T E)\n(Element Fire Earth Air Water)");
assert_eq!(children.len(), 2);
match &children[1] { RootChild::Enum(e) => ... }

// AFTER
let module = parse("(T E)\n(Element Fire Earth Air Water)");
assert_eq!(module.enums.len(), 1);
let e = &module.enums[0];
assert_eq!(e.name.0, "Element");
```

The parse() test helper returns ModuleDef. Tests access
module.enums, module.structs, module.newtypes, etc. directly.
This is cleaner — tests say what they mean.

15 tests need updating. The changes are mechanical: replace
Vec indexing + RootChild matching with direct field access.


## The Full Pipeline After Redesign

```
corec       .aski → Rust with rkyv derives (bootstrap)
aski-core   grammar .aski + corec → rkyv types (askicc↔askic)
aski        parse tree .aski + corec → rkyv types (askic↔veric↔semac)
askicc      .synth → rkyv dialect-data-tree (embedded in askic)
askic       .aski → per-module .rkyv (ModuleDef, structured)
veric       per-module .rkyv → program.rkyv (verified, linked)
domainc     program.rkyv → domain types (proc macro at compile time)
semac       program.rkyv + domain types → .sema (expression compilation)
rsc         .sema + domain types → .rs (Rust projection)
askid       .sema + domain types + name table → .aski (deparse)
```

askic and veric split what was one step (compile + verify)
into two (compile then verify). The split enables:
- Parallel per-module compilation
- Incremental recompilation (change one file, recompile one)
- Early error detection (structural errors before semac)
- External dependency support (pass dep .rkyv to veric)
- Clean separation of concerns

semac receives a verified program and can trust that every
name reference is valid. It never needs to check "does this
type exist?" — veric already did.
