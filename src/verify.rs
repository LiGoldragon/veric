/// Verify — structural verification of the aski program.
///
/// Five tiers. All tiers run even if earlier tiers have errors.
/// All methods on Verifier struct.

use std::collections::HashSet;
use sema_core::aski_core::*;
use crate::index::Index;

#[derive(Debug)]
pub struct VerifyError {
    pub module: String,
    pub message: String,
}

pub struct Verifier<'a> {
    modules: &'a [ModuleDef],
    index: &'a Index,
    errors: Vec<VerifyError>,
    /// Generic params currently in scope
    generic_scope: Vec<String>,
}

impl<'a> Verifier<'a> {
    pub fn verify(modules: &'a [ModuleDef], index: &'a Index) -> Vec<VerifyError> {
        let mut v = Verifier {
            modules,
            index,
            errors: Vec::new(),
            generic_scope: Vec::new(),
        };
        v.tier1_imports();
        v.tier2_type_graph();
        v.tier3_trait_structure();
        v.tier4_scopes();
        v.tier5_literals();
        v.errors
    }

    fn err(&mut self, module: &str, msg: String) {
        self.errors.push(VerifyError {
            module: module.to_string(),
            message: msg,
        });
    }

    // ── Tier 1: Import resolution ───────────────────────────

    fn tier1_imports(&mut self) {
        // Circular import detection
        let cycles = self.index.find_import_cycles();
        for cycle in &cycles {
            let names: Vec<&str> = cycle.iter()
                .map(|&mi| self.modules[mi].name.0.as_str())
                .collect();
            self.err(&names[0], format!(
                "circular import: {}", names.join(" → ")));
        }

        for (mi, m) in self.modules.iter().enumerate() {
            let mod_name = &m.name.0;

            // Export validity — exported names must exist
            for exp in &m.exports {
                match exp {
                    ExportItem::Type_(name) => {
                        if !self.index.types.contains_key(&(mi, name.0.clone())) {
                            self.err(mod_name, format!(
                                "exports type '{}' but it is not defined", name.0));
                        }
                    }
                    ExportItem::Trait(name) => {
                        if !self.index.traits.contains_key(&(mi, name.0.clone())) {
                            self.err(mod_name, format!(
                                "exports trait '{}' but it is not defined", name.0));
                        }
                    }
                }
            }

            // Import resolution
            let mut imported_names: HashSet<String> = HashSet::new();
            for imp in &m.imports {
                let src_name = &imp.source.0;
                match self.index.modules.get(src_name) {
                    None => {
                        self.err(mod_name, format!(
                            "imports from module '{}' which does not exist", src_name));
                    }
                    Some(&src_mi) => {
                        for item in &imp.names {
                            let (name, is_trait) = match item {
                                ImportItem::Type_(n) => (&n.0, false),
                                ImportItem::Trait(n) => (&n.0, true),
                            };

                            // Check export exists in source module
                            let exported = if is_trait {
                                self.index.exported_traits.get(&src_mi)
                                    .map_or(false, |s| s.contains(name))
                            } else {
                                self.index.exported_types.get(&src_mi)
                                    .map_or(false, |s| s.contains(name))
                            };
                            if !exported {
                                self.err(mod_name, format!(
                                    "imports {} '{}' from '{}' but it is not exported",
                                    if is_trait { "trait" } else { "type" },
                                    name, src_name));
                            }

                            // Name collision detection
                            if !imported_names.insert(name.clone()) {
                                self.err(mod_name, format!(
                                    "name '{}' imported more than once", name));
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Tier 2: Type graph ──────────────────────────────────

    fn tier2_type_graph(&mut self) {
        for (mi, m) in self.modules.iter().enumerate() {
            let mod_name = m.name.0.clone();

            for e in &m.enums {
                self.with_generics(&e.generic_params, |v| {
                    v.verify_enum_children(mi, &mod_name, &e.children);
                    v.verify_generic_bounds(mi, &mod_name, &e.generic_params);
                });
            }

            for s in &m.structs {
                self.with_generics(&s.generic_params, |v| {
                    v.verify_struct_children(mi, &mod_name, &s.children);
                    v.verify_generic_bounds(mi, &mod_name, &s.generic_params);
                });
            }

            for n in &m.newtypes {
                self.with_generics(&n.generic_params, |v| {
                    v.verify_type_expr(mi, &mod_name, &n.wraps);
                    v.verify_generic_bounds(mi, &mod_name, &n.generic_params);
                });
            }

            for c in &m.consts {
                self.generic_scope.clear();
                self.verify_type_expr(mi, &mod_name, &c.typ);
            }

            for td in &m.trait_decls {
                self.with_generics(&td.generic_params, |v| {
                    for bound in &td.super_traits {
                        v.verify_trait_bound(mi, &mod_name, bound);
                    }
                    for sig in &td.signatures {
                        v.verify_method_types(mi, &mod_name, &sig.generic_params,
                            &sig.params, &sig.return_type);
                    }
                });
            }

            for ti in &m.trait_impls {
                self.with_generics(&ti.generic_params, |v| {
                    if !v.index.trait_exists(mi, &ti.trait_name.0) {
                        v.err(&mod_name, format!(
                            "implements trait '{}' which does not exist", ti.trait_name.0));
                    }
                    v.verify_type_expr(mi, &mod_name, &ti.typ);
                    for arg in &ti.trait_args {
                        v.verify_type_expr(mi, &mod_name, arg);
                    }
                    for method in &ti.methods {
                        v.verify_method_types(mi, &mod_name, &method.generic_params,
                            &method.params, &method.return_type);
                    }
                });
            }

            for f in &m.ffi {
                for func in &f.functions {
                    self.generic_scope.clear();
                    for p in &func.params {
                        self.verify_param_type(mi, &mod_name, p);
                    }
                    if let Some(rt) = &func.return_type {
                        self.verify_type_expr(mi, &mod_name, rt);
                    }
                }
            }

            self.generic_scope.clear();
        }
    }

    fn with_generics(&mut self, params: &[GenericParamDef], f: impl FnOnce(&mut Self)) {
        let saved = self.generic_scope.clone();
        self.generic_scope.extend(params.iter().map(|p| p.name.0.clone()));
        f(self);
        self.generic_scope = saved;
    }

    fn verify_generic_bounds(&mut self, mi: usize, mod_name: &str, params: &[GenericParamDef]) {
        for p in params {
            for bound in &p.bounds {
                self.verify_trait_bound(mi, mod_name, bound);
            }
            if let Some(default) = &p.default {
                self.verify_type_expr(mi, mod_name, default);
            }
        }
    }

    fn verify_method_types(&mut self, mi: usize, mod_name: &str,
        method_generics: &[GenericParamDef], params: &[Param], return_type: &Option<TypeExpr>)
    {
        let saved = self.generic_scope.clone();
        self.generic_scope.extend(method_generics.iter().map(|p| p.name.0.clone()));
        for p in params {
            self.verify_param_type(mi, mod_name, p);
        }
        if let Some(rt) = return_type {
            self.verify_type_expr(mi, mod_name, rt);
        }
        self.generic_scope = saved;
    }

    fn verify_enum_children(&mut self, mi: usize, mod_name: &str, children: &[EnumChild]) {
        for child in children {
            match child {
                EnumChild::Variant { .. } => {}
                EnumChild::DataVariant { payload, .. } => {
                    self.verify_type_expr(mi, mod_name, payload);
                }
                EnumChild::StructVariant { fields, .. } => {
                    for f in fields {
                        self.verify_type_expr(mi, mod_name, &f.typ);
                    }
                }
                EnumChild::NestedEnum(e) => {
                    self.with_generics(&e.generic_params, |v| {
                        v.verify_enum_children(mi, mod_name, &e.children);
                    });
                }
                EnumChild::NestedStruct(s) => {
                    self.with_generics(&s.generic_params, |v| {
                        v.verify_struct_children(mi, mod_name, &s.children);
                    });
                }
            }
        }
    }

    fn verify_struct_children(&mut self, mi: usize, mod_name: &str, children: &[StructChild]) {
        for child in children {
            match child {
                StructChild::TypedField { typ, .. } => {
                    self.verify_type_expr(mi, mod_name, typ);
                }
                StructChild::SelfTypedField { name, .. } => {
                    if !self.index.type_exists(mi, &name.0) {
                        self.err(mod_name, format!(
                            "self-typed field '{}': type '{}' not found", name.0, name.0));
                    }
                }
                StructChild::NestedEnum(e) => {
                    self.with_generics(&e.generic_params, |v| {
                        v.verify_enum_children(mi, mod_name, &e.children);
                    });
                }
                StructChild::NestedStruct(s) => {
                    self.with_generics(&s.generic_params, |v| {
                        v.verify_struct_children(mi, mod_name, &s.children);
                    });
                }
            }
        }
    }

    fn verify_type_expr(&mut self, mi: usize, mod_name: &str, expr: &TypeExpr) {
        match expr {
            TypeExpr::Named(name) => {
                if !self.index.type_exists(mi, &name.0)
                    && !self.generic_scope.contains(&name.0) {
                    self.err(mod_name, format!("type '{}' not found", name.0));
                }
            }
            TypeExpr::Application(app) => {
                let name = &app.constructor.0;
                if !self.index.type_exists(mi, name)
                    && !self.generic_scope.contains(name) {
                    self.err(mod_name, format!(
                        "type constructor '{}' not found", name));
                } else if let Some(expected) = self.index.type_arity(mi, name) {
                    if app.args.len() != expected {
                        self.err(mod_name, format!(
                            "'{}' expects {} type arguments, got {}",
                            name, expected, app.args.len()));
                    }
                }
                for arg in &app.args {
                    self.verify_type_expr(mi, mod_name, arg);
                }
            }
            TypeExpr::Param(name) => {
                if !self.generic_scope.contains(&name.0) {
                    self.err(mod_name, format!(
                        "type parameter '{}' not in scope", name.0));
                }
            }
            TypeExpr::BoundedParam { bounds } => {
                for b in bounds {
                    if !self.index.trait_exists(mi, &b.0)
                        && !self.index.type_exists(mi, &b.0) {
                        self.err(mod_name, format!("bound '{}' not found", b.0));
                    }
                }
            }
            TypeExpr::Ref { inner } | TypeExpr::MutRef { inner }
            | TypeExpr::Boxed(inner) => {
                self.verify_type_expr(mi, mod_name, inner);
            }
            TypeExpr::Tuple { elements } => {
                for e in elements {
                    self.verify_type_expr(mi, mod_name, e);
                }
            }
            TypeExpr::Array { element, .. } | TypeExpr::Slice { element } => {
                self.verify_type_expr(mi, mod_name, element);
            }
            TypeExpr::FnPtr { params, return_ } => {
                for p in params {
                    self.verify_type_expr(mi, mod_name, p);
                }
                self.verify_type_expr(mi, mod_name, return_);
            }
            TypeExpr::DynTrait { bounds } | TypeExpr::ImplTrait { bounds } => {
                for b in bounds {
                    self.verify_trait_bound(mi, mod_name, b);
                }
            }
            TypeExpr::InstanceRef(app) => {
                if !self.index.type_exists(mi, &app.constructor.0) {
                    self.err(mod_name, format!(
                        "instance type '{}' not found", app.constructor.0));
                }
                for arg in &app.args {
                    self.verify_type_expr(mi, mod_name, arg);
                }
            }
            TypeExpr::QualifiedPath { base, .. } => {
                self.verify_type_expr(mi, mod_name, base);
            }
            TypeExpr::SelfType | TypeExpr::Unit | TypeExpr::Never => {}
        }
    }

    fn verify_trait_bound(&mut self, mi: usize, mod_name: &str, bound: &TraitBound) {
        if !self.index.trait_exists(mi, &bound.trait_name.0) {
            self.err(mod_name, format!("trait '{}' not found", bound.trait_name.0));
        }
        for arg in &bound.args {
            self.verify_type_expr(mi, mod_name, arg);
        }
    }

    fn verify_param_type(&mut self, mi: usize, mod_name: &str, param: &Param) {
        match param {
            Param::BorrowNamed { typ, .. }
            | Param::MutBorrowNamed { typ, .. }
            | Param::Named { typ, .. } => {
                self.verify_type_expr(mi, mod_name, typ);
            }
            Param::BorrowSelf | Param::MutBorrowSelf
            | Param::OwnedSelf | Param::Bare { .. } => {}
        }
    }

    // ── Tier 3: Trait structure ──────────────────────────────

    fn tier3_trait_structure(&mut self) {
        for m in self.modules.iter() {
            let mod_name = &m.name.0;

            for ti in &m.trait_impls {
                let decl = match self.find_trait_decl(m, &ti.trait_name.0) {
                    Some(d) => d,
                    None => continue, // Already reported in tier 2
                };

                // Method completeness — every declared method must be implemented
                for sig in &decl.signatures {
                    if !ti.methods.iter().any(|m| m.name.0 == sig.name.0) {
                        self.err(mod_name, format!(
                            "impl '{}': missing method '{}'",
                            ti.trait_name.0, sig.name.0));
                    }
                }

                // No extraneous methods
                for method in &ti.methods {
                    if !decl.signatures.iter().any(|s| s.name.0 == method.name.0) {
                        self.err(mod_name, format!(
                            "impl '{}': method '{}' not in declaration",
                            ti.trait_name.0, method.name.0));
                    }
                }

                // Signature consistency for each matching method
                for method in &ti.methods {
                    if let Some(sig) = decl.signatures.iter().find(|s| s.name.0 == method.name.0) {
                        // Param count
                        if method.params.len() != sig.params.len() {
                            self.err(mod_name, format!(
                                "impl '{}' method '{}': {} params, declaration has {}",
                                ti.trait_name.0, method.name.0,
                                method.params.len(), sig.params.len()));
                        }

                        // Return type presence must match
                        if method.return_type.is_some() != sig.return_type.is_some() {
                            self.err(mod_name, format!(
                                "impl '{}' method '{}': return type {}",
                                ti.trait_name.0, method.name.0,
                                if sig.return_type.is_some() {
                                    "expected but missing"
                                } else {
                                    "present but not in declaration"
                                }));
                        }

                        // Self parameter compatibility
                        if !method.params.is_empty() && !sig.params.is_empty() {
                            let impl_self = Self::self_kind(&method.params[0]);
                            let decl_self = Self::self_kind(&sig.params[0]);
                            if impl_self != decl_self {
                                self.err(mod_name, format!(
                                    "impl '{}' method '{}': self parameter kind mismatch",
                                    ti.trait_name.0, method.name.0));
                            }
                        }
                    }
                }

                // Associated types — every required associated type must be provided
                for at in &decl.associated_types {
                    if at.default.is_none() {
                        let provided = ti.associated_types.iter()
                            .any(|a| a.name.0 == at.name.0);
                        if !provided {
                            self.err(mod_name, format!(
                                "impl '{}': missing associated type '{}'",
                                ti.trait_name.0, at.name.0));
                        }
                    }
                }
            }
        }
    }

    fn self_kind(param: &Param) -> u8 {
        match param {
            Param::BorrowSelf => 0,
            Param::MutBorrowSelf => 1,
            Param::OwnedSelf => 2,
            _ => 3, // non-self
        }
    }

    fn find_trait_decl(&self, module: &'a ModuleDef, trait_name: &str) -> Option<&'a TraitDeclDef> {
        if let Some(td) = module.trait_decls.iter().find(|t| t.name.0 == trait_name) {
            return Some(td);
        }
        for imp in &module.imports {
            let has_trait = imp.names.iter().any(|n| {
                matches!(n, ImportItem::Trait(t) if t.0 == trait_name)
            });
            if has_trait {
                if let Some(&src_mi) = self.index.modules.get(&imp.source.0) {
                    if let Some(td) = self.modules[src_mi].trait_decls.iter()
                        .find(|t| t.name.0 == trait_name) {
                        return Some(td);
                    }
                }
            }
        }
        None
    }

    // ── Tier 4: Scopes ──────────────────────────────────────

    fn tier4_scopes(&mut self) {
        for m in self.modules.iter() {
            let mod_name = &m.name.0;

            // Type name uniqueness at module scope
            let mut all_names: HashSet<String> = HashSet::new();

            for e in &m.enums {
                if !all_names.insert(e.name.0.clone()) {
                    self.err(mod_name, format!("duplicate type name '{}'", e.name.0));
                }
                // Check nested types don't collide with top-level
                self.check_nested_enum_collisions(mod_name, &e.children, &all_names);
            }
            for s in &m.structs {
                if !all_names.insert(s.name.0.clone()) {
                    self.err(mod_name, format!("duplicate type name '{}'", s.name.0));
                }
                self.check_nested_struct_collisions(mod_name, &s.children, &all_names);
            }
            for n in &m.newtypes {
                if !all_names.insert(n.name.0.clone()) {
                    self.err(mod_name, format!("duplicate type name '{}'", n.name.0));
                }
            }
            for c in &m.consts {
                if !all_names.insert(c.name.0.clone()) {
                    self.err(mod_name, format!("duplicate name '{}'", c.name.0));
                }
            }

            // Trait name uniqueness
            let mut trait_names: HashSet<String> = HashSet::new();
            for td in &m.trait_decls {
                if !trait_names.insert(td.name.0.clone()) {
                    self.err(mod_name, format!("duplicate trait name '{}'", td.name.0));
                }
            }

            // Generic param uniqueness per definition
            for e in &m.enums {
                self.check_generic_uniqueness(mod_name, &e.name.0, &e.generic_params);
            }
            for s in &m.structs {
                self.check_generic_uniqueness(mod_name, &s.name.0, &s.generic_params);
            }
            for n in &m.newtypes {
                self.check_generic_uniqueness(mod_name, &n.name.0, &n.generic_params);
            }
        }
    }

    fn check_nested_enum_collisions(&mut self, mod_name: &str, children: &[EnumChild], top_level: &HashSet<String>) {
        for child in children {
            match child {
                EnumChild::NestedEnum(e) => {
                    if top_level.contains(&e.name.0) {
                        self.err(mod_name, format!(
                            "nested type '{}' collides with top-level name", e.name.0));
                    }
                    self.check_nested_enum_collisions(mod_name, &e.children, top_level);
                }
                EnumChild::NestedStruct(s) => {
                    if top_level.contains(&s.name.0) {
                        self.err(mod_name, format!(
                            "nested type '{}' collides with top-level name", s.name.0));
                    }
                    self.check_nested_struct_collisions(mod_name, &s.children, top_level);
                }
                _ => {}
            }
        }
    }

    fn check_nested_struct_collisions(&mut self, mod_name: &str, children: &[StructChild], top_level: &HashSet<String>) {
        for child in children {
            match child {
                StructChild::NestedEnum(e) => {
                    if top_level.contains(&e.name.0) {
                        self.err(mod_name, format!(
                            "nested type '{}' collides with top-level name", e.name.0));
                    }
                    self.check_nested_enum_collisions(mod_name, &e.children, top_level);
                }
                StructChild::NestedStruct(s) => {
                    if top_level.contains(&s.name.0) {
                        self.err(mod_name, format!(
                            "nested type '{}' collides with top-level name", s.name.0));
                    }
                    self.check_nested_struct_collisions(mod_name, &s.children, top_level);
                }
                _ => {}
            }
        }
    }

    fn check_generic_uniqueness(&mut self, mod_name: &str, type_name: &str, params: &[GenericParamDef]) {
        let mut seen = HashSet::new();
        for p in params {
            if !seen.insert(&p.name.0) {
                self.err(mod_name, format!(
                    "duplicate generic param '{}' in '{}'", p.name.0, type_name));
            }
        }
    }

    // ── Tier 5: Literal/const verification ──────────────────

    fn tier5_literals(&mut self) {
        for m in self.modules.iter() {
            let mod_name = &m.name.0;
            for c in &m.consts {
                self.verify_const(mod_name, c);
            }
        }
    }

    fn verify_const(&mut self, mod_name: &str, c: &ConstDef) {
        let type_name = match &c.typ {
            TypeExpr::Named(n) => Some(n.0.as_str()),
            _ => None,
        };

        if let Some(tn) = type_name {
            // Type/value compatibility
            let compatible = match (&c.value, tn) {
                (LiteralValue::Int(_), "U8" | "U16" | "U32" | "U64"
                    | "I8" | "I16" | "I32" | "I64") => true,
                (LiteralValue::Float(_), "F32" | "F64") => true,
                (LiteralValue::Str(_), "String") => true,
                (LiteralValue::Bool(_), "Bool") => true,
                (LiteralValue::Char(_), "Char") => true,
                _ => false,
            };
            if !compatible {
                self.err(mod_name, format!(
                    "const '{}': type '{}' incompatible with value {:?}",
                    c.name.0, tn, c.value));
            }

            // Integer range validation
            if let LiteralValue::Int(v) = &c.value {
                let in_range = match tn {
                    "U8" => *v >= 0 && *v <= 255,
                    "U16" => *v >= 0 && *v <= 65535,
                    "U32" => *v >= 0 && *v <= u32::MAX as i64,
                    "U64" => *v >= 0,
                    "I8" => *v >= -128 && *v <= 127,
                    "I16" => *v >= -32768 && *v <= 32767,
                    "I32" => *v >= i32::MIN as i64 && *v <= i32::MAX as i64,
                    "I64" => true,
                    _ => true,
                };
                if !in_range {
                    self.err(mod_name, format!(
                        "const '{}': value {} out of range for '{}'",
                        c.name.0, v, tn));
                }
            }
        }
    }
}
