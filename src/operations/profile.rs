use crate::{domain::AllpResult, operations::OperationContext, profiles};
use std::path::Path;

pub fn save(context: &OperationContext<'_>, name: &str) -> AllpResult<()> {
    profiles::save_current(context, name).map(|_| ())
}

pub fn list(context: &OperationContext<'_>) -> AllpResult<()> {
    profiles::list(context.config_dir, context.renderer).map(|_| ())
}

pub fn show(context: &OperationContext<'_>, name: &str) -> AllpResult<()> {
    profiles::show(context.config_dir, name, context.renderer).map(|_| ())
}

pub fn install(context: &OperationContext<'_>, name: &str) -> AllpResult<()> {
    profiles::install(context, name)
}

pub fn export(context: &OperationContext<'_>, name: &str, path: &Path) -> AllpResult<()> {
    profiles::export(context.config_dir, name, path, context.renderer)
}

pub fn import(context: &OperationContext<'_>, path: &Path, name: Option<&str>) -> AllpResult<()> {
    profiles::import(context.config_dir, path, name, context.renderer).map(|_| ())
}
