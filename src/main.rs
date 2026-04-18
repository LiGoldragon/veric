/// veric — the aski verifier.
///
/// Reads per-module .rkyv files, verifies structural correctness,
/// produces verified program.rkyv.
///
/// Usage: veric <module.rkyv>... [-o program.rkyv]

use std::fs;
use veri_core::aski_core::ModuleDef;
use veric::{loader, index, verify, emit};

struct Veric {
    modules: Vec<ModuleDef>,
}

impl Veric {
    fn load(paths: &[String]) -> Result<Self, String> {
        let mut modules = Vec::new();
        for path in paths {
            let bytes = fs::read(path)
                .map_err(|e| format!("failed to read {}: {}", path, e))?;
            let module = loader::Loader::load(&bytes)?;
            eprintln!("veric: loaded module {} from {}", module.name.0, path);
            modules.push(module);
        }
        Ok(Veric { modules })
    }

    fn verify_and_emit(&self, output_path: &str) -> Result<(), String> {
        let idx = index::Index::build(&self.modules);
        let errors = verify::Verifier::verify(&self.modules, &idx);

        if !errors.is_empty() {
            for err in &errors {
                eprintln!("veric: error in {}: {}", err.module, err.message);
            }
            return Err(format!("{} verification errors", errors.len()));
        }

        let program = emit::Emitter::emit(&self.modules, &idx);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&program)
            .map_err(|e| format!("serialization failed: {}", e))?;

        fs::write(output_path, bytes.as_ref())
            .map_err(|e| format!("failed to write {}: {}", output_path, e))?;

        eprintln!("veric: wrote {} ({} bytes, {} modules)",
            output_path, bytes.len(), self.modules.len());
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut input_paths = Vec::new();
    let mut output_path = String::from("program.rkyv");

    let mut i = 0;
    while i < args.len() {
        if args[i] == "-o" && i + 1 < args.len() {
            output_path = args[i + 1].clone();
            i += 2;
        } else {
            input_paths.push(args[i].clone());
            i += 1;
        }
    }

    if input_paths.is_empty() {
        eprintln!("usage: veric <module.rkyv>... [-o program.rkyv]");
        std::process::exit(1);
    }

    let veric = Veric::load(&input_paths).unwrap_or_else(|e| {
        eprintln!("veric: {}", e);
        std::process::exit(1);
    });

    veric.verify_and_emit(&output_path).unwrap_or_else(|e| {
        eprintln!("veric: {}", e);
        std::process::exit(1);
    });
}
