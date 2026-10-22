extends Node
# Tests for NobodyWhoDecision. Model-backed (needs TEST_DECISION_MODEL).
#
# Run: TEST_DECISION_MODEL=/path/to/Laya-Q8_0.gguf \
#      nix shell nixpkgs#godot_4 --command godot --headless --path .

const REFERENCE_STATE := "Customer message: I was charged twice for my order last week and nobody has replied."
const REFERENCE_QUESTIONS := {
	"route": {
		"type": "choice",
		"instructions": "Choose the best team for this customer message.",
		"criteria": {
			"billing": "payments, charges, refunds, invoices",
			"shipping": "delivery, tracking, lost or late parcels",
			"technical": "bugs, errors, login problems",
		},
	},
	"angry": {"type": "noul", "instructions": "Is the customer angry?"},
	"urgency": {
		"type": "score",
		"instructions": "How urgent is this?",
		"criteria": ["can wait", "this week", "today", "right now"],
	},
}

var _model: String

func run(runner: Node) -> void:
	_model = OS.get_environment("TEST_DECISION_MODEL")
	if _model.is_empty():
		print("SKIP: decision_test needs TEST_DECISION_MODEL env var (e.g. Laya-Q8_0.gguf)")
		return
	await _test_decide(runner)
	await _test_errors(runner)

func _test_decide(runner: Node) -> void:
	var decision = await NobodyWhoDecision.create(_model, {})
	if decision == null:
		runner.fail("decision: could not create handle (check TEST_DECISION_MODEL)")
		return
	var response = await decision.decide(REFERENCE_STATE, REFERENCE_QUESTIONS)
	if response == null:
		runner.fail("decision: decide returned null")
		return
	var answers: Dictionary = response["answers"]
	if answers.size() != REFERENCE_QUESTIONS.size():
		runner.fail("decision: expected %d answers, got %s" % [REFERENCE_QUESTIONS.size(), str(answers.keys())])
		return

	var route: Dictionary = answers["route"]
	if route["type"] != "choice" or not REFERENCE_QUESTIONS["route"]["criteria"].has(route["choice"]):
		runner.fail("decision: route answer is not a valid choice: %s" % str(route))
		return
	var probabilities: Dictionary = route["probabilities"]
	var total := 0.0
	for value in probabilities.values():
		if value < 0.0 or value > 1.0:
			runner.fail("decision: probability out of range: %s" % str(value))
			return
		total += value
	if abs(total - 1.0) > 0.01:
		runner.fail("decision: probabilities sum to %f, not 1" % total)
		return

	var angry: Dictionary = answers["angry"]
	if angry["type"] != "noul" or angry["noul"] < 0.0 or angry["noul"] > 1.0:
		runner.fail("decision: angry answer is not a valid noul: %s" % str(angry))
		return

	var urgency: Dictionary = answers["urgency"]
	if urgency["type"] != "score" or urgency["score"] < 0.0 or urgency["score"] > urgency["legend"].size() - 1:
		runner.fail("decision: urgency answer is not a valid score: %s" % str(urgency))
		return

	var usage: Dictionary = response["usage"]
	if usage["input_tokens"] <= 0 or usage["output_tokens"] < 0:
		runner.fail("decision: usage looks wrong: %s" % str(usage))
		return
	runner.ok("decision: choice/noul/score answers with probabilities and usage")
	decision = null

func _test_errors(runner: Node) -> void:
	var decision = await NobodyWhoDecision.create(_model, {"n_ctx": 1024})
	if decision == null:
		runner.fail("decision: errors: could not create handle")
		return

	var empty_questions = await decision.decide(REFERENCE_STATE, {})
	if empty_questions == null:
		runner.ok("decision: empty questions rejected")
	else:
		runner.fail("decision: empty questions were accepted")

	var bad_questions = await decision.decide(REFERENCE_STATE, {"x": {"type": "nonsense"}})
	if bad_questions == null:
		runner.ok("decision: unknown question type rejected")
	else:
		runner.fail("decision: unknown question type was accepted")

	var bad_config = await NobodyWhoDecision.create(_model, {"n_ctx": 0})
	if bad_config == null:
		runner.ok("decision: zero n_ctx rejected")
	else:
		runner.fail("decision: zero n_ctx was accepted")

	var bad_key = await NobodyWhoDecision.create(_model, {"nope": 1})
	if bad_key == null:
		runner.ok("decision: unknown config key rejected")
	else:
		runner.fail("decision: unknown config key was accepted")
	decision = null
