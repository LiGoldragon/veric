/// Index — build module/type/trait lookup indices.

use std::collections::{HashMap, HashSet};
use sema_core::aski_core::*;

pub struct Index {
    /// Module name → position in Vec<ModuleDef>
    pub modules: HashMap<String, usize>,
    /// (module_idx, type_name) → (TypeKind, definition_index)
    pub types: HashMap<(usize, String), (TypeKind, usize)>,
    /// (module_idx, trait_name) → declaration_index
    pub traits: HashMap<(usize, String), usize>,
    /// (module_idx, type_name) → generic arity
    pub arities: HashMap<(usize, String), usize>,
    /// module_idx → set of names visible (defined + imported)
    pub visible_types: HashMap<usize, HashSet<String>>,
    /// module_idx → set of trait names visible
    pub visible_traits: HashMap<usize, HashSet<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypeKind {
    Enum,
    Struct,
    Newtype,
    Const,
}

impl Index {
    pub fn build(modules: &[ModuleDef]) -> Self {
        let mut idx = Index {
            modules: HashMap::new(),
            types: HashMap::new(),
            traits: HashMap::new(),
            arities: HashMap::new(),
            visible_types: HashMap::new(),
            visible_traits: HashMap::new(),
        };

        // Register modules
        for (i, m) in modules.iter().enumerate() {
            idx.modules.insert(m.name.0.clone(), i);
        }

        // Register definitions per module
        for (mi, m) in modules.iter().enumerate() {
            let mut vis_types = HashSet::new();
            let mut vis_traits = HashSet::new();

            for (di, e) in m.enums.iter().enumerate() {
                let name = e.name.0.clone();
                idx.types.insert((mi, name.clone()), (TypeKind::Enum, di));
                idx.arities.insert((mi, name.clone()), e.generic_params.len());
                vis_types.insert(name);
                // Register nested types
                idx.register_nested_enum(mi, &e.children, &mut vis_types);
            }

            for (di, s) in m.structs.iter().enumerate() {
                let name = s.name.0.clone();
                idx.types.insert((mi, name.clone()), (TypeKind::Struct, di));
                idx.arities.insert((mi, name.clone()), s.generic_params.len());
                vis_types.insert(name);
                idx.register_nested_struct(mi, &s.children, &mut vis_types);
            }

            for (di, n) in m.newtypes.iter().enumerate() {
                let name = n.name.0.clone();
                idx.types.insert((mi, name.clone()), (TypeKind::Newtype, di));
                idx.arities.insert((mi, name.clone()), n.generic_params.len());
                vis_types.insert(name);
            }

            for (di, c) in m.consts.iter().enumerate() {
                let name = c.name.0.clone();
                idx.types.insert((mi, name.clone()), (TypeKind::Const, di));
                vis_types.insert(name);
            }

            for (di, t) in m.trait_decls.iter().enumerate() {
                let name = t.name.0.clone();
                idx.traits.insert((mi, name.clone()), di);
                idx.arities.insert((mi, name.clone()), t.generic_params.len());
                vis_traits.insert(name);
            }

            idx.visible_types.insert(mi, vis_types);
            idx.visible_traits.insert(mi, vis_traits);
        }

        // Resolve imports: add imported names to visible sets
        for (mi, m) in modules.iter().enumerate() {
            for imp in &m.imports {
                if let Some(_src_mi) = idx.modules.get(&imp.source.0) {
                    for item in &imp.names {
                        match item {
                            ImportItem::Type_(name) => {
                                idx.visible_types.entry(mi).or_default()
                                    .insert(name.0.clone());
                            }
                            ImportItem::Trait(name) => {
                                idx.visible_traits.entry(mi).or_default()
                                    .insert(name.0.clone());
                            }
                        }
                    }
                }
            }
        }

        idx
    }

    fn register_nested_enum(&mut self, mi: usize, children: &[EnumChild], vis: &mut HashSet<String>) {
        for child in children {
            match child {
                EnumChild::NestedEnum(e) => {
                    let name = e.name.0.clone();
                    vis.insert(name.clone());
                    // Nested enums get registered at module scope
                    let next_idx = self.types.values()
                        .filter(|(k, _)| *k == TypeKind::Enum)
                        .count();
                    self.types.insert((mi, name), (TypeKind::Enum, next_idx));
                    self.register_nested_enum(mi, &e.children, vis);
                }
                EnumChild::NestedStruct(s) => {
                    let name = s.name.0.clone();
                    vis.insert(name.clone());
                    let next_idx = self.types.values()
                        .filter(|(k, _)| *k == TypeKind::Struct)
                        .count();
                    self.types.insert((mi, name), (TypeKind::Struct, next_idx));
                    self.register_nested_struct(mi, &s.children, vis);
                }
                _ => {}
            }
        }
    }

    fn register_nested_struct(&mut self, mi: usize, children: &[StructChild], vis: &mut HashSet<String>) {
        for child in children {
            match child {
                StructChild::NestedEnum(e) => {
                    let name = e.name.0.clone();
                    vis.insert(name.clone());
                    let next_idx = self.types.values()
                        .filter(|(k, _)| *k == TypeKind::Enum)
                        .count();
                    self.types.insert((mi, name), (TypeKind::Enum, next_idx));
                    self.register_nested_enum(mi, &e.children, vis);
                }
                StructChild::NestedStruct(s) => {
                    let name = s.name.0.clone();
                    vis.insert(name.clone());
                    let next_idx = self.types.values()
                        .filter(|(k, _)| *k == TypeKind::Struct)
                        .count();
                    self.types.insert((mi, name), (TypeKind::Struct, next_idx));
                    self.register_nested_struct(mi, &s.children, vis);
                }
                _ => {}
            }
        }
    }

    pub fn type_exists(&self, mi: usize, name: &str) -> bool {
        Primitive::is_primitive(name)
            || self.visible_types.get(&mi).map_or(false, |s| s.contains(name))
    }

    pub fn trait_exists(&self, mi: usize, name: &str) -> bool {
        self.visible_traits.get(&mi).map_or(false, |s| s.contains(name))
    }

    pub fn type_arity(&self, mi: usize, name: &str) -> Option<usize> {
        if let Some(a) = Primitive::arity_of(name) {
            return Some(a as usize);
        }
        self.arities.get(&(mi, name.to_string())).copied()
    }
}
