use regex::Regex;
pub struct TestFilter {
    pub include: Vec<(String, String)>,
    pub exclude: Vec<(String, String)>,
}

impl TestFilter {
    pub fn from<'a>(
        include: impl Iterator<Item = &'a str>,
        exclude: impl Iterator<Item = &'a str>,
    ) -> TestFilter {
        TestFilter {
            include: include
                .flat_map(|s| match s.to_string().split_once("::") {
                    Some((function_match, test_match)) => {
                        vec![(function_match.to_string(), test_match.to_string())]
                    }
                    None => {
                        vec![
                            (s.to_string(), "".to_string()),
                            ("".to_string(), s.to_string()),
                        ]
                    }
                })
                .collect(),
            exclude: exclude
                .flat_map(|s| match s.to_string().split_once("::") {
                    Some((function_match, test_match)) => {
                        vec![(function_match.to_string(), test_match.to_string())]
                    }
                    None => {
                        vec![
                            (s.to_string(), "".to_string()),
                            ("".to_string(), s.to_string()),
                        ]
                    }
                })
                .collect(),
        }
    }

    pub fn filter_expr_match(filter_expr: &str, subject: &str) -> bool {
        if filter_expr.is_empty() {
            return true;
        }

        let ret = Regex::new(&format!("^{}$", filter_expr.replace("*", ".*")))
            .unwrap()
            .is_match(subject);
        ret
    }
    pub fn includes(&self, function_name: &str, test_name: &str) -> bool {
        for (func_pattern, test_pattern) in &self.exclude {
            if TestFilter::filter_expr_match(func_pattern, function_name)
                && TestFilter::filter_expr_match(test_pattern, test_name)
            {
                return false;
            }
        }
        for (func_pattern, test_pattern) in &self.include {
            if TestFilter::filter_expr_match(func_pattern, function_name)
                && TestFilter::filter_expr_match(test_pattern, test_name)
            {
                return true;
            }
        }

        // Fall-through behavior changes based on the presence of include filters:
        //
        // |                           | 0 exclude filters              | at least 1 exclude filter                       |
        // |---------------------------|--------------------------------|-------------------------------------------------|
        // | 0 include filters         | include all tests              | include all tests, except excluded              |
        // | at least 1 include filter | include only --include matches | include only --include matches, except excluded |
        // |---------------------------|--------------------------------|-------------------------------------------------|
        self.include.is_empty()
    }
}

mod filter_test {
    use super::*;

    #[test]
    pub fn does_include() {
        assert!(test_filters(&["MyFunc::MyTest"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["MyFunc::*"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["MyFunc::"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["*::MyTest"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["::MyTest"], &[]).includes("MyFunc", "MyTest"));

        assert!(test_filters(&["My*::"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["*Func::"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["My*::MyTest"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["*Func::MyTest"], &[]).includes("MyFunc", "MyTest"));

        assert!(test_filters(&["::My*"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["::*Test"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["MyFunc::My*"], &[]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["MyFunc::*Test"], &[]).includes("MyFunc", "MyTest"));

        assert!(!test_filters(&["My::"], &[]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&["Func::"], &[]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&["::My"], &[]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&["::Test"], &[]).includes("MyFunc", "MyTest"));
    }

    #[test]
    pub fn does_not_include() {
        assert!(!test_filters(&["MyFunc::MyTest"], &[]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(!test_filters(&["MyFunc::*"], &[]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(!test_filters(&["MyFunc::"], &[]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(!test_filters(&["*::MyTest"], &[]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(!test_filters(&["::MyTest"], &[]).includes("MyOtherFunc", "MyOtherTest"));
    }

    #[test]
    pub fn does_exclude() {
        assert!(!test_filters(&[], &["MyFunc::MyTest"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["MyFunc::*"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["MyFunc::"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["*::MyTest"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["::MyTest"]).includes("MyFunc", "MyTest"));

        assert!(!test_filters(&[], &["My*::"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["*Func::"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["My*::MyTest"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["*Func::MyTest"]).includes("MyFunc", "MyTest"));

        assert!(!test_filters(&[], &["::My*"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["::*Test"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["MyFunc::My*"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&[], &["MyFunc::*Test"]).includes("MyFunc", "MyTest"));

        assert!(test_filters(&[], &["My::"]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&[], &["Func::"]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&[], &["::My"]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&[], &["::Test"]).includes("MyFunc", "MyTest"));
    }

    #[test]
    pub fn does_not_exclude() {
        assert!(test_filters(&[], &["MyFunc::MyTest"]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(test_filters(&[], &["MyFunc::*"]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(test_filters(&[], &["MyFunc::"]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(test_filters(&[], &["*::MyTest"]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(test_filters(&[], &["::MyTest"]).includes("MyOtherFunc", "MyOtherTest"));
    }

    #[test]
    pub fn mixed_include_exclude() {
        // Existing test cases
        assert!(!test_filters(&["MyFunc"], &["MyTest"]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["MyFunc"], &["MyOtherTest"]).includes("MyFunc", "MyTest"));

        assert!(!test_filters(&["MyFunc::*"], &["My*"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&["MyFunc::*"], &["MyTest"]).includes("MyFunc", "MyTest"));

        assert!(!test_filters(&["*Func::*"], &["MyTest"]).includes("MyFunc", "MyTest"));
        assert!(test_filters(&["*Func::*"], &["MyOtherTest"]).includes("MyFunc", "MyTest"));

        // Case where include and exclude are the same
        assert!(
            !test_filters(&["MyFunc::MyTest"], &["MyFunc::MyTest"]).includes("MyFunc", "MyTest")
        );
    }

    #[test]
    pub fn mixed_include_exclude_specificities() {
        // Case where include is more specific than exclude
        //
        // Both openai and claude suggested that this should be the other way
        // around, but we deliberately do not implement specificity scoring,
        // primarily because it feels very hard to reason about (I got 2/3 of the way through
        // implementing it and then didn't love how hard it felt to reason about edge cases).
        //
        // It's easy to tell the user that "excludes always take precedence over
        // includes" but it's very hard to explain to the user that
        // "--include MyFunc::MyTest --exclude MyFunc::*" matches "MyFunc::MyTest"
        // but "--include MyFunc --exclude MyFunc::*" does not.
        // If we revisit this and find that it's not that hard to explain,
        // we can implement specificity scoring.
        assert!(!test_filters(&["MyFunc::MyTest"], &["MyFunc::*"]).includes("MyFunc", "MyTest"));

        // Case where exclude is more specific than include
        assert!(!test_filters(&["MyFunc::*"], &["MyFunc::MyTest"]).includes("MyFunc", "MyTest"));

        // Case with wildcard in function name
        assert!(
            test_filters(&["*Func::MyTest"], &["OtherFunc::MyTest"]).includes("MyFunc", "MyTest")
        );
        assert!(!test_filters(&["*Func::MyTest"], &["MyFunc::MyTest"]).includes("MyFunc", "MyTest"));

        // Case with wildcard in test name
        assert!(
            test_filters(&["MyFunc::*Test"], &["MyFunc::OtherTest"]).includes("MyFunc", "MyTest")
        );
        assert!(!test_filters(&["MyFunc::*Test"], &["MyFunc::MyTest"]).includes("MyFunc", "MyTest"));
    }

    fn test_filters(include: &[&str], exclude: &[&str]) -> TestFilter {
        TestFilter::from(include.iter().copied(), exclude.iter().copied())
    }
}
