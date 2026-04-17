#[cfg(test)]
mod tests {
    use sema_core::aski_core::*;
    use crate::index::Index;
    use crate::verify::Verifier;

    fn module(name: &str) -> ModuleDef {
        ModuleDef {
            name: TypeName(name.to_string()),
            visibility: Visibility::Public,
            exports: vec![],
            imports: vec![],
            enums: vec![],
            structs: vec![],
            newtypes: vec![],
            consts: vec![],
            trait_decls: vec![],
            trait_impls: vec![],
            ffi: vec![],
            process: None,
            span: Span { start: 0, end: 0 },
        }
    }

    fn bare_enum(name: &str, variants: &[&str]) -> EnumDef {
        EnumDef {
            name: TypeName(name.to_string()),
            visibility: Visibility::Public,
            generic_params: vec![],
            derives: vec![],
            children: variants.iter().map(|v| EnumChild::Variant {
                name: VariantName(v.to_string()),
                span: Span { start: 0, end: 0 },
            }).collect(),
            span: Span { start: 0, end: 0 },
        }
    }

    #[test]
    fn single_module_no_imports() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire", "Earth"]));
        m.exports.push(ExportItem::Type_(TypeName("Element".into())));

        let modules = vec![m];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn export_nonexistent_type() {
        let mut m = module("Test");
        m.exports.push(ExportItem::Type_(TypeName("Missing".into())));

        let modules = vec![m];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("Missing"));
    }

    #[test]
    fn import_from_missing_module() {
        let mut m = module("Test");
        m.imports.push(ModuleImport {
            source: TypeName("Nonexistent".into()),
            names: vec![ImportItem::Type_(TypeName("Foo".into()))],
        });

        let modules = vec![m];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("Nonexistent"));
    }

    #[test]
    fn import_unexported_type() {
        let mut m1 = module("Core");
        m1.enums.push(bare_enum("Token", &["Ident", "Number"]));
        // Token exists but is NOT exported

        let mut m2 = module("Parser");
        m2.imports.push(ModuleImport {
            source: TypeName("Core".into()),
            names: vec![ImportItem::Type_(TypeName("Token".into()))],
        });

        let modules = vec![m1, m2];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("not exported"));
    }

    #[test]
    fn valid_import() {
        let mut m1 = module("Core");
        m1.enums.push(bare_enum("Token", &["Ident", "Number"]));
        m1.exports.push(ExportItem::Type_(TypeName("Token".into())));

        let mut m2 = module("Parser");
        m2.imports.push(ModuleImport {
            source: TypeName("Core".into()),
            names: vec![ImportItem::Type_(TypeName("Token".into()))],
        });

        let modules = vec![m1, m2];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn type_not_found_in_field() {
        let mut m = module("Test");
        m.structs.push(StructDef {
            name: TypeName("Point".into()),
            visibility: Visibility::Public,
            generic_params: vec![],
            derives: vec![],
            children: vec![
                StructChild::TypedField {
                    name: FieldName("X".into()),
                    visibility: Visibility::Public,
                    typ: TypeExpr::Named(TypeName("NonexistentType".into())),
                    span: Span { start: 0, end: 0 },
                },
            ],
            span: Span { start: 0, end: 0 },
        });

        let modules = vec![m];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("NonexistentType"));
    }

    #[test]
    fn primitive_type_always_valid() {
        let mut m = module("Test");
        m.structs.push(StructDef {
            name: TypeName("Point".into()),
            visibility: Visibility::Public,
            generic_params: vec![],
            derives: vec![],
            children: vec![
                StructChild::TypedField {
                    name: FieldName("X".into()),
                    visibility: Visibility::Public,
                    typ: TypeExpr::Named(TypeName("F64".into())),
                    span: Span { start: 0, end: 0 },
                },
            ],
            span: Span { start: 0, end: 0 },
        });

        let modules = vec![m];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

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

        let modules = vec![m];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("Pi"));
    }

    #[test]
    fn duplicate_type_name() {
        let mut m = module("Test");
        m.enums.push(bare_enum("Element", &["Fire"]));
        m.structs.push(StructDef {
            name: TypeName("Element".into()),
            visibility: Visibility::Public,
            generic_params: vec![],
            derives: vec![],
            children: vec![],
            span: Span { start: 0, end: 0 },
        });

        let modules = vec![m];
        let idx = Index::build(&modules);
        let errors = Verifier::verify(&modules, &idx);
        assert!(errors.iter().any(|e| e.message.contains("duplicate")),
            "errors: {:?}", errors);
    }
}
