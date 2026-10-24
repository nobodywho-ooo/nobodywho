use super::laya::{softmax, PreparedQuestion};
use super::*;
use crate::errors::ReadError;
use serde_json::json;

#[test]
fn softmax_handles_large_and_invalid_scores() {
    let probabilities = softmax(&[1000.0, 1001.0, 999.0], 1.0).unwrap();
    assert!((probabilities.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!((probabilities[1] - 0.6652409557748218).abs() < 1e-12);
    for scores in [vec![], vec![f32::NAN], vec![f32::INFINITY]] {
        assert!(softmax(&scores, 1.0).is_err());
    }
}

#[test]
fn rejects_invalid_questions() {
    for question in [
        json!({"type": "choice", "instructions": "Choose", "criteria": {}}),
        json!({"type": "score", "instructions": "Rate", "criteria": ["low"]}),
        json!({"type": "noul", "instructions": null}),
        json!({"type": "noul", "instructions": "True?", "criteria": {"yes": null}}),
    ] {
        let question = serde_json::from_value(question).unwrap();
        assert!(PreparedQuestion::new("id", &question).is_err());
    }
    let question = DecisionQuestion::Choice {
        instructions: json!("Choose"),
        criteria: (0..256).map(|i| (i.to_string(), Value::Null)).collect(),
    };
    assert!(PreparedQuestion::new("id", &question).is_err());
    let question = DecisionQuestion::Score {
        instructions: json!("Rate"),
        criteria: (0..11).map(|i| json!(i)).collect(),
    };
    assert!(PreparedQuestion::new("id", &question).is_err());
}

fn assert_json_close(actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Number(number) if number.is_f64() => {
            let expected = number.as_f64().unwrap();
            let actual = actual.as_f64().unwrap();
            assert!(
                (actual - expected).abs() < 0.05,
                "{path}: {actual} vs {expected}"
            );
        }
        Value::Object(fields) => {
            assert_eq!(actual.as_object().unwrap().len(), fields.len(), "{path}");
            for (key, value) in fields {
                assert_json_close(&actual[key], value, &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn real_laya_matches_llama_cpp() {
    let Ok(path) = std::env::var("TEST_DECISION_MODEL") else {
        eprintln!("skipping: set TEST_DECISION_MODEL to Laya-Q8_0.gguf");
        return;
    };
    crate::test_utils::init_test_tracing();
    let model = Arc::new(crate::llm::get_model(&path, false, None, None, None).unwrap());
    assert!(matches!(
        Decision::new(model.clone(), 0),
        Err(DecisionError::InvalidRequest(_))
    ));
    let decisions = Decision::new(model.clone(), 1024).unwrap();

    #[derive(Deserialize)]
    struct Case {
        name: String,
        request: DecisionRequest,
        response: Value,
    }
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../tests/fixtures/laya/cases.json")).unwrap();
    let mut cases = cases.into_iter();
    let reference = cases.next().unwrap();
    let request = reference.request;
    let actual = decisions.decide(request.clone()).unwrap();
    assert_json_close(
        &serde_json::to_value(&actual).unwrap(),
        &reference.response,
        &reference.name,
    );
    for case in cases {
        let response = decisions
            .decide(case.request)
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        assert_json_close(
            &serde_json::to_value(response).unwrap(),
            &case.response,
            &case.name,
        );
    }

    let mut invalid = request.clone();
    invalid.state = json!("context ".repeat(2000));
    assert!(matches!(
        decisions.decide(invalid),
        Err(DecisionError::Read(ReadError::InputExceedsContext { .. }))
    ));
    let mut invalid = request.clone();
    invalid.questions.clear();
    assert!(matches!(
        decisions.decide(invalid),
        Err(DecisionError::InvalidRequest(_))
    ));
    let mut invalid = request.clone();
    invalid.state = Value::Null;
    assert!(matches!(
        decisions.decide(invalid),
        Err(DecisionError::InvalidRequest(_))
    ));
    assert_eq!(actual, decisions.decide(request.clone()).unwrap());

    let asynchronous = DecisionAsync::new(model, 1024).unwrap();
    assert_eq!(
        actual,
        futures::executor::block_on(asynchronous.decide(request)).unwrap()
    );
}

#[test]
fn real_non_decision_model_is_rejected() {
    let Some(model) = crate::test_utils::load_model_from_env("TEST_MODEL", None) else {
        return;
    };
    assert!(matches!(
        Decision::new(model, 512),
        Err(DecisionError::UnsupportedModel { .. })
    ));
}
