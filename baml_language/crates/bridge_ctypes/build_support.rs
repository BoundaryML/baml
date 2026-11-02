pub fn allow_missing(result: std::io::Result<()>) -> std::io::Result<()> {
    match result {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::allow_missing;
    use std::io::{Error, ErrorKind};

    #[test]
    fn cleanup_tolerates_missing_directories_but_preserves_other_errors() {
        assert!(allow_missing(Ok(())).is_ok());
        assert!(allow_missing(Err(Error::from(ErrorKind::NotFound))).is_ok());
        let error = allow_missing(Err(Error::new(
            ErrorKind::PermissionDenied,
            "cleanup refused",
        )))
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "cleanup refused");
    }
}
