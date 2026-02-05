// Parser module using nom
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case},
    character::complete::multispace0,
    combinator::map,
    multi::separated_list1,
    sequence::{delimited, separated_pair},
    IResult, Parser,
};

fn comma_sep(input: &str) -> IResult<&str, &str> {
    delimited(multispace0, tag(","), multispace0).parse(input)
}

pub fn simple_type(input: &str) -> IResult<&str, serde_json::Value> {
    alt((
        map(
            tag_no_case("str"),
            |_s| serde_json::json!({"type" : "string"}),
        ),
        map(
            tag_no_case("int"),
            |_s| serde_json::json!({"type" : "integer"}),
        ),
        map(
            tag_no_case("float"),
            |_s| serde_json::json!({"type" : "number"}),
        ),
        map(
            tag_no_case("bool"),
            |_s| serde_json::json!({"type" : "boolean"}),
        ),
        map(
            alt((tag_no_case("None"), tag_no_case("NoneType"))),
            |_s| serde_json::json!({"type" : "null"}),
        ),
    ))
    .parse(input)
}

// TODO: this is not used, because we can't bijectively map between most python types and json
//       the same is true for other types where we lose information, like sets or dataclasses.
//       if we can preserve the information about what the original python types were, when we
//       convert back from json to python, we could add support for these fancier types.
#[allow(dead_code)]
pub fn tuple_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(
            tag_no_case("tuple["),
            separated_list1(comma_sep, type_parser),
            tag("]"),
        ),
        |vec_of_values| serde_json::json!({"type" : "array", "prefixItems" : vec_of_values, "items" : "false"}),
    ).parse(input)
}

pub fn list_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(tag_no_case("list["), type_parser, tag("]")),
        |inner| serde_json::json!({"type" : "array" , "items" : inner}),
    )
    .parse(input)
}

/// This function parses types that can be dict keys. Currently this is only strings
/// but we might want to add more types later.
pub fn dict_keys(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        tag_no_case("str"),
        |_s| serde_json::json!({"type" : "string"}),
    )
    .parse(input)
}

pub fn dict_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(
            tag_no_case("dict["),
            separated_pair(dict_keys, comma_sep, type_parser),
            tag("]"),
        ),
        |(_, inner)| serde_json::json!({"type" : "object", "additionalProperties" : inner}),
    )
    .parse(input)
}

pub fn type_parser(input: &str) -> IResult<&str, serde_json::Value> {
    alt((list_type, dict_type, simple_type)).parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_types() {
        let (remaining, result) = type_parser("str").unwrap();
        assert_eq!(remaining, "");
        assert_eq!(result, serde_json::json!({"type": "string"}));

        let (remaining, result) = type_parser("int").unwrap();
        assert_eq!(remaining, "");
        assert_eq!(result, serde_json::json!({"type": "integer"}));
    }

    #[test]
    fn test_list_of_dicts() {
        let (remaining, result) = type_parser("list[dict[str, int]]").unwrap();
        assert_eq!(remaining, "");
        assert_eq!(
            result,
            serde_json::json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": {"type": "integer"}
                }
            })
        );
    }

    #[test]
    fn test_dict_of_lists() {
        let (remaining, result) = type_parser("dict[str, list[str]]").unwrap();
        assert_eq!(remaining, "");
        assert_eq!(
            result,
            serde_json::json!({
                "type": "object",
                "additionalProperties": {
                    "type": "array",
                    "items": {"type": "string"}
                }
            })
        );
    }
}
