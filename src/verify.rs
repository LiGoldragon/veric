/// Verify — structural verification of the aski program.
///
/// Five tiers. All tiers run even if earlier tiers have errors.

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
    /// Generic params in scope for the current definition
    generic_scope: Vec<String>,
}

impl<'a> Verifier<'a> {
    pub fn verify(modules: &[ModuleDef], index: &Index) -> Vec<VerifyError> {
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
        for (mi, m) in self.modules.iter().enumerate() {
            let mod_name = &m.name.0;

            // Verify exports reference real definitions
            for exp in &m.exports {
                match exp {
                    ExportItem::Type_(name) => {
                        if !self.index.types.contains_key(&(mi, name.0.clone())) {
                            self.err(mod_name, format!(
                                "exports type '{}' but it is not defined in this module", name.0));
                        }
                    }
                    ExportItem::Trait(name) => {
                        if !self.index.traits.contains_key(&(mi, name.0.clone())) {
                            self.err(mod_name, format!(
                                "exports trait '{}' but it is not defined in this module", name.0));
                        }
                    }
                }
            }

            // Verify imports
            for imp in &m.imports {
                let src_name = &imp.source.0;
                match self.index.modules.get(src_name) {
                    None => {
                        self.err(mod_name, format!(
                            "imports from module '{}' which does not exist", src_name));
                    }
                    Some(&src_mi) => {
                        let src_mod = &self.modules[src_mi];
                        for item in &imp.names {
                            match item {
                                ImportItem::Type_(name) => {
                                    let exported = src_mod.exports.iter().any(|e| {
                                        matches!(e, ExportItem::Type_(n) if n.0 == name.0)
                                    });
                                    if !exported {
                                        self.err(mod_name, format!(
                                            "imports type '{}' from '{}' but it is not exported",
                                            name.0, src_name));
                                    }
                                }
                                ImportItem::Trait(name) => {
                                    let exported = src_mod.exports.iter().any(|e| {
                                        matches!(e, ExportItem::Trait(n) if n.0 == name.0)
                                    });
                                    if !exported {
                                        self.err(mod_name, format!(
                                            "imports trait '{}' from '{}' but it is not exported",
                                            name.0, src_name));
                                    }
                                }
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
                self.generic_scope = e.generic_params.iter()
                    .map(|p| p.name.0.clone()).collect();
                self.verify_enum_children(mi, &mod_name, &e.children);
            }

            for s in &m.structs {
                self.generic_scope = s.generic_params.iter()
                    .map(|p| p.name.0.clone()).collect();
                self.verify_struct_children(mi, &mod_name, &s.children);
            }

            for n in &m.newtypes {
                self.generic_scope = n.generic_params.iter()
                    .map(|p| p.name.0.clone()).collect();
                self.verify_type_expr(mi, &mod_name, &n.wraps);
            }

            for c in &m.consts {
                self.generic_scope.clear();
                self.verify_type_expr(mi, &mod_name, &c.typ);
            }

            // Verify types in trait declarations
            for td in &m.trait_decls {
                self.generic_scope = td.generic_params.iter()
                    .map(|p| p.name.0.clone()).collect();
                for bound in &td.super_traits {
                    self.verify_trait_bound(mi, &mod_name, bound);
                }
                for sig in &td.signatures {
                    self.verify_method_sig(mi, &mod_name, sig);
                }
            }

            // Verify types in trait implementations
            for ti in &m.trait_impls {
                self.generic_scope = ti.generic_params.iter()
                    .map(|p| p.name.0.clone()).collect();
                if !self.index.trait_exists(mi, &ti.trait_name.0) {
                    self.err(&mod_name, format!(
                        "implements trait '{}' which does not exist", ti.trait_name.0));
                }
                self.verify_type_expr(mi, &mod_name, &ti.typ);
                for arg in &ti.trait_args {
                    self.verify_type_expr(mi, &mod_name, arg);
                }
                for method in &ti.methods {
                    self.verify_method_sig_from_def(mi, &mod_name, method);
                }
            }

            // Verify FFI
            for f in &m.ffi {
                for func in &f.functions {
                    self.generic_scope.clear();
                    for p in &func.params {
                        self.verify_param(mi, &mod_name, p);
                    }
                    if let Some(rt) = &func.return_type {
                        self.verify_type_expr(mi, &mod_name, rt);
                    }
                }
            }

            self.generic_scope.clear();
        }
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
                    let saved = self.generic_scope.clone();
                    self.generic_scope.extend(
                        e.generic_params.iter().map(|p| p.name.0.clone()));
                    self.verify_enum_children(mi, mod_name, &e.children);
                    self.generic_scope = saved;
                }
                EnumChild::NestedStruct(s) => {
                    let saved = self.generic_scope.clone();
                    self.generic_scope.extend(
                        s.generic_params.iter().map(|p| p.name.0.clone()));
                    self.verify_struct_children(mi, mod_name, &s.children);
                    self.generic_scope = saved;
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
                    // Field name IS the type — verify type exists
                    if !self.index.type_exists(mi, &name.0) {
                        self.err(mod_name, format!(
                            "self-typed field '{}' but no type '{}' exists", name.0, name.0));
                    }
                }
                StructChild::NestedEnum(e) => {
                    let saved = self.generic_scope.clone();
                    self.generic_scope.extend(
                        e.generic_params.iter().map(|p| p.name.0.clone()));
                    self.verify_enum_children(mi, mod_name, &e.children);
                    self.generic_scope = saved;
                }
                StructChild::NestedStruct(s) => {
                    let saved = self.generic_scope.clone();
                    self.generic_scope.extend(
                        s.generic_params.iter().map(|p| p.name.0.clone()));
                    self.verify_struct_children(mi, mod_name, &s.children);
                    self.generic_scope = saved;
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
                if !self.index.type_exists(mi, &app.constructor.0)
                    && !self.generic_scope.contains(&app.constructor.0) {
                    self.err(mod_name, format!(
                        "type constructor '{}' not found", app.constructor.0));
                } else if let Some(expected) = self.index.type_arity(mi, &app.constructor.0) {
                    if app.args.len() != expected {
                        self.err(mod_name, format!(
                            "'{}' expects {} type arguments, got {}",
                            app.constructor.0, expected, app.args.len()));
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

    fn verify_param(&mut self, mi: usize, mod_name: &str, param: &Param) {
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

    fn verify_method_sig(&mut self, mi: usize, mod_name: &str, sig: &MethodSig) {
        let saved = self.generic_scope.clone();
        self.generic_scope.extend(
            sig.generic_params.iter().map(|p| p.name.0.clone()));
        for p in &sig.params {
            self.verify_param(mi, mod_name, p);
        }
        if let Some(rt) = &sig.return_type {
            self.verify_type_expr(mi, mod_name, rt);
        }
        self.generic_scope = saved;
    }

    fn verify_method_sig_from_def(&mut self, mi: usize, mod_name: &str, method: &MethodDef) {
        let saved = self.generic_scope.clone();
        self.generic_scope.extend(
            method.generic_params.iter().map(|p| p.name.0.clone()));
        for p in &method.params {
            self.verify_param(mi, mod_name, p);
        }
        if let Some(rt) = &method.return_type {
            self.verify_type_expr(mi, mod_name, rt);
        }
        self.generic_scope = saved;
    }

    // ── Tier 3: Trait structure ──────────────────────────────

    fn tier3_trait_structure(&mut self) {
        for m in self.modules.iter() {
            let mod_name = &m.name.0;

            for ti in &m.trait_impls {
                // Find the trait declaration
                let decl = self.find_trait_decl(m, &ti.trait_name.0);
                let decl = match decl {
                    Some(d) => d,
                    None => continue, // Already reported in tier 2
                };

                // Check method completeness
                for sig in &decl.signatures {
                    let found = ti.methods.iter().any(|m| m.name.0 == sig.name.0);
                    if !found {
                        self.err(mod_name, format!(
                            "trait impl for '{}' missing method '{}'",
                            ti.trait_name.0, sig.name.0));
                    }
                }

                // Check no extraneous methods
                for method in &ti.methods {
                    let found = decl.signatures.iter().any(|s| s.name.0 == method.name.0);
                    if !found {
                        self.err(mod_name, format!(
                            "trait impl for '{}' has method '{}' not in declaration",
                            ti.trait_name.0, method.name.0));
                    }
                }

                // Check param count matches
                for method in &ti.methods {
                    if let Some(sig) = decl.signatures.iter().find(|s| s.name.0 == method.name.0) {
                        if method.params.len() != sig.params.len() {
                            self.err(mod_name, format!(
                                "method '{}' in impl of '{}': {} params, declaration has {}",
                                method.name.0, ti.trait_name.0,
                                method.params.len(), sig.params.len()));
                        }
                    }
                }
            }
        }
    }

    fn find_trait_decl(&self, module: &'a ModuleDef, trait_name: &str) -> Option<&'a TraitDeclDef> {
        // Check current module
        if let Some(td) = module.trait_decls.iter().find(|t| t.name.0 == trait_name) {
            return Some(td);
        }
        // Check imported modules
        for imp in &module.imports {
            let has_trait = imp.names.iter().any(|n| {
                matches!(n, ImportItem::Trait(t) if t.0 == trait_name)
            });
            if has_trait {
                if let Some(&src_mi) = self.index.modules.get(&imp.source.0) {
                    let src = &self.modules[src_mi];
                    if let Some(td) = src.trait_decls.iter().find(|t| t.name.0 == trait_name) {
                        return Some(td);
                    }
                }
            }
        }
        None
    }

    // ── Tier 4: Scope uniqueness ────────────────────────────

    fn tier4_scopes(&mut self) {
        for m in self.modules.iter() {
            let mod_name = &m.name.0;
            let mut type_names: Vec<&str> = Vec::new();

            for e in &m.enums {
                if type_names.contains(&e.name.0.as_str()) {
                    self.err(mod_name, format!("duplicate type name '{}'", e.name.0));
                }
                type_names.push(&e.name.0);
            }
            for s in &m.structs {
                if type_names.contains(&s.name.0.as_str()) {
                    self.err(mod_name, format!("duplicate type name '{}'", s.name.0));
                }
                type_names.push(&s.name.0);
            }
            for n in &m.newtypes {
                if type_names.contains(&n.name.0.as_str()) {
                    self.err(mod_name, format!("duplicate type name '{}'", n.name.0));
                }
                type_names.push(&n.name.0);
            }

            // Check generic param uniqueness per definition
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

    fn check_generic_uniqueness(&mut self, mod_name: &str, type_name: &str, params: &[GenericParamDef]) {
        let mut seen = Vec::new();
        for p in params {
            if seen.contains(&p.name.0.as_str()) {
                self.err(mod_name, format!(
                    "duplicate generic param '{}' in '{}'", p.name.0, type_name));
            }
            seen.push(&p.name.0);
        }
    }

    // ── Tier 5: Literal/const verification ──────────────────

    fn tier5_literals(&mut self) {
        for m in self.modules.iter() {
            let mod_name = &m.name.0;
            for c in &m.consts {
                self.verify_const_value(mod_name, c);
            }
        }
    }

    fn verify_const_value(&mut self, mod_name: &str, c: &ConstDef) {
        let type_name = match &c.typ {
            TypeExpr::Named(n) => Some(n.0.as_str()),
            _ => None,
        };

        if let Some(tn) = type_name {
            let ok = match (&c.value, tn) {
                (LiteralValue::Int(_), "U8" | "U16" | "U32" | "U64"
                    | "I8" | "I16" | "I32" | "I64") => true,
                (LiteralValue::Float(_), "F32" | "F64") => true,
                (LiteralValue::Str(_), "String") => true,
                (LiteralValue::Bool(_), "Bool") => true,
                (LiteralValue::Char(_), "Char") => true,
                _ => false,
            };
            if !ok {
                self.err(mod_name, format!(
                    "const '{}' declared as '{}' but value is {:?}",
                    c.name.0, tn, c.value));
            }
        }
    }
}
