use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while},
    character::complete::{alphanumeric1, multispace0},
    combinator::{map, recognize},
    multi::separated_list1,
    sequence::{delimited, preceded, separated_pair},
    IResult, Parser,
};

fn comma_sep(input: &str) -> IResult<&str, &str> {
    delimited(multispace0, tag(","), multispace0).parse(input)
}

fn simple_type(input: &str) -> IResult<&str, serde_json::Value> {
    alt((
        map(
            tag_no_case("int"),
            |_s| serde_json::json!({"type" : "integer"}),
        ),
        map(
            tag_no_case("double"),
            |_s| serde_json::json!({"type" : "number"}),
        ),
        map(
            tag_no_case("String"),
            |_s| serde_json::json!({"type" : "string"}),
        ),
        map(
            tag_no_case("bool"),
            |_s| serde_json::json!({"type" : "boolean"}),
        ),
        map(
            tag_no_case("DateTime"),
            |_s| serde_json::json!({"type" : "string", "format" : "date-time"}),
        ),
    ))
    .parse(input)
}

fn list_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(tag_no_case("List<"), type_parser, tag(">")),
        |inner| serde_json::json!({"type" : "array", "items" : inner}),
    )
    .parse(input)
}

fn _set_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(tag_no_case("Set<"), type_parser, tag(">")),
        |inner| serde_json::json!({"type" : "array", "items" : inner, "uniqueItems" : "true"} ),
    )
    .parse(input)
}

fn _map_key(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        tag_no_case("String"),
        |_s| serde_json::json!({"type" : "string"}),
    )
    .parse(input)
}

fn _map_type(input: &str) -> IResult<&str, serde_json::Value> {
    map(
        delimited(
            tag_no_case("Map<"),
            separated_pair(_map_key, comma_sep, type_parser),
            tag(">"),
        ),
        |(_, inner)| serde_json::json!({"type" : "object", "additionalProperties" : inner}),
    )
    .parse(input)
}

fn type_parser(input: &str) -> IResult<&str, serde_json::Value> {
    delimited(multispace0, alt((list_type, simple_type)), multispace0).parse(input)
}

pub(crate) fn return_type_parser(input: &str) -> IResult<&str, &str> {
    delimited(
        multispace0,
        alt((tag_no_case("Future<String>"), tag_no_case("String"))),
        multispace0,
    )
    .parse(input)
}

/// Parses a valid Dart identifier/parameter name
/// Examples: foo, _bar, camelCase, snake_case, name123   
fn parameter_name(input: &str) -> IResult<&str, &str> {
    recognize(nom::multi::many0(alt((alphanumeric1, tag("_"))))).parse(input)
}

fn parameter_parser(input: &str) -> IResult<&str, (&str, serde_json::Value)> {
    map(
        preceded(tag("required").and(multispace0), type_parser)
            .and(preceded(multispace0, parameter_name)),
        |(param_type, param_name)| (param_name, param_type),
    )
    .parse(input)
}

pub(crate) fn runtime_type_parser(
    input: &str,
) -> IResult<&str, (Vec<(&str, serde_json::Value)>, &str)> {
    alt((
        map(tag("()"), |_s| Vec::new()),
        delimited(
            tag("({").and(multispace0),
            separated_list1(comma_sep, parameter_parser),
            tag("})"),
        ),
    ))
    .and(preceded(multispace0.and(tag("=>")), take_while(|_b| true)))
    .parse(input)
}
