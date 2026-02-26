use regex::Regex;

pub struct TestFilter {
    pub include: Vec<(String, String)>,
    pub exclude: Vec<(String, String)>,
}

impl TestFilter {
    fn parse_patterns<'a>(patterns: impl Iterator<Item = &'a str>) -> Vec<(String, String)> {
        patterns
            .flat_map(|s| match s.split_once("::") {
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
            .collect()
    }

    pub fn new<'a>(
        include: impl Iterator<Item = &'a str>,
        exclude: impl Iterator<Item = &'a str>,
    ) -> TestFilter {
        TestFilter {
            include: Self::parse_patterns(include),
            exclude: Self::parse_patterns(exclude),
        }
    }

    #[allow(clippy::print_stderr)]
    pub fn filter_expr_match(filter_expr: &str, subject: &str) -> bool {
        if filter_expr.is_empty() {
            return true;
        }

        Regex::new(&format!("^{}$", filter_expr.replace("*", ".*"))).map_or_else(
            |e| {
                eprintln!("Failed to parse filter: {e}");
                false
            },
            |r| r.is_match(subject),
        )
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
        self.include.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_include() {
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
    fn does_not_include() {
        assert!(!test_filters(&["MyFunc::MyTest"], &[]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(!test_filters(&["MyFunc::*"], &[]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(!test_filters(&["MyFunc::"], &[]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(!test_filters(&["*::MyTest"], &[]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(!test_filters(&["::MyTest"], &[]).includes("MyOtherFunc", "MyOtherTest"));
    }

    #[test]
    fn does_exclude() {
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
    fn does_not_exclude() {
        assert!(test_filters(&[], &["MyFunc::MyTest"]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(test_filters(&[], &["MyFunc::*"]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(test_filters(&[], &["MyFunc::"]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(test_filters(&[], &["*::MyTest"]).includes("MyOtherFunc", "MyOtherTest"));
        assert!(test_filters(&[], &["::MyTest"]).includes("MyOtherFunc", "MyOtherTest"));
    }

    #[test]
    fn mixed_include_exclude() {
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
    fn mixed_include_exclude_specificities() {
        // Excludes always take precedence over includes (no specificity scoring).
        assert!(!test_filters(&["MyFunc::MyTest"], &["MyFunc::*"]).includes("MyFunc", "MyTest"));
        assert!(!test_filters(&["MyFunc::*"], &["MyFunc::MyTest"]).includes("MyFunc", "MyTest"));

        assert!(
            test_filters(&["*Func::MyTest"], &["OtherFunc::MyTest"]).includes("MyFunc", "MyTest")
        );
        assert!(
            !test_filters(&["*Func::MyTest"], &["MyFunc::MyTest"]).includes("MyFunc", "MyTest")
        );

        assert!(
            test_filters(&["MyFunc::*Test"], &["MyFunc::OtherTest"]).includes("MyFunc", "MyTest")
        );
        assert!(
            !test_filters(&["MyFunc::*Test"], &["MyFunc::MyTest"]).includes("MyFunc", "MyTest")
        );
    }

    fn test_filters(include: &[&str], exclude: &[&str]) -> TestFilter {
        TestFilter::new(include.iter().copied(), exclude.iter().copied())
    }
}
