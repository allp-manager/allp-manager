use crate::{
    domain::{AllpResult, Capability, MultiOperationReport},
    operations::{maintenance, OperationContext},
};

pub fn run(context: &OperationContext<'_>) -> AllpResult<MultiOperationReport> {
    maintenance::run(context, Capability::Upgrade, "upgrade")
}
