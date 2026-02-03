// Parser module using nom
use nom::{
    branch::alt,
    bytes::{complete::tag, complete::tag_no_case},
    character::streaming::multispace0,
    combinator::map,
    multi::separated_list1,
    sequence::{delimited, separated_pair},
    IResult,
};

fn comma_sep(input: &str) -> IResult<&str, &str> {
    delimited(multispace0, tag(","), multispace0)(input)
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
    ))(input)
}

pub fn tuple_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(
            tag_no_case("tuple["),
            separated_list1(comma_sep, type_parser),
            tag("]"),
        ),
        |vec_of_values| serde_json::json!({"type" : "array", "prefixItems" : vec_of_values, "items" : "false"}),
    )(input)
}

pub fn list_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(tag_no_case("list["), type_parser, tag("]")),
        |inner| serde_json::json!({"type" : "array" , "items" : inner}),
    )(input)
}

/// This function parses types that can be dict keys. Currently this is only strings
/// but we might want to add more types later.
pub fn dict_keys(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        tag_no_case("str"),
        |_s| serde_json::json!({"type" : "string"}),
    )(input)
}

pub fn dict_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(
            tag_no_case("dict["),
            separated_pair(dict_keys, comma_sep, type_parser),
            tag("]"),
        ),
        |(_, inner)| serde_json::json!({"type" : "object", "additionalProperties" : inner}),
    )(input)
}

pub fn type_parser(input: &str) -> IResult<&str, serde_json::Value> {
    alt((list_type, dict_type, tuple_type, simple_type))(input)
}
