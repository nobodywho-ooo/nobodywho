// Parser module using nom
use nom::{
    branch::alt,
    bytes::streaming::tag,
    character::streaming::multispace0,
    combinator::map,
    sequence::{delimited, separated_pair},
    IResult,
};

fn comma_sep(input: &str) -> IResult<&str, &str> {
    delimited(multispace0, tag(","), multispace0)(input)
}

pub fn simple_type(input: &str) -> IResult<&str, serde_json::Value> {
    alt((
        map(tag("str"), |_s| serde_json::json!({"type" : "string"})),
        map(tag("int"), |_s| serde_json::json!({"type" : "integer"})),
        map(tag("float"), |_s| serde_json::json!({"type" : "number"})),
        map(tag("bool"), |_s| serde_json::json!({"type" : "boolean"})),
        map(tag("list"), |_s| serde_json::json!({"type" : "array"})),
        map(tag("dict"), |_s| serde_json::json!({"type" : "object"})),
    ))(input)
}

pub fn list_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(tag("list["), type_parser, tag("]")),
        |inner| serde_json::json!({"type" : "array" , "items" : inner}),
    )(input)
}

pub fn dict_keys(input: &str) -> IResult<&str, serde_json::Value> {
    map(tag("str"), |_s| serde_json::json!({"type" : "string"}))(input)
}

pub fn dict_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(
            tag("dict["),
            separated_pair(dict_keys, comma_sep, type_parser),
            tag("]"),
        ),
        |(_, inner)| serde_json::json!({"type" : "object", "additionalProperties" : inner}),
    )(input)
}

pub fn type_parser(input: &str) -> IResult<&str, serde_json::Value> {
    alt((list_type, dict_type, simple_type))(input)
}
