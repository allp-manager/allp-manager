use crate::{
    domain::{ExecutionPlan, NativeCommand, PrivilegeRequirement, RuntimePrivilegeContext},
    execution::privilege::runtime_context,
};
use std::ffi::OsStr;

pub fn render_execution_plan(plan: &ExecutionPlan) -> String {
    render_execution_plan_with_context(plan, &runtime_context())
}

pub fn render_execution_plan_with_context(
    plan: &ExecutionPlan,
    context: &RuntimePrivilegeContext,
) -> String {
    let native = render_native_command(&plan.command);
    match plan.privilege {
        PrivilegeRequirement::RootRequired if !context.is_root() => format!("sudo -- {native}"),
        PrivilegeRequirement::OriginalUserRequired if context.is_root() => {
            if let Some(user) = context.original_user() {
                format!("sudo -u {} -- {native}", quote(OsStr::new(&user.name)))
            } else {
                format!("run as original user: {native}")
            }
        }
        _ => native,
    }
}

pub fn render_native_command(command: &NativeCommand) -> String {
    let mut parts = vec![quote(command.program.as_os_str())];
    parts.extend(command.args.iter().map(|arg| quote(arg.as_os_str())));
    parts.join(" ")
}

pub fn render_native_argv(command: &NativeCommand) -> String {
    let mut values = vec![escape_argv_value(command.program.as_os_str())];
    values.extend(
        command
            .args
            .iter()
            .map(|arg| escape_argv_value(arg.as_os_str())),
    );
    format!("[{}]", values.join(", "))
}

fn escape_argv_value(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    format!("\"{}\"", value.escape_debug())
}

fn quote(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._/:=@+".contains(character))
    {
        return value.into_owned();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::{render_execution_plan_with_context, render_native_argv, render_native_command};
    use crate::domain::{
        ExecutionPlan, NativeCommand, OperationKind, OriginalUser, PrivilegeRequirement,
        RuntimePrivilegeContext,
    };

    #[test]
    fn renders_arguments_without_using_a_shell() {
        let command = NativeCommand::new("apt-get").args(["install", "name with spaces"]);
        assert_eq!(
            render_native_command(&command),
            "apt-get install 'name with spaces'"
        );
        assert_eq!(
            render_native_argv(&command),
            "[\"apt-get\", \"install\", \"name with spaces\"]"
        );
    }

    #[test]
    fn normal_user_root_plan_renders_sudo_child_only() {
        let plan = plan(PrivilegeRequirement::RootRequired);

        assert_eq!(
            render_execution_plan_with_context(&plan, &RuntimePrivilegeContext::NormalUser),
            "sudo -- /usr/bin/apt-get update"
        );
    }

    #[test]
    fn root_plan_does_not_render_nested_sudo() {
        let plan = plan(PrivilegeRequirement::RootRequired);

        assert_eq!(
            render_execution_plan_with_context(&plan, &RuntimePrivilegeContext::RootDirect),
            "/usr/bin/apt-get update"
        );
    }

    #[test]
    fn sudo_root_user_scoped_plan_renders_original_user() {
        let plan = plan(PrivilegeRequirement::OriginalUserRequired);
        let context = RuntimePrivilegeContext::SudoRootWithOriginalUser(OriginalUser {
            name: "alice".to_owned(),
            uid: Some(1000),
            gid: Some(1000),
        });

        assert_eq!(
            render_execution_plan_with_context(&plan, &context),
            "sudo -u alice -- /usr/bin/apt-get update"
        );
    }

    fn plan(privilege: PrivilegeRequirement) -> ExecutionPlan {
        ExecutionPlan {
            backend_id: "example".to_owned(),
            backend_name: "Example".to_owned(),
            operation: OperationKind::Update,
            action: "Test".to_owned(),
            package_id: None,
            source: None,
            scope: None,
            details: Vec::new(),
            command: NativeCommand::new("/usr/bin/apt-get").arg("update"),
            privilege,
            requires_root: privilege == PrivilegeRequirement::RootRequired,
            interactive: true,
        }
    }
}
