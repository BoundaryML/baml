// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::sync_client::B;
use baml_client::types::*;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_literals() {
        let result = B
            .TestStringLiterals
            .call("test string literals")
            .expect("Failed to call TestStringLiterals");

        // Verify string literal values
        assert!(
            matches!(result.status, Union3KactiveOrKinactiveOrKpending::Kactive),
            "Expected status to be 'active', got {:?}",
            result.status
        );
        assert!(
            matches!(result.environment, Union3KdevOrKprodOrKstaging::Kprod),
            "Expected environment to be 'prod', got {:?}",
            result.environment
        );
        assert!(
            matches!(result.method, Union4KDELETEOrKGETOrKPOSTOrKPUT::KPOST),
            "Expected method to be 'POST', got {:?}",
            result.method
        );
    }

    #[test]
    fn test_integer_literals() {
        let result = B
            .TestIntegerLiterals
            .call("test integer literals")
            .expect("Failed to call TestIntegerLiterals");

        // Verify integer literal values
        assert!(
            matches!(
                result.priority,
                Union5IntK1OrIntK2OrIntK3OrIntK4OrIntK5::IntK3
            ),
            "Expected priority to be 3, got {:?}",
            result.priority
        );
        assert!(
            matches!(
                result.httpStatus,
                Union5IntK200OrIntK201OrIntK400OrIntK404OrIntK500::IntK201
            ),
            "Expected httpStatus to be 201, got {:?}",
            result.httpStatus
        );
        assert!(
            matches!(result.maxRetries, Union4IntK0OrIntK1OrIntK3OrIntK5::IntK3),
            "Expected maxRetries to be 3, got {:?}",
            result.maxRetries
        );
    }

    #[test]
    fn test_boolean_literals() {
        let result = B
            .TestBooleanLiterals
            .call("test boolean literals")
            .expect("Failed to call TestBooleanLiterals");

        // Verify boolean literal values
        assert!(
            result.alwaysTrue,
            "Expected alwaysTrue to be true, got false"
        );
        assert!(
            !result.alwaysFalse,
            "Expected alwaysFalse to be false, got true"
        );
        assert!(
            matches!(result.eitherBool, Union2BoolKFalseOrBoolKTrue::BoolKTrue),
            "Expected eitherBool to be true, got {:?}",
            result.eitherBool
        );
    }

    #[test]
    fn test_mixed_literals() {
        let result = B
            .TestMixedLiterals
            .call("test mixed literals")
            .expect("Failed to call TestMixedLiterals");

        // Verify mixed literal values
        assert_eq!(
            result.id, 12345,
            "Expected id to be 12345, got {}",
            result.id
        );
        assert!(
            matches!(result.r#type, Union3KadminOrKguestOrKuser::Kadmin),
            "Expected type to be 'admin', got {:?}",
            result.r#type
        );
        assert!(
            matches!(result.level, Union3IntK1OrIntK2OrIntK3::IntK2),
            "Expected level to be 2, got {:?}",
            result.level
        );
        assert!(
            matches!(result.isActive, Union2BoolKFalseOrBoolKTrue::BoolKTrue),
            "Expected isActive to be true, got {:?}",
            result.isActive
        );
        assert!(
            matches!(result.apiVersion, Union3Kv1OrKv2OrKv3::Kv2),
            "Expected apiVersion to be 'v2', got {:?}",
            result.apiVersion
        );
    }

    #[test]
    fn test_complex_literals() {
        let result = B
            .TestComplexLiterals
            .call("test complex literals")
            .expect("Failed to call TestComplexLiterals");

        // Verify complex literal values
        assert!(
            matches!(
                result.state,
                Union4KarchivedOrKdeletedOrKdraftOrKpublished::Kpublished
            ),
            "Expected state to be 'published', got {:?}",
            result.state
        );
        assert!(
            matches!(
                result.retryCount,
                Union7IntK0OrIntK1OrIntK13OrIntK2OrIntK3OrIntK5OrIntK8::IntK5
            ),
            "Expected retryCount to be 5, got {:?}",
            result.retryCount
        );
        assert!(
            matches!(result.response, Union3KerrorOrKsuccessOrKtimeout::Ksuccess),
            "Expected response to be 'success', got {:?}",
            result.response
        );
        assert_eq!(
            result.flags.len(),
            3,
            "Expected flags length 3, got {}",
            result.flags.len()
        );
        assert_eq!(
            result.codes.len(),
            3,
            "Expected codes length 3, got {}",
            result.codes.len()
        );
    }
}
