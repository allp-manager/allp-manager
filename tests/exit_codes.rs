use allp::domain::AllpError;

#[test]
fn public_error_categories_have_distinct_exit_codes() {
    assert_eq!(AllpError::InvalidInput("x".into()).exit_code(), 2);
    assert_eq!(AllpError::PackageNotFound("x".into()).exit_code(), 3);
    assert_eq!(AllpError::NonInteractiveSelectionRequired.exit_code(), 4);
    assert_eq!(AllpError::BackendNotDetected("x".into()).exit_code(), 5);
    assert_eq!(
        AllpError::UnsupportedOperation {
            backend: "x".into(),
            operation: "search".into()
        }
        .exit_code(),
        6
    );
    assert_eq!(
        AllpError::CommandFailed {
            backend: "x".into(),
            command: "x".into(),
            code: Some(1),
            stderr: String::new()
        }
        .exit_code(),
        7
    );
    assert_eq!(AllpError::PartialFailure("x".into()).exit_code(), 8);
    assert_eq!(AllpError::Timeout("x".into()).exit_code(), 9);
    assert_eq!(
        AllpError::Parse {
            backend: "x".into(),
            message: "x".into()
        }
        .exit_code(),
        10
    );
    assert_eq!(
        AllpError::BackendBusy {
            backend: "APT".into(),
            command: "apt-get install -- git".into(),
            code: Some(100),
            lock_path: Some("/var/lib/dpkg/lock-frontend".into()),
            holder_pid: Some(7515),
            holder_process: Some("packagekitd".into()),
        }
        .exit_code(),
        11
    );
}
