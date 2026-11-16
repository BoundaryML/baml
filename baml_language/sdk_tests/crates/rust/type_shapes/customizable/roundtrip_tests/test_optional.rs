//! Roundtrip coverage for `baml_sdk::optional` — optional Ty variants.

use baml_sdk::optional::{
    IntOrString, OptionalContainer, Resume, SetupLinkRequest, SetupLinkRequestNext,
    round_trip_optional_container_sync, round_trip_optional_int_sync,
    round_trip_optional_resume_sync, round_trip_optional_union_sync, round_trip_resume_sync,
    round_trip_setup_link_request_next_sync, round_trip_setup_link_request_sync,
};

#[test]
fn test_optional_round_trip_optional_int() {
    assert_eq!(round_trip_optional_int_sync(Some(5)).unwrap(), Some(5));
    assert_eq!(round_trip_optional_int_sync(None).unwrap(), None);
}

#[test]
fn test_optional_round_trip_optional_resume() {
    let r = Resume {
        name: "ada".to_string(),
    };
    assert_eq!(
        round_trip_optional_resume_sync(Some(r.clone())).unwrap(),
        Some(r)
    );
    assert_eq!(round_trip_optional_resume_sync(None).unwrap(), None);
}

#[test]
fn test_optional_round_trip_optional_union() {
    assert_eq!(
        round_trip_optional_union_sync(Some(IntOrString::Int(3))).unwrap(),
        Some(IntOrString::Int(3))
    );
    assert_eq!(
        round_trip_optional_union_sync(Some(IntOrString::String("s".to_string()))).unwrap(),
        Some(IntOrString::String("s".to_string()))
    );
    assert_eq!(round_trip_optional_union_sync(None).unwrap(), None);
}

#[test]
fn test_optional_round_trip_resume() {
    let r = Resume {
        name: "grace".to_string(),
    };
    assert_eq!(round_trip_resume_sync(r.clone()).unwrap(), r);
}

#[test]
fn test_optional_round_trip_optional_container() {
    let c = OptionalContainer {
        optional_int: None,
        optional_class: Some(Resume {
            name: "x".to_string(),
        }),
        optional_union: Some(IntOrString::String("y".to_string())),
    };
    assert_eq!(round_trip_optional_container_sync(c.clone()).unwrap(), c);
}

// SDK_PARITY_LINT(skip): exercises the Rust struct constructor and struct update syntax
#[test]
fn test_optional_constructor_leaves_optional_fields_none_rust_only() {
    let request = SetupLinkRequest::new("proj_1".to_string(), "staging".to_string());
    assert_eq!(
        request,
        SetupLinkRequest {
            project_id: "proj_1".to_string(),
            name: "staging".to_string(),
            max_deployments: None,
        }
    );
    assert_eq!(
        round_trip_setup_link_request_sync(&request).unwrap(),
        request
    );
}

// SDK_PARITY_LINT(skip): exercises the Rust struct constructor and struct update syntax
#[test]
fn test_optional_constructor_with_struct_update_survives_new_optional_fields_rust_only() {
    // The same caller source, compiled against the request before and
    // after it gained `expires_in_days` and `label`: required fields go
    // through `new`, optional ones through struct update syntax.
    macro_rules! build_request {
        ($request:ident) => {
            $request {
                max_deployments: Some(5),
                ..$request::new("proj_1".to_string(), "staging".to_string())
            }
        };
    }
    let before = build_request!(SetupLinkRequest);
    assert_eq!(before.max_deployments, Some(5));
    assert_eq!(round_trip_setup_link_request_sync(&before).unwrap(), before);

    let after = build_request!(SetupLinkRequestNext);
    assert_eq!(
        after,
        SetupLinkRequestNext {
            project_id: "proj_1".to_string(),
            name: "staging".to_string(),
            max_deployments: Some(5),
            expires_in_days: None,
            label: None,
        }
    );
    assert_eq!(
        round_trip_setup_link_request_next_sync(&after).unwrap(),
        after
    );
}
