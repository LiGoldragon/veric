/// Index — build module/type/trait lookup indices.
///
/// All methods on Index struct.

use std::collections::{HashMap, HashSet};
use veri_core::aski_core::*;

pub struct Index {
    /// Module name → position in Vec<ModuleDef>
    pub modules: HashMap<String, usize>,
    /// (module_idx, type_name) → (TypeKind, definition_index)
    pub types: HashMap<(usize, String), (TypeKind, usize)>,
    /// (module_idx, trait_name) → declaration_index
    pub traits: HashMap<(usize, String), usize>,
    /// (module_idx, type_or_trait_name) → generic arity
    pub arities: HashMap<(usize, String), usize>,
    /// module_idx → set of type names visible (defined + imported)
    pub visible_types: HashMap<usize, HashSet<String>>,
    /// module_idx → set of trait names visible (defined + imported)
    pub visible_traits: HashMap<usize, HashSet<String>>,
    /// module_idx → set of names exported
    pub exported_types: HashMap<usize, HashSet<String>>,
    pub exported_traits: HashMap<usize, HashSet<String>>,
    /// Import graph: module_idx → set of module indices it imports from
    pub import_graph: HashMap<usize, HashSet<usize>>,
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
            exported_types: HashMap::new(),
            exported_traits: HashMap::new(),
            import_graph: HashMap::new(),
        };

        // Register modules by name
        for (i, m) in modules.iter().enumerate() {
            idx.modules.insert(m.name.0.clone(), i);
        }

        // Register definitions per module
        for (mi, m) in modules.iter().enumerate() {
            let mut vis_types = HashSet::new();
            let mut vis_traits = HashSet::new();
            let mut exp_types = HashSet::new();
            let mut exp_traits = HashSet::new();

            // Enums — index matches position in module.enums
            for (di, e) in m.enums.iter().enumerate() {
                idx.register_type(mi, &e.name.0, TypeKind::Enum, di, e.generic_params.len());
                vis_types.insert(e.name.0.clone());
                idx.register_nested_in_enum(mi, &e.children, &mut vis_types);
            }

            // Structs — index matches position in module.structs
            for (di, s) in m.structs.iter().enumerate() {
                idx.register_type(mi, &s.name.0, TypeKind::Struct, di, s.generic_params.len());
                vis_types.insert(s.name.0.clone());
                idx.register_nested_in_struct(mi, &s.children, &mut vis_types);
            }

            // Newtypes — index matches position in module.newtypes
            for (di, n) in m.newtypes.iter().enumerate() {
                idx.register_type(mi, &n.name.0, TypeKind::Newtype, di, n.generic_params.len());
                vis_types.insert(n.name.0.clone());
            }

            // Consts — index matches position in module.consts
            for (di, c) in m.consts.iter().enumerate() {
                idx.register_type(mi, &c.name.0, TypeKind::Const, di, 0);
                vis_types.insert(c.name.0.clone());
            }

            // Trait declarations
            for (di, t) in m.trait_decls.iter().enumerate() {
                idx.traits.insert((mi, t.name.0.clone()), di);
                idx.arities.insert((mi, t.name.0.clone()), t.generic_params.len());
                vis_traits.insert(t.name.0.clone());
            }

            // Exports
            for exp in &m.exports {
                match exp {
                    ExportItem::Type_(name) => { exp_types.insert(name.0.clone()); }
                    ExportItem::Trait(name) => { exp_traits.insert(name.0.clone()); }
                }
            }

            idx.visible_types.insert(mi, vis_types);
            idx.visible_traits.insert(mi, vis_traits);
            idx.exported_types.insert(mi, exp_types);
            idx.exported_traits.insert(mi, exp_traits);
        }

        // Build import graph and add imported names to visible sets
        for (mi, m) in modules.iter().enumerate() {
            let mut deps = HashSet::new();
            for imp in &m.imports {
                if let Some(&src_mi) = idx.modules.get(&imp.source.0) {
                    deps.insert(src_mi);
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
            idx.import_graph.insert(mi, deps);
        }

        idx
    }

    fn register_type(&mut self, mi: usize, name: &str, kind: TypeKind, idx: usize, arity: usize) {
        self.types.insert((mi, name.to_string()), (kind, idx));
        self.arities.insert((mi, name.to_string()), arity);
    }

    fn register_nested_in_enum(&mut self, mi: usize, children: &[EnumChild], vis: &mut HashSet<String>) {
        for child in children {
            match child {
                EnumChild::NestedEnum(e) => {
                    // Nested types are visible at module scope by name.
                    // They don't get a positional index in module.enums — they're
                    // accessed through their parent. Index as usize::MAX to distinguish.
                    vis.insert(e.name.0.clone());
                    self.types.insert((mi, e.name.0.clone()), (TypeKind::Enum, usize::MAX));
                    self.register_nested_in_enum(mi, &e.children, vis);
                }
                EnumChild::NestedStruct(s) => {
                    vis.insert(s.name.0.clone());
                    self.types.insert((mi, s.name.0.clone()), (TypeKind::Struct, usize::MAX));
                    self.register_nested_in_struct(mi, &s.children, vis);
                }
                _ => {}
            }
        }
    }

    fn register_nested_in_struct(&mut self, mi: usize, children: &[StructChild], vis: &mut HashSet<String>) {
        for child in children {
            match child {
                StructChild::NestedEnum(e) => {
                    vis.insert(e.name.0.clone());
                    self.types.insert((mi, e.name.0.clone()), (TypeKind::Enum, usize::MAX));
                    self.register_nested_in_enum(mi, &e.children, vis);
                }
                StructChild::NestedStruct(s) => {
                    vis.insert(s.name.0.clone());
                    self.types.insert((mi, s.name.0.clone()), (TypeKind::Struct, usize::MAX));
                    self.register_nested_in_struct(mi, &s.children, vis);
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

    /// Detect cycles in the import graph via DFS.
    pub fn find_import_cycles(&self) -> Vec<Vec<usize>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut on_stack = HashSet::new();

        for &mi in self.import_graph.keys() {
            if !visited.contains(&mi) {
                self.dfs_cycle(mi, &mut visited, &mut stack, &mut on_stack, &mut cycles);
            }
        }
        cycles
    }

    fn dfs_cycle(
        &self,
        node: usize,
        visited: &mut HashSet<usize>,
        stack: &mut Vec<usize>,
        on_stack: &mut HashSet<usize>,
        cycles: &mut Vec<Vec<usize>>,
    ) {
        visited.insert(node);
        stack.push(node);
        on_stack.insert(node);

        if let Some(deps) = self.import_graph.get(&node) {
            for &dep in deps {
                if !visited.contains(&dep) {
                    self.dfs_cycle(dep, visited, stack, on_stack, cycles);
                } else if on_stack.contains(&dep) {
                    // Found cycle — extract it from stack
                    let start = stack.iter().position(|&n| n == dep).unwrap();
                    cycles.push(stack[start..].to_vec());
                }
            }
        }

        stack.pop();
        on_stack.remove(&node);
    }
}
