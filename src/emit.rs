/// Emit — build Program with ResolutionTable from verified modules.
///
/// All methods on Emitter struct.

use sema_core::*;
use sema_core::aski_core::*;
use crate::index::{Index, TypeKind as IdxTypeKind};

pub struct Emitter;

impl Emitter {
    pub fn emit(modules: &[ModuleDef], index: &Index) -> Program {
        let resolution = Self::build_resolution(modules, index);
        Program {
            modules: modules.to_vec(),
            resolution,
        }
    }

    fn to_sema_kind(k: &IdxTypeKind) -> TypeKind {
        match k {
            IdxTypeKind::Enum => TypeKind::Enum,
            IdxTypeKind::Struct => TypeKind::Struct,
            IdxTypeKind::Newtype => TypeKind::Newtype,
            IdxTypeKind::Const => TypeKind::Const,
        }
    }

    fn build_resolution(modules: &[ModuleDef], index: &Index) -> ResolutionTable {
        // Module index — sorted by name
        let mut mod_entries: Vec<ModuleEntry> = index.modules.iter()
            .map(|(name, &idx)| ModuleEntry {
                name: name.clone(),
                index: idx as u32,
            })
            .collect();
        mod_entries.sort_by(|a, b| a.name.cmp(&b.name));

        // Type index — sorted by name, module index from HashMap key
        let mut type_entries: Vec<TypeEntry> = index.types.iter()
            .map(|((mi, name), (kind, def_idx))| TypeEntry {
                name: name.clone(),
                location: TypeLocation {
                    module: *mi as u32,
                    kind: Self::to_sema_kind(kind),
                    index: *def_idx as u32,
                },
            })
            .collect();
        type_entries.sort_by(|a, b| a.name.cmp(&b.name));

        // Trait index — sorted by name
        let mut trait_entries: Vec<TraitEntry> = index.traits.iter()
            .map(|((mi, name), &decl_idx)| TraitEntry {
                name: name.clone(),
                location: TraitLocation {
                    module: *mi as u32,
                    decl_index: decl_idx as u32,
                },
            })
            .collect();
        trait_entries.sort_by(|a, b| a.name.cmp(&b.name));

        // Trait-to-impl mapping — resolve actual types
        let mut impl_entries = Vec::new();
        for (mi, m) in modules.iter().enumerate() {
            for (ii, ti) in m.trait_impls.iter().enumerate() {
                // Find the trait declaration location
                let trait_loc = Self::find_trait_location(index, mi, &ti.trait_name.0);
                // Find the type being implemented
                let type_loc = Self::resolve_type_location(index, mi, &ti.typ);

                if let (Some(tl), Some(ty)) = (trait_loc, type_loc) {
                    impl_entries.push(ImplEntry {
                        trait_name: tl,
                        typ: ty,
                        impl_module: mi as u32,
                        impl_index: ii as u32,
                    });
                }
            }
        }

        // Generic arity entries — only for types with arity > 0
        let mut generic_entries: Vec<GenericEntry> = Vec::new();
        for ((mi, name), &arity) in &index.arities {
            if arity > 0 {
                if let Some((kind, def_idx)) = index.types.get(&(*mi, name.clone())) {
                    generic_entries.push(GenericEntry {
                        location: TypeLocation {
                            module: *mi as u32,
                            kind: Self::to_sema_kind(kind),
                            index: *def_idx as u32,
                        },
                        arity: arity as u32,
                    });
                }
            }
        }
        generic_entries.sort_by_key(|e| e.location.module);

        // Import resolutions — resolve BOTH type and trait imports
        let mut import_resolutions = Vec::new();
        for (mi, m) in modules.iter().enumerate() {
            for imp in &m.imports {
                if let Some(&src_mi) = index.modules.get(&imp.source.0) {
                    let mut resolved = Vec::new();
                    for item in &imp.names {
                        match item {
                            ImportItem::Type_(name) => {
                                if let Some((kind, def_idx)) = index.types.get(&(src_mi, name.0.clone())) {
                                    resolved.push(ResolvedImport {
                                        name: name.0.clone(),
                                        location: TypeLocation {
                                            module: src_mi as u32,
                                            kind: Self::to_sema_kind(kind),
                                            index: *def_idx as u32,
                                        },
                                    });
                                }
                            }
                            ImportItem::Trait(name) => {
                                if let Some(&decl_idx) = index.traits.get(&(src_mi, name.0.clone())) {
                                    // Traits get a TypeLocation with kind Enum as placeholder
                                    // since ResolutionTable uses TypeLocation for all imports.
                                    // The trait_entries table has the real TraitLocation.
                                    resolved.push(ResolvedImport {
                                        name: name.0.clone(),
                                        location: TypeLocation {
                                            module: src_mi as u32,
                                            kind: TypeKind::Enum, // trait marker
                                            index: decl_idx as u32,
                                        },
                                    });
                                }
                            }
                        }
                    }
                    if !resolved.is_empty() {
                        import_resolutions.push(ImportResolution {
                            source_module: mi as u32,
                            resolved,
                        });
                    }
                }
            }
        }

        ResolutionTable {
            modules: mod_entries,
            types: type_entries,
            traits: trait_entries,
            impls: impl_entries,
            generics: generic_entries,
            imports: import_resolutions,
        }
    }

    fn find_trait_location(index: &Index, mi: usize, trait_name: &str) -> Option<TraitLocation> {
        // Check current module
        if let Some(&decl_idx) = index.traits.get(&(mi, trait_name.to_string())) {
            return Some(TraitLocation { module: mi as u32, decl_index: decl_idx as u32 });
        }
        // Check all modules (trait could be imported)
        for (&(ref_mi, ref ref_name), &decl_idx) in &index.traits {
            if ref_name == trait_name {
                return Some(TraitLocation { module: ref_mi as u32, decl_index: decl_idx as u32 });
            }
        }
        None
    }

    fn resolve_type_location(index: &Index, mi: usize, typ: &TypeExpr) -> Option<TypeLocation> {
        let name = match typ {
            TypeExpr::Named(n) => &n.0,
            TypeExpr::Application(app) => &app.constructor.0,
            _ => return None,
        };
        // Check current module first
        if let Some((kind, def_idx)) = index.types.get(&(mi, name.clone())) {
            return Some(TypeLocation {
                module: mi as u32,
                kind: Self::to_sema_kind(kind),
                index: *def_idx as u32,
            });
        }
        // Check all modules (type could be imported)
        for ((ref_mi, ref ref_name), (kind, def_idx)) in &index.types {
            if ref_name == name {
                return Some(TypeLocation {
                    module: *ref_mi as u32,
                    kind: Self::to_sema_kind(kind),
                    index: *def_idx as u32,
                });
            }
        }
        None
    }
}
