#[cfg(test)]
mod tests {
    use sema_core::aski_core::*;
    use crate::index::Index;
    use crate::verify::Verifier;

    fn module(name: &str) -> ModuleDef {
        ModuleDef {
            name: TypeName(name.into()),
            visibility: Visibility::Public,
            exports: vec![], imports: vec![],
            enums: vec![], structs: vec![], newtypes: vec![],
            consts: vec![], trait_decls: vec![], trait_impls: vec![],
            ffi: vec![], process: None,
            span: Span { start: 0, end: 0 },
        }
    }

    fn bare_enum(name: &str, variants: &[&str]) -> EnumDef {
        EnumDef {
            name: TypeName(name.into()),
            visibility: Visibility::Public,
            generic_params: vec![], derives: vec![],
            children: variants.iter().map(|v| EnumChild::Variant {
                name: VariantName(v.to_string()),
                span: Span { start: 0, end: 0 },
            }).collect(),
            span: Span { start: 0, end: 0 },
        }
    }

    fn typed_struct(name: &str, fields: &[(&str, &str)]) -> StructDef {
        StructDef {
            name: TypeName(name.into()),
            visibility: Visibility::Public,
            generic_params: vec![], derives: vec![],
            children: fields.iter().map(|(n, t)| StructChild::TypedField {
                name: FieldName(n.to_string()),
                visibility: Visibility::Public,
                typ: TypeExpr::Named(TypeName(t.to_string())),
                span: Span { start: 0, end: 0 },
            }).collect(),
            span: Span { start: 0, end: 0 },
        }
    }

    fn verify(modules: Vec<ModuleDef>) -> Vec<String> {
        let idx = Index::build(&modules);
        Verifier::verify(&modules, &idx).into_iter()
            .map(|e| format!("{}: {}", e.module, e.message))
            .collect()
    }

    // ── Tier 1: Import resolution ───────────────────────────

