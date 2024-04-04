// Helper file for converting a json value to a specific type.

use std::ops::Deref;

use anyhow::Result;
use serde_json::{Number, Value};

#[derive(Debug)]
enum Conditions {
    None,
    BoolStringToBool,
    YesNoStringToBool,
    BoolIntToBool,
    BoolFloatToBool,
    SingleArrayElement,
    ArrayElement(usize),
    SingleObjectKey(String),
    ObjectKey(String),
    Group(Vec<Conditions>),
}

impl std::ops::BitOr for Conditions {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Conditions::None, Conditions::None) => Conditions::None,
            (Conditions::None, rhs) => rhs,
            (lhs, Conditions::None) => lhs,
            (Conditions::Group(mut lhs), Conditions::Group(rhs)) => {
                lhs.extend(rhs);
                Conditions::Group(lhs)
            }
            (Conditions::Group(mut lhs), rhs) => {
                lhs.push(rhs);
                Conditions::Group(lhs)
            }
            (lhs, Conditions::Group(mut rhs)) => {
                rhs.insert(0, lhs);
                Conditions::Group(rhs)
            }
            (lhs, rhs) => Conditions::Group(vec![lhs, rhs]),
        }
    }
}

impl PartialEq for Conditions {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ArrayElement(l0), Self::ArrayElement(r0)) => l0 == r0,
            (Self::SingleObjectKey(l0), Self::SingleObjectKey(r0)) => l0 == r0,
            (Self::ObjectKey(l0), Self::ObjectKey(r0)) => l0 == r0,
            (Self::Group(l0), Self::Group(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl Conditions {
    fn rank(&self) -> usize {
        match self {
            Conditions::None => 0,
            // Direct primitive conversions
            Conditions::BoolStringToBool => 1,
            Conditions::BoolIntToBool => 1,
            Conditions::BoolFloatToBool => 1,

            // Indirect primitive conversions
            Conditions::YesNoStringToBool => 2,

            // Finding a value in an array
            Conditions::SingleArrayElement => 1, // We have an array, but only one element.
            Conditions::ArrayElement(_) => 3,

            // Finding a value in an object
            Conditions::SingleObjectKey(_) => 1, // We have an object, but only one key.
            Conditions::ObjectKey(_) => 3,

            // Grouping
            Conditions::Group(v) => v.iter().map(|c| c.rank()).max().unwrap_or(0),
        }
    }
}

pub fn to_bool(v: &Value) -> Result<(bool, Conditions)> {
    match v {
        Value::Bool(b) => Ok((*b, Conditions::None)),
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.len() <= 5 {
                match trimmed.to_lowercase().as_str() {
                    "true" => return Ok((true, Conditions::BoolStringToBool)),
                    "false" => return Ok((false, Conditions::BoolStringToBool)),
                    "yes" => return Ok((true, Conditions::YesNoStringToBool)),
                    "no" => return Ok((false, Conditions::YesNoStringToBool)),
                    _ => {}
                }
            }
            anyhow::bail!("Unable to convert string to bool: {:?}", s)
        }
        Value::Number(n) => {
            // if == 1 or 0
            if let Some(n) = n.as_i64() {
                if n == 1 {
                    return Ok((true, Conditions::BoolIntToBool));
                } else if n == 0 {
                    return Ok((false, Conditions::BoolIntToBool));
                }
            } else if let Some(n) = n.as_u64() {
                if n == 1 {
                    return Ok((true, Conditions::BoolIntToBool));
                } else if n == 0 {
                    return Ok((false, Conditions::BoolIntToBool));
                }
            } else if let Some(n) = n.as_f64() {
                if n == 1.0 {
                    return Ok((true, Conditions::BoolFloatToBool));
                } else if n == 0.0 {
                    return Ok((false, Conditions::BoolFloatToBool));
                }
            }
            anyhow::bail!("Unable to convert number to bool: {}", n)
        }
        Value::Array(arr) => {
            // iterate over the array, and see if any of the values fit.
            let mut parsed = arr
                .iter()
                .enumerate()
                .filter_map(|(i, v)| to_bool(v).ok().map(|b| (i, b.0, b.1)))
                .collect::<Vec<(usize, bool, Conditions)>>();

            parsed.sort_by(|a, b| a.2.rank().cmp(&b.2.rank()));
            // Only keep the highest ranked values.
            let top_rank = parsed.first().map(|v| v.2.rank()).unwrap_or(0);
            parsed.retain(|v| v.2.rank() == top_rank);

            match parsed.len() {
                0 => anyhow::bail!("Unable to convert array to bool: {:?}", arr),
                1 => {
                    let (i, b, c) = parsed.pop().unwrap();
                    if arr.len() == 1 {
                        return Ok((b, Conditions::SingleArrayElement | c));
                    } else {
                        return Ok((b, Conditions::ArrayElement(i) | c));
                    }
                }
                _ => {
                    // If we have multiple values, and they are all the same, then we can return that value.
                    let (idx, first, _) = parsed[0];
                    if parsed.iter().all(|(_, b, _)| *b == first) {
                        let mut c = Conditions::None;
                        for (_, _, cond) in parsed {
                            c = c | cond;
                        }
                        return Ok((first, Conditions::ArrayElement(idx) | c));
                    }
                    anyhow::bail!("Unable to convert array to bool: {:?}", arr);
                }
            }
        }
        Value::Null => {
            anyhow::bail!("Unable to convert null to bool");
        }
        Value::Object(m) => {
            let mut parsed = m
                .iter()
                .filter_map(|(k, v)| to_bool(v).ok().map(|b| (k.clone(), b.0, b.1)))
                .collect::<Vec<(String, bool, Conditions)>>();

            parsed.sort_by(|a, b| a.2.rank().cmp(&b.2.rank()));
            // Only keep the highest ranked values.
            let top_rank = parsed.first().map(|v| v.2.rank()).unwrap_or(0);
            parsed.retain(|v| v.2.rank() == top_rank);

            match parsed.len() {
                0 => anyhow::bail!("Unable to convert object to bool: {:?}", m),
                1 => {
                    let (i, b, c) = parsed.pop().unwrap();
                    if m.len() == 1 {
                        return Ok((b, Conditions::SingleObjectKey(i) | c));
                    } else {
                        return Ok((b, Conditions::ObjectKey(i) | c));
                    }
                }
                _ => {
                    // If we have multiple values, and they are all the same, then we can return that value.
                    let (idx, first) = parsed
                        .first()
                        .map(|(key, val, _)| (key.clone(), *val))
                        .unwrap();
                    if parsed.iter().all(|(_, b, _)| *b == first) {
                        let mut c = Conditions::None;
                        for (_, _, cond) in parsed {
                            c = c | cond;
                        }
                        return Ok((first, Conditions::SingleObjectKey(idx) | c));
                    }
                    anyhow::bail!("Unable to convert object to bool: {:?}", m);
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{to_bool, Conditions};

    macro_rules! test_bool_passes {
        ($name:ident, $value:expr, $conditions:expr, $($json:tt)+) => {
            #[test]
            pub fn $name() {
                match to_bool(&serde_json::json!($($json)+)) {
                    Ok((b, c)) => {
                        assert_eq!(b, $value);
                        assert_eq!(c, $conditions);
                    }
                    Err(e) => panic!("Failed to convert to bool: {:?}", e),
                }
            }
        };
    }

    macro_rules! test_bool_fails {
        ($name:ident, $($json:tt)+) => {
            #[test]
            pub fn $name() {
                match to_bool(&serde_json::json!($($json)+)) {
                    Ok((b, c)) => panic!("Expected to fail, but got: {:?} {:?}", b, c),
                    Err(_) => {}
                }
            }
        };
    }

    test_bool_passes!(test_bool_true, true, Conditions::None, true);
    test_bool_passes!(test_bool_false, false, Conditions::None, false);
    test_bool_passes!(
        test_bool_true_string,
        true,
        Conditions::BoolStringToBool,
        "true"
    );
    test_bool_passes!(
        test_bool_false_string,
        false,
        Conditions::BoolStringToBool,
        "false"
    );
    test_bool_passes!(test_bool_yes, true, Conditions::YesNoStringToBool, "yes");
    test_bool_passes!(test_bool_no, false, Conditions::YesNoStringToBool, "no");
    test_bool_passes!(test_bool_one, true, Conditions::BoolIntToBool, 1);
    test_bool_passes!(test_bool_zero, false, Conditions::BoolIntToBool, 0);
    test_bool_passes!(test_bool_one_float, true, Conditions::BoolFloatToBool, 1.0);
    test_bool_passes!(
        test_bool_zero_float,
        false,
        Conditions::BoolFloatToBool,
        0.0
    );
    test_bool_passes!(
        test_bool_single_array,
        true,
        Conditions::SingleArrayElement,
        [true]
    );

    test_bool_passes!(
        test_bool_array,
        true,
        Conditions::ArrayElement(0),
        [true, true]
    );
    // Array with multiple values, but all different fails
    test_bool_fails!(test_bool_array_fail, [true, false]);

    test_bool_passes!(
        test_bool_single_object,
        true,
        Conditions::SingleObjectKey("a".to_string()),
        {"a": true}
    );
    test_bool_passes!(
        test_bool_object,
        true,
        Conditions::SingleObjectKey("a".to_string()),
        {"a": true}
    );

    test_bool_fails!(
        test_bool_object_fail,
        {"a": true, "b": false}
    );

    // Test ranking
    test_bool_passes!(
        test_bool_ranking,
        false,
        Conditions::ArrayElement(4),
        ["true", 1, 1.0, "yes", false, [true], {"a": true}]
    );
}
