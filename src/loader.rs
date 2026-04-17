/// Loader — deserialize ModuleDef from rkyv bytes.

use sema_core::aski_core::{ModuleDef, ArchivedModuleDef};
use rkyv::Deserialize;
use rkyv::rancor::{Error as RkyvError, Strategy};

pub struct Loader;

impl Loader {
    pub fn load(bytes: &[u8]) -> Result<ModuleDef, String> {
        let archived: &ArchivedModuleDef = unsafe {
            rkyv::access_unchecked::<ArchivedModuleDef>(bytes)
        };
        let module: ModuleDef = archived.deserialize(
            Strategy::<_, RkyvError>::wrap(&mut rkyv::de::Pool::new())
        ).map_err(|e| format!("deserialization failed: {}", e))?;
        Ok(module)
    }
}
