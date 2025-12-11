extends NobodyWhoChat

@onready var potion_embeddings = await generate_potion_embeddings()
var selected_potion = null

func _ready() -> void:
	start_worker()

func _on_response_updated(new_token: String) -> void:
	%SpeechBubbleLabel.text += new_token.replace("\n", "")

func _on_response_finished(response: String) -> void:
	# re-enable text input
	%TextEdit.editable = true
	%TextEdit.text = ""

func _on_send_button_pressed() -> void:
	await user_submitted_text()

func _on_text_edit_text_submitted(new_text: String) -> void:
	await user_submitted_text()

func user_submitted_text():
	# disable the input box
	%TextEdit.editable = false
	
	# fetch the text
	var text = %TextEdit.text
	
	# reset the speech bubble
	%SpeechBubbleLabel.text = ""
	
	# send user text to the llm
	say(%TextEdit.text)
	
	# test if they asked to buy a potion
	var potion = await match_sentence(%TextEdit.text)
	if potion != null:
		confirm_buy_potion(potion)

func generate_potion_embeddings():
	# generate embeddings for a few example sentences for each potion
	# these will be what we use to detect if the user has asked to buy a potion
	var sentences = {
		"health_potion": [
			"I'd like to buy the potion of minor healing.",
			"I'd like to buy the health potion",
			"I'd like to buy the potion in the round bottle.",
			"Give me the health potion.",
			"Buy the red potion."
		],
		"mana_potion": [
			"Give me the blue potion.",
			"I'll get that mana potion.",
			"I'd like the potion in the triangle flask.",
			"I will purchase the potion in the triangle-shaped flask"
		],
		"strength_potion": [
			"I'm gonna buy the strength potion.",
			"Give me the orange potion.",
			"I'd like the one in the test tube",
			"I will purchase the potion that gives extra damage"
		]
	}
	var embeddings = {"health_potion": [], "mana_potion": [], "strength_potion": []}
	for potion in sentences:
		for sentence in sentences[potion]:
			embeddings[potion].append(await %NobodyWhoEmbedding.embed(sentence))
	return embeddings

func match_sentence(sentence: String):
	# test if the user asked to buy a potion
	# returns the name of the potion if yes
	# returns null otherwise
	var max_similarity = 0
	var most_similar = null
	var input_embed = await %NobodyWhoEmbedding.embed(sentence)
	for potion in potion_embeddings:
		for embedding in potion_embeddings[potion]:
			var similarity = %NobodyWhoEmbedding.cosine_similarity(input_embed, embedding)
			if similarity > max_similarity:
				most_similar = potion
				max_similarity = similarity
	var threshold = 0.85
	if max_similarity > threshold:
		return most_similar
	return null

func confirm_buy_potion(potion: String):
	selected_potion = potion
	if potion == "health_potion":
		%ConfirmLabel.text = "Buy health potion for 3 gold?"
	elif potion == "mana_potion":
		%ConfirmLabel.text = "Buy mana potion for 3 gold?"
	elif potion == "strength_potion":
		%ConfirmLabel.text = "Buy strength potion for 3 gold?"
	else:
		assert(false) # unreachable, if we're ever here, it's a bug.
	%ConfirmBox.visible = true

func _on_yes_button_pressed() -> void:
	if selected_potion == "health_potion":
		%HealthPotionSprite.visible = false
	elif selected_potion == "mana_potion":
		%ManaPotionSprite.visible = false
	elif selected_potion == "strength_potion":
		%StrengthPotionSprite.visible = false
	
	%SpeechBubbleLabel.text = ""
	%ConfirmBox.visible = false

	say("*user has confirmed buying a " + selected_potion + "*")
	selected_potion = null

func _on_no_button_pressed() -> void:
	%ConfirmBox.visible = false
	selected_potion = null
