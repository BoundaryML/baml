#[cfg(test)]
mod tests {
    use std::{env, fs::write, io::Write, path::PathBuf};

    use anyhow::Result;
    use tempfile::NamedTempFile;

    use super::*;
    use crate::cli::dotenv::{dotenv, load_env_file, load_env_from_string};

    // Helper to create temp file with content
    fn create_env_file(content: &str) -> Result<PathBuf> {
        let mut file = NamedTempFile::new()?;
        write!(file, "{content}")?;
        Ok(file.into_temp_path().to_path_buf())
    }

    #[test]
    fn test_basic_key_value_pairs() -> Result<()> {
        let content = r#"
            KEY1=value1
            KEY2=value2
            KEY3=value3
        "#;

        let vars = load_env_from_string(content)?;

        assert_eq!(vars.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("KEY2"), Some(&"value2".to_string()));
        assert_eq!(vars.get("KEY3"), Some(&"value3".to_string()));

        Ok(())
    }

    #[test]
    fn test_quoted_values() -> Result<()> {
        let content = r#"
            SINGLE_QUOTED='single quoted value'
            DOUBLE_QUOTED="double quoted value"
            QUOTES_INSIDE="value with 'nested' quotes"
        "#;

        let vars = load_env_from_string(content)?;

        assert_eq!(
            vars.get("SINGLE_QUOTED"),
            Some(&"single quoted value".to_string())
        );
        assert_eq!(
            vars.get("DOUBLE_QUOTED"),
            Some(&"double quoted value".to_string())
        );
        assert_eq!(
            vars.get("QUOTES_INSIDE"),
            Some(&"value with 'nested' quotes".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_multiline_values() -> Result<()> {
        let content = r#"
MULTILINE="This is a 
multiline value
with three lines"

JSON='{
    "name": "test",
    "value": 123
}'

AFTER=after_multiline
        "#;

        let vars = load_env_from_string(content)?;

        assert_eq!(
            vars.get("MULTILINE"),
            Some(&"This is a \nmultiline value\nwith three lines".to_string())
        );
        assert_eq!(
            vars.get("JSON"),
            Some(&"{\n    \"name\": \"test\",\n    \"value\": 123\n}".to_string())
        );
        assert_eq!(vars.get("AFTER"), Some(&"after_multiline".to_string()));

        Ok(())
    }

    #[test]
    fn test_comments_and_empty_lines() -> Result<()> {
        let content = r#"
            # This is a comment
            KEY1=value1
            
            # Another comment
            KEY2=value2
            
            # Comment after empty line
        "#;

        let vars = load_env_from_string(content)?;

        assert_eq!(vars.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("KEY2"), Some(&"value2".to_string()));
        assert_eq!(vars.len(), 2);

        Ok(())
    }

    #[test]
    fn test_escape_sequences() -> Result<()> {
        let content = r#"
            NEWLINES="line1\nline2\nline3"
            TABS="value1\tvalue2\tvalue3"
            QUOTES="escaped \"quotes\" inside"
            BACKSLASH="double\\backslash"
        "#;

        let vars = load_env_from_string(content)?;

        assert_eq!(
            vars.get("NEWLINES"),
            Some(&"line1\nline2\nline3".to_string())
        );
        assert_eq!(
            vars.get("TABS"),
            Some(&"value1\tvalue2\tvalue3".to_string())
        );
        assert_eq!(
            vars.get("QUOTES"),
            Some(&"escaped \"quotes\" inside".to_string())
        );
        assert_eq!(
            vars.get("BACKSLASH"),
            Some(&"double\\backslash".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_variable_interpolation() -> Result<()> {
        let content = r#"
            BASE_DIR=/app
            LOGS_DIR=${BASE_DIR}/logs
            CONFIG_PATH=$BASE_DIR/config/app.conf
            NESTED_VAR=${LOGS_DIR}/debug.log
        "#;

        let vars = load_env_from_string(content)?;

        assert_eq!(vars.get("BASE_DIR"), Some(&"/app".to_string()));
        assert_eq!(vars.get("LOGS_DIR"), Some(&"/app/logs".to_string()));
        assert_eq!(
            vars.get("CONFIG_PATH"),
            Some(&"/app/config/app.conf".to_string())
        );
        assert_eq!(
            vars.get("NESTED_VAR"),
            Some(&"/app/logs/debug.log".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_system_env_variables() -> Result<()> {
        // Set a system environment variable
        env::set_var("SYSTEM_VAR", "system_value");

        let content = r#"
            LOCAL_VAR=local_value
            COMBINED_VAR=prefix_${SYSTEM_VAR}_suffix
        "#;

        let vars = load_env_from_string(content)?;

        assert_eq!(vars.get("LOCAL_VAR"), Some(&"local_value".to_string()));
        assert_eq!(
            vars.get("COMBINED_VAR"),
            Some(&"prefix_system_value_suffix".to_string())
        );

        // Clean up
        env::remove_var("SYSTEM_VAR");

        Ok(())
    }

    #[test]
    fn test_missing_variable() -> Result<()> {
        let content = r#"
            REF_MISSING=${NON_EXISTENT_VAR}
            PART_MISSING=prefix_${NON_EXISTENT_VAR}_suffix
        "#;

        let vars = load_env_from_string(content)?;

        // Missing variable references should be left as-is
        assert_eq!(
            vars.get("REF_MISSING"),
            Some(&"${NON_EXISTENT_VAR}".to_string())
        );
        assert_eq!(
            vars.get("PART_MISSING"),
            Some(&"prefix_${NON_EXISTENT_VAR}_suffix".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_complex_file() -> Result<()> {
        let content = r#"
            # Database configuration
            DB_HOST=localhost
            DB_PORT=5432
            DB_USER=postgres
            DB_PASS="complex!@#$%^&*()password"
            
            # Connection string with interpolation
            DATABASE_URL="postgres://${DB_USER}:${DB_PASS}@${DB_HOST}:${DB_PORT}/mydb"
            
            # Multiline JSON configuration
            CONFIG_JSON='{
                "server": {
                    "port": 8080,
                    "host": "0.0.0.0"
                },
                "logging": {
                    "level": "debug",
                    "file": "/var/log/app.log"
                }
            }'
            
            # Values with escape sequences
            LOG_FORMAT="time=\"%H:%M:%S\"\ttype=%t\tmessage=\"%m\""
        "#;

        let vars = load_env_from_string(content)?;

        assert_eq!(vars.get("DB_HOST"), Some(&"localhost".to_string()));
        assert_eq!(vars.get("DB_PORT"), Some(&"5432".to_string()));
        assert_eq!(vars.get("DB_USER"), Some(&"postgres".to_string()));
        assert_eq!(
            vars.get("DB_PASS"),
            Some(&"complex!@#$%^&*()password".to_string())
        );

        let expected_url = "postgres://postgres:complex!@#$%^&*()password@localhost:5432/mydb";
        assert_eq!(vars.get("DATABASE_URL"), Some(&expected_url.to_string()));

        // Check that the JSON is preserved correctly
        assert!(vars.get("CONFIG_JSON").unwrap().contains("\"server\""));
        assert!(vars.get("CONFIG_JSON").unwrap().contains("\"logging\""));

        assert_eq!(
            vars.get("LOG_FORMAT"),
            Some(&"time=\"%H:%M:%S\"\ttype=%t\tmessage=\"%m\"".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_dotenv_function() -> Result<()> {
        let content = r#"
            TEST_VAR1=from_dotenv
            TEST_VAR2=also_from_dotenv
        "#;

        // Create a temp .env file in the current directory
        let temp_dir = tempfile::tempdir()?;
        let current_dir = env::current_dir()?;

        // Change to temp directory
        env::set_current_dir(&temp_dir)?;

        // Create .env file
        std::fs::write(".env", content)?;

        // Call dotenv
        dotenv(None)?;

        // Check that variables were set in environment
        assert_eq!(env::var("TEST_VAR1").ok(), Some("from_dotenv".to_string()));
        assert_eq!(
            env::var("TEST_VAR2").ok(),
            Some("also_from_dotenv".to_string())
        );

        // Clean up and restore directory
        env::set_current_dir(current_dir)?;

        Ok(())
    }
}
