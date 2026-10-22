import json
import os
from pathlib import Path

import nobodywho
import pytest


CASES_PATH = Path(
    os.environ.get(
        key="TEST_DECISION_CASES",
        default=Path(__file__).resolve().parents[2] / "core/src/decision/cases.json",
    )
)


@pytest.fixture(scope="module")
def cases():
    return {
        case["name"]: case
        for case in json.loads(s=CASES_PATH.read_text(encoding="utf-8"))
    }


@pytest.fixture(scope="module")
def cpu_model():
    return nobodywho.Model(
        model_path=os.environ["TEST_DECISION_MODEL"], use_gpu_if_available=False
    )


@pytest.fixture(scope="module", params=[False, True], ids=["cpu", "gpu-requested"])
def model(request, cpu_model):
    if not request.param:
        return cpu_model, False
    return nobodywho.Model(model_path=os.environ["TEST_DECISION_MODEL"]), True


def check_response(response, expected, *, golden, tolerance=0.05):
    assert response["usage"] == expected["usage"]
    assert list(response["answers"]) == list(expected["answers"])
    for name, reference in expected["answers"].items():
        answer = response["answers"][name]
        assert answer["type"] == reference["type"]
        for key in ("choice", "legend"):
            if key in reference:
                assert answer[key] == reference[key]
        if "probabilities" in answer:
            probabilities = answer["probabilities"]
            assert list(probabilities) == list(reference["probabilities"])
            assert sum(probabilities.values()) == pytest.approx(expected=1.0)
            assert all(0 <= value <= 1 for value in probabilities.values())
        if golden:
            for key in ("probabilities", "confidence", "score", "noul"):
                if key in reference:
                    assert answer[key] == pytest.approx(
                        expected=reference[key], abs=tolerance
                    )
        elif "noul" in answer:
            assert 0 <= answer["noul"] <= 1
        elif "score" in answer:
            assert 0 <= answer["score"] <= len(answer["legend"]) - 1


def test_decision(model, cases):
    loaded_model, gpu_requested = model
    decisions = nobodywho.Decision(model=loaded_model, n_ctx=1024)
    for name in ("reference", "structured_json"):
        case = cases[name]
        response = decisions.decide(**case["request"])
        check_response(
            response=response, expected=case["response"], golden=not gpu_requested
        )


@pytest.mark.asyncio
async def test_decision_async(model, cases):
    loaded_model, _ = model
    synchronous = nobodywho.Decision(model=loaded_model, n_ctx=1024)
    asynchronous = nobodywho.DecisionAsync(model=loaded_model, n_ctx=1024)
    request = cases["reference"]["request"]
    expected = synchronous.decide(**request)
    result = await asynchronous.decide(**request)
    check_response(response=result, expected=expected, golden=True, tolerance=1e-4)


@pytest.mark.asyncio
async def test_decision_errors(cpu_model, cases):
    request = cases["reference"]["request"]
    decisions = nobodywho.Decision(model=cpu_model, n_ctx=1024)
    expected = decisions.decide(**request)
    with pytest.raises(expected_exception=ValueError):
        nobodywho.Decision(model=cpu_model, n_ctx=0)
    for invalid_state, invalid_questions in (
        (object(), request["questions"]),
        (request["state"], {}),
        ("context " * 2000, request["questions"]),
    ):
        with pytest.raises(expected_exception=ValueError):
            decisions.decide(state=invalid_state, questions=invalid_questions)
    asynchronous = nobodywho.DecisionAsync(model=cpu_model, n_ctx=1024)
    with pytest.raises(expected_exception=ValueError):
        await asynchronous.decide(state=request["state"], questions={})
    check_response(
        response=await asynchronous.decide(**request),
        expected=expected,
        golden=True,
        tolerance=1e-4,
    )
    normal_model = nobodywho.Model(
        model_path=os.environ["TEST_MODEL"], use_gpu_if_available=False
    )
    with pytest.raises(expected_exception=RuntimeError, match="Laya"):
        nobodywho.Decision(model=normal_model)
    check_response(
        response=decisions.decide(**request),
        expected=expected,
        golden=True,
        tolerance=1e-4,
    )