    #[test]
    fn single_module_no_imports() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire", "Earth"]));
        m.exports.push(ExportItem::Type_(TypeName("Element".into())));
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn export_nonexistent_type() {
        let mut m = module("Test");
        m.exports.push(ExportItem::Type_(TypeName("Missing".into())));
        let errs = verify(vec![m]);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("Missing"));
    }

    #[test]
    fn import_from_missing_module() {
        let mut m = module("Test");
        m.imports.push(ModuleImport {
            source: TypeName("Nonexistent".into()),
            names: vec![ImportItem::Type_(TypeName("Foo".into()))],
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("Nonexistent")));
    }

    #[test]
    fn import_unexported_type() {
        let mut m1 = module("Core");
        m1.enums.push(bare_enum("Token", &["Ident"]));
        // Token exists but is NOT exported

        let mut m2 = module("Parser");
        m2.imports.push(ModuleImport {
            source: TypeName("Core".into()),
            names: vec![ImportItem::Type_(TypeName("Token".into()))],
        });
        let errs = verify(vec![m1, m2]);
        assert!(errs.iter().any(|e| e.contains("not exported")));
    }

    #[test]
    fn valid_import() {
        let mut m1 = module("Core");
        m1.enums.push(bare_enum("Token", &["Ident"]));
        m1.exports.push(ExportItem::Type_(TypeName("Token".into())));

        let mut m2 = module("Parser");
        m2.imports.push(ModuleImport {
            source: TypeName("Core".into()),
            names: vec![ImportItem::Type_(TypeName("Token".into()))],
        });
        assert!(verify(vec![m1, m2]).is_empty());
    }

    #[test]
    fn circular_import() {
        let mut m1 = module("A");
        m1.exports.push(ExportItem::Type_(TypeName("X".into())));
        m1.enums.push(bare_enum("X", &["V"]));
        m1.imports.push(ModuleImport {
            source: TypeName("B".into()),
            names: vec![ImportItem::Type_(TypeName("Y".into()))],
        });

        let mut m2 = module("B");
        m2.exports.push(ExportItem::Type_(TypeName("Y".into())));
        m2.enums.push(bare_enum("Y", &["V"]));
        m2.imports.push(ModuleImport {
            source: TypeName("A".into()),
            names: vec![ImportItem::Type_(TypeName("X".into()))],
        });

        let errs = verify(vec![m1, m2]);
        assert!(errs.iter().any(|e| e.contains("circular")));
    }

    #[test]
    fn import_name_collision() {
        let mut core = module("Core");
        core.enums.push(bare_enum("Token", &["Ident"]));
        core.exports.push(ExportItem::Type_(TypeName("Token".into())));

        let mut utils = module("Utils");
        utils.enums.push(bare_enum("Token", &["Number"]));
        utils.exports.push(ExportItem::Type_(TypeName("Token".into())));

        let mut app = module("App");
        app.imports.push(ModuleImport {
            source: TypeName("Core".into()),
            names: vec![ImportItem::Type_(TypeName("Token".into()))],
        });
        app.imports.push(ModuleImport {
            source: TypeName("Utils".into()),
            names: vec![ImportItem::Type_(TypeName("Token".into()))],
        });

        let errs = verify(vec![core, utils, app]);
        assert!(errs.iter().any(|e| e.contains("imported more than once")));
    }

    // ── Tier 2: Type graph ──────────────────────────────────

    #[test]
    fn type_not_found_in_field() {
        let mut m = module("Test");
        m.structs.push(typed_struct("Point", &[("X", "Nonexistent")]));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("Nonexistent")));
    }

    #[test]
    fn primitive_type_always_valid() {
        let mut m = module("Test");
        m.structs.push(typed_struct("Point", &[("X", "F64"), ("Y", "F64")]));
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn generic_arity_mismatch() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire"]));
        m.structs.push(StructDef {
            name: TypeName("Bad".into()),
            visibility: Visibility::Public,
            generic_params: vec![], derives: vec![],
            children: vec![StructChild::TypedField {
                name: FieldName("Items".into()),
                visibility: Visibility::Public,
                typ: TypeExpr::Application(TypeApplication {
                    constructor: TypeName("Vec".into()),
                    args: vec![], // Vec needs 1 arg
                }),
                span: Span { start: 0, end: 0 },
            }],
            span: Span { start: 0, end: 0 },
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("expects 1")));
    }

    #[test]
    fn type_param_in_scope() {
        let mut m = module("Test");
        m.enums.push(EnumDef {
            name: TypeName("Option".into()),
            visibility: Visibility::Public,
            generic_params: vec![GenericParamDef {
                name: TypeParamName("Value".into()),
                bounds: vec![], default: None,
            }],
            derives: vec![],
            children: vec![
                EnumChild::DataVariant {
                    name: VariantName("Some".into()),
                    payload: TypeExpr::Param(TypeParamName("Value".into())),
                    span: Span { start: 0, end: 0 },
                },
                EnumChild::Variant {
                    name: VariantName("None".into()),
                    span: Span { start: 0, end: 0 },
                },
            ],
            span: Span { start: 0, end: 0 },
        });
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn type_param_out_of_scope() {
        let mut m = module("Test");
        m.structs.push(StructDef {
            name: TypeName("Bad".into()),
            visibility: Visibility::Public,
            generic_params: vec![], // no generics declared
            derives: vec![],
            children: vec![StructChild::TypedField {
                name: FieldName("X".into()),
                visibility: Visibility::Public,
                typ: TypeExpr::Param(TypeParamName("Missing".into())),
                span: Span { start: 0, end: 0 },
            }],
            span: Span { start: 0, end: 0 },
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("Missing") && e.contains("not in scope")));
    }

    #[test]
    fn self_typed_field_exists() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Name", &["Alice", "Bob"]));
        m.structs.push(StructDef {
            name: TypeName("Person".into()),
            visibility: Visibility::Public,
            generic_params: vec![], derives: vec![],
            children: vec![StructChild::SelfTypedField {
                name: FieldName("Name".into()),
                visibility: Visibility::Public,
                span: Span { start: 0, end: 0 },
            }],
            span: Span { start: 0, end: 0 },
        });
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn self_typed_field_missing() {
        let mut m = module("Test");
        m.structs.push(StructDef {
            name: TypeName("Person".into()),
            visibility: Visibility::Public,
            generic_params: vec![], derives: vec![],
            children: vec![StructChild::SelfTypedField {
                name: FieldName("NoSuchType".into()),
                visibility: Visibility::Public,
                span: Span { start: 0, end: 0 },
            }],
            span: Span { start: 0, end: 0 },
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("NoSuchType")));
    }

    // ── Tier 3: Trait structure ──────────────────────────────

    fn simple_trait(name: &str, methods: &[&str]) -> TraitDeclDef {
        TraitDeclDef {
            name: TraitName(name.into()),
            visibility: Visibility::Public,
            generic_params: vec![], super_traits: vec![],
            associated_types: vec![],
            signatures: methods.iter().map(|m| MethodSig {
                name: MethodName(m.to_string()),
                generic_params: vec![],
                params: vec![Param::BorrowSelf],
                return_type: None,
                span: Span { start: 0, end: 0 },
            }).collect(),
            span: Span { start: 0, end: 0 },
        }
    }

    fn simple_impl(trait_name: &str, type_name: &str, methods: &[&str]) -> TraitImplDef {
        TraitImplDef {
            trait_name: TraitName(trait_name.into()),
            trait_args: vec![],
            typ: TypeExpr::Named(TypeName(type_name.into())),
            generic_params: vec![],
            methods: methods.iter().map(|m| MethodDef {
                name: MethodName(m.to_string()),
                generic_params: vec![],
                params: vec![Param::BorrowSelf],
                return_type: None,
                body: MethodBody::Block(Block { statements: vec![], tail: None }),
                span: Span { start: 0, end: 0 },
            }).collect(),
            associated_types: vec![],
            span: Span { start: 0, end: 0 },
        }
    }

    #[test]
    fn trait_impl_complete() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire"]));
        m.trait_decls.push(simple_trait("describe", &["describe"]));
        m.trait_impls.push(simple_impl("describe", "Element", &["describe"]));
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn trait_impl_missing_method() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire"]));
        m.trait_decls.push(simple_trait("describe", &["describe", "display"]));
        m.trait_impls.push(simple_impl("describe", "Element", &["describe"]));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("missing method 'display'")));
    }

    #[test]
    fn trait_impl_extraneous_method() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire"]));
        m.trait_decls.push(simple_trait("describe", &["describe"]));
        m.trait_impls.push(simple_impl("describe", "Element", &["describe", "extra"]));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("'extra' not in declaration")));
    }

    #[test]
    fn trait_impl_return_type_mismatch() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire"]));
        // Declaration has return type
        let mut td = simple_trait("compute", &["compute"]);
        td.signatures[0].return_type = Some(TypeExpr::Named(TypeName("U32".into())));
        m.trait_decls.push(td);
        // Impl has NO return type
        m.trait_impls.push(simple_impl("compute", "Element", &["compute"]));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("return type")));
    }

    // ── Tier 4: Scopes ──────────────────────────────────────

    #[test]
    fn duplicate_type_name() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire"]));
        m.structs.push(typed_struct("Element", &[("X", "F64")]));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("duplicate")));
    }

    #[test]
    fn nested_type_collision() {
        let mut m = module("Test");
        // Top-level Delimiter
        m.enums.push(bare_enum("Delimiter", &["LParen"]));
        // Token with nested Delimiter — collides
        m.enums.push(EnumDef {
            name: TypeName("Token".into()),
            visibility: Visibility::Public,
            generic_params: vec![], derives: vec![],
            children: vec![
                EnumChild::Variant { name: VariantName("Ident".into()), span: Span { start: 0, end: 0 } },
                EnumChild::NestedEnum(bare_enum("Delimiter", &["RParen"])),
            ],
            span: Span { start: 0, end: 0 },
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("nested") && e.contains("Delimiter")));
    }

    #[test]
    fn duplicate_generic_param() {
        let mut m = module("Test");
        m.enums.push(EnumDef {
            name: TypeName("Bad".into()),
            visibility: Visibility::Public,
            generic_params: vec![
                GenericParamDef { name: TypeParamName("T".into()), bounds: vec![], default: None },
                GenericParamDef { name: TypeParamName("T".into()), bounds: vec![], default: None },
            ],
            derives: vec![], children: vec![],
            span: Span { start: 0, end: 0 },
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("duplicate generic")));
    }

    // ── Tier 5: Literals ────────────────────────────────────

    #[test]
    fn const_type_mismatch() {
        let mut m = module("Test");
        m.consts.push(ConstDef {
            name: TypeName("Pi".into()),
            visibility: Visibility::Public,
            typ: TypeExpr::Named(TypeName("U32".into())),
            value: LiteralValue::Float(3.14),
            span: Span { start: 0, end: 0 },
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("Pi")));
    }

    #[test]
    fn const_valid() {
        let mut m = module("Test");
        m.consts.push(ConstDef {
            name: TypeName("Max".into()),
            visibility: Visibility::Public,
            typ: TypeExpr::Named(TypeName("U32".into())),
            value: LiteralValue::Int(12),
            span: Span { start: 0, end: 0 },
        });
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn const_out_of_range() {
        let mut m = module("Test");
        m.consts.push(ConstDef {
            name: TypeName("Big".into()),
            visibility: Visibility::Public,
            typ: TypeExpr::Named(TypeName("U8".into())),
            value: LiteralValue::Int(300),
            span: Span { start: 0, end: 0 },
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("out of range")));
    }

    #[test]
    fn const_i8_negative_in_range() {
        let mut m = module("Test");
        m.consts.push(ConstDef {
            name: TypeName("N".into()),
            visibility: Visibility::Public,
            typ: TypeExpr::Named(TypeName("I8".into())),
            value: LiteralValue::Int(-128),
            span: Span { start: 0, end: 0 },
        });
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn const_i8_out_of_range() {
        let mut m = module("Test");
        m.consts.push(ConstDef {
            name: TypeName("N".into()),
            visibility: Visibility::Public,
            typ: TypeExpr::Named(TypeName("I8".into())),
            value: LiteralValue::Int(200),
            span: Span { start: 0, end: 0 },
        });
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("out of range")));
    }

    #[test]
    fn const_string_valid() {
        let mut m = module("Test");
        m.consts.push(ConstDef {
            name: TypeName("Greeting".into()),
            visibility: Visibility::Public,
            typ: TypeExpr::Named(TypeName("String".into())),
            value: LiteralValue::Str("hello".into()),
            span: Span { start: 0, end: 0 },
        });
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn const_bool_valid() {
        let mut m = module("Test");
        m.consts.push(ConstDef {
            name: TypeName("Flag".into()),
            visibility: Visibility::Public,
            typ: TypeExpr::Named(TypeName("Bool".into())),
            value: LiteralValue::Bool(true),
            span: Span { start: 0, end: 0 },
        });
        assert!(verify(vec![m]).is_empty());
    }

    // ── TypeExpr variant coverage ───────────────────────────

    fn struct_with_type(name: &str, field_type: TypeExpr) -> StructDef {
        StructDef {
            name: TypeName(name.into()),
            visibility: Visibility::Public,
            generic_params: vec![], derives: vec![],
            children: vec![StructChild::TypedField {
                name: FieldName("Field".into()),
                visibility: Visibility::Public,
                typ: field_type,
                span: Span { start: 0, end: 0 },
            }],
            span: Span { start: 0, end: 0 },
        }
    }

    #[test]
    fn type_ref_valid() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S",
            TypeExpr::Ref { inner: Box::new(TypeExpr::Named(TypeName("U32".into()))) }));
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn type_ref_invalid_inner() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S",
            TypeExpr::Ref { inner: Box::new(TypeExpr::Named(TypeName("Missing".into()))) }));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("Missing")));
    }

    #[test]
    fn type_boxed_valid() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S",
            TypeExpr::Boxed(Box::new(TypeExpr::Named(TypeName("String".into()))))));
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn type_tuple_valid() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S",
            TypeExpr::Tuple { elements: vec![
                TypeExpr::Named(TypeName("U32".into())),
                TypeExpr::Named(TypeName("F64".into())),
            ]}));
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn type_tuple_invalid_element() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S",
            TypeExpr::Tuple { elements: vec![
                TypeExpr::Named(TypeName("U32".into())),
                TypeExpr::Named(TypeName("Missing".into())),
            ]}));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("Missing")));
    }

    #[test]
    fn type_array_valid() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S",
            TypeExpr::Array {
                element: Box::new(TypeExpr::Named(TypeName("U8".into()))),
                size: 16,
            }));
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn type_fn_ptr_valid() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S",
            TypeExpr::FnPtr {
                params: vec![TypeExpr::Named(TypeName("U32".into()))],
                return_: Box::new(TypeExpr::Named(TypeName("Bool".into()))),
            }));
        assert!(verify(vec![m]).is_empty());
    }

    #[test]
    fn type_fn_ptr_invalid_return() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S",
            TypeExpr::FnPtr {
                params: vec![TypeExpr::Named(TypeName("U32".into()))],
                return_: Box::new(TypeExpr::Named(TypeName("Missing".into()))),
            }));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("Missing")));
    }

    #[test]
    fn type_unit_and_never_always_valid() {
        let mut m = module("Test");
        m.structs.push(struct_with_type("S1", TypeExpr::Unit));
        m.structs.push(struct_with_type("S2", TypeExpr::Never));
        assert!(verify(vec![m]).is_empty());
    }

    // ── Trait tier 3 extra checks ───────────────────────────

    #[test]
    fn trait_impl_param_count_mismatch() {
        let mut m = module("Test");
        m.enums.push(bare_enum("E", &["V"]));
        // Decl has 2 params (self + arg)
        let mut td = simple_trait("compute", &["compute"]);
        td.signatures[0].params.push(Param::Named {
            name: TypeName("X".into()),
            typ: TypeExpr::Named(TypeName("U32".into())),
        });
        m.trait_decls.push(td);
        // Impl has 1 param (self only)
        m.trait_impls.push(simple_impl("compute", "E", &["compute"]));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("params")));
    }

    #[test]
    fn trait_impl_self_param_mismatch() {
        let mut m = module("Test");
        m.enums.push(bare_enum("E", &["V"]));
        // Decl uses &self
        m.trait_decls.push(simple_trait("update", &["update"]));
        // Impl uses self (owned) — mismatch
        let mut ti = simple_impl("update", "E", &["update"]);
        ti.methods[0].params = vec![Param::OwnedSelf];
        m.trait_impls.push(ti);
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("self parameter")));
    }

    #[test]
    fn trait_impl_missing_associated_type() {
        let mut m = module("Test");
        m.enums.push(bare_enum("E", &["V"]));
        let mut td = simple_trait("container", &[]);
        td.associated_types.push(AssociatedTypeDef {
            name: TypeName("Item".into()),
            bounds: vec![],
            default: None, // required — no default
        });
        m.trait_decls.push(td);
        m.trait_impls.push(simple_impl("container", "E", &[]));
        let errs = verify(vec![m]);
        assert!(errs.iter().any(|e| e.contains("associated type 'Item'")));
    }

    // ── Cycle detection edges ───────────────────────────────

    #[test]
    fn three_module_cycle() {
        let mut a = module("A");
        a.enums.push(bare_enum("X", &["V"]));
        a.exports.push(ExportItem::Type_(TypeName("X".into())));
        a.imports.push(ModuleImport {
            source: TypeName("C".into()),
            names: vec![ImportItem::Type_(TypeName("Z".into()))],
        });

        let mut b = module("B");
        b.enums.push(bare_enum("Y", &["V"]));
        b.exports.push(ExportItem::Type_(TypeName("Y".into())));
        b.imports.push(ModuleImport {
            source: TypeName("A".into()),
            names: vec![ImportItem::Type_(TypeName("X".into()))],
        });

        let mut c = module("C");
        c.enums.push(bare_enum("Z", &["V"]));
        c.exports.push(ExportItem::Type_(TypeName("Z".into())));
        c.imports.push(ModuleImport {
            source: TypeName("B".into()),
            names: vec![ImportItem::Type_(TypeName("Y".into()))],
        });

        let errs = verify(vec![a, b, c]);
        assert!(errs.iter().any(|e| e.contains("circular")));
    }

    #[test]
    fn diamond_dependency_no_cycle() {
        // A imports B and C, both import D — not a cycle
        let mut d = module("D");
        d.enums.push(bare_enum("Base", &["V"]));
        d.exports.push(ExportItem::Type_(TypeName("Base".into())));

        let mut b = module("B");
        b.enums.push(bare_enum("X", &["V"]));
        b.exports.push(ExportItem::Type_(TypeName("X".into())));
        b.imports.push(ModuleImport {
            source: TypeName("D".into()),
            names: vec![ImportItem::Type_(TypeName("Base".into()))],
        });

        let mut c = module("C");
        c.enums.push(bare_enum("Y", &["V"]));
        c.exports.push(ExportItem::Type_(TypeName("Y".into())));
        c.imports.push(ModuleImport {
            source: TypeName("D".into()),
            names: vec![ImportItem::Type_(TypeName("Base".into()))],
        });

        let mut a = module("A");
        a.imports.push(ModuleImport {
            source: TypeName("B".into()),
            names: vec![ImportItem::Type_(TypeName("X".into()))],
        });
        a.imports.push(ModuleImport {
            source: TypeName("C".into()),
            names: vec![ImportItem::Type_(TypeName("Y".into()))],
        });

        let errs = verify(vec![a, b, c, d]);
        assert!(errs.iter().all(|e| !e.contains("circular")),
            "diamond should not be a cycle: {:?}", errs);
    }

    // ── Emit tests ──────────────────────────────────────────

    #[test]
    fn emit_produces_valid_program() {
        use crate::emit::Emitter;
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire", "Earth"]));
        m.exports.push(ExportItem::Type_(TypeName("Element".into())));

        let modules = vec![m];
        let idx = Index::build(&modules);
        let program = Emitter::emit(&modules, &idx);

        assert_eq!(program.modules.len(), 1);
        assert!(!program.resolution.modules.is_empty());
        assert!(!program.resolution.types.is_empty());
    }

    #[test]
    fn emit_resolution_table_sorted() {
        use crate::emit::Emitter;
        let mut m1 = module("Beta");
        m1.enums.push(bare_enum("Y", &["V"]));
        let mut m2 = module("Alpha");
        m2.enums.push(bare_enum("X", &["V"]));

        let modules = vec![m1, m2];
        let idx = Index::build(&modules);
        let program = Emitter::emit(&modules, &idx);

        // Modules should be sorted by name
        assert_eq!(program.resolution.modules[0].name, "Alpha");
        assert_eq!(program.resolution.modules[1].name, "Beta");
    }

    #[test]
    fn emit_serializes_to_rkyv() {
        use crate::emit::Emitter;
        let mut m = module("Test");
        m.enums.push(bare_enum("E", &["V"]));

        let modules = vec![m];
        let idx = Index::build(&modules);
        let program = Emitter::emit(&modules, &idx);

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&program);
        assert!(bytes.is_ok(), "serialization should succeed");
        assert!(bytes.unwrap().len() > 0);
    }
}
