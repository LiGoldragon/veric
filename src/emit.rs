/// Emit — build Program with ResolutionTable from verified modules.

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

    fn build_resolution(modules: &[ModuleDef], index: &Index) -> ResolutionTable {
        let mut mod_entries: Vec<ModuleEntry> = index.modules.iter()
            .map(|(name, &idx)| ModuleEntry {
                name: name.clone(),
                index: idx as u32,
            })
            .collect();
        mod_entries.sort_by(|a, b| a.name.cmp(&b.name));

        let mut type_entries: Vec<TypeEntry> = index.types.iter()
            .map(|((_, name), (kind, def_idx))| TypeEntry {
                name: name.clone(),
                location: TypeLocation {
                    module: 0, // filled below
                    kind: match kind {
                        IdxTypeKind::Enum => sema_core::TypeKind::Enum,
                        IdxTypeKind::Struct => sema_core::TypeKind::Struct,
                        IdxTypeKind::Newtype => sema_core::TypeKind::Newtype,
                        IdxTypeKind::Const => sema_core::TypeKind::Const,
                    },
                    index: *def_idx as u32,
                },
            })
            .collect();
        // Fix module indices
        for entry in &mut type_entries {
            for ((mi, name), _) in &index.types {
                if *name == entry.name {
                    entry.location.module = *mi as u32;
                    break;
                }
            }
        }
        type_entries.sort_by(|a, b| a.name.cmp(&b.name));

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

        // Trait-to-impl mapping
        let mut impl_entries = Vec::new();
        for (mi, m) in modules.iter().enumerate() {
            for (ii, ti) in m.trait_impls.iter().enumerate() {
                if let Some(&decl_mi) = index.modules.iter()
                    .find(|(_, &idx)| {
                        index.traits.contains_key(&(idx, ti.trait_name.0.clone()))
                    })
                    .map(|(_, idx)| idx)
                {
                    if let Some(&decl_idx) = index.traits.get(&(decl_mi, ti.trait_name.0.clone())) {
                        impl_entries.push(ImplEntry {
                            trait_name: TraitLocation {
                                module: decl_mi as u32,
                                decl_index: decl_idx as u32,
                            },
                            typ: TypeLocation {
                                module: mi as u32,
                                kind: sema_core::TypeKind::Enum, // placeholder
                                index: 0,
                            },
                            impl_module: mi as u32,
                            impl_index: ii as u32,
                        });
                    }
                }
            }
        }

        // Generic arity entries
        let mut generic_entries: Vec<GenericEntry> = index.arities.iter()
            .filter(|(_, &arity)| arity > 0)
            .filter_map(|((mi, name), &arity)| {
                index.types.get(&(*mi, name.clone())).map(|(kind, def_idx)| {
                    GenericEntry {
                        location: TypeLocation {
                            module: *mi as u32,
                            kind: match kind {
                                IdxTypeKind::Enum => sema_core::TypeKind::Enum,
                                IdxTypeKind::Struct => sema_core::TypeKind::Struct,
                                IdxTypeKind::Newtype => sema_core::TypeKind::Newtype,
                                IdxTypeKind::Const => sema_core::TypeKind::Const,
                            },
                            index: *def_idx as u32,
                        },
                        arity: arity as u32,
                    }
                })
            })
            .collect();
        generic_entries.sort_by_key(|e| e.arity);

        // Import resolutions
        let mut import_resolutions = Vec::new();
        for (mi, m) in modules.iter().enumerate() {
            for imp in &m.imports {
                if let Some(&src_mi) = index.modules.get(&imp.source.0) {
                    let mut resolved = Vec::new();
                    for item in &imp.names {
                        let name = match item {
                            ImportItem::Type_(n) => &n.0,
                            ImportItem::Trait(n) => &n.0,
                        };
                        if let Some((kind, def_idx)) = index.types.get(&(src_mi, name.clone())) {
                            resolved.push(ResolvedImport {
                                name: name.clone(),
                                location: TypeLocation {
                                    module: src_mi as u32,
                                    kind: match kind {
                                        IdxTypeKind::Enum => sema_core::TypeKind::Enum,
                                        IdxTypeKind::Struct => sema_core::TypeKind::Struct,
                                        IdxTypeKind::Newtype => sema_core::TypeKind::Newtype,
                                        IdxTypeKind::Const => sema_core::TypeKind::Const,
                                    },
                                    index: *def_idx as u32,
                                },
                            });
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
}
